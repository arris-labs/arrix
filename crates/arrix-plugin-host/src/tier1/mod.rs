//! Tier 1 (docs/PLUGINS.md §Three tiers, ADR-0035): a plugin as a
//! WebAssembly component of the world, run in wasmtime. Its feature types
//! are adapted onto the registry's `FeatureType` as a Tier 0 plugin's are.
//! Each call runs in a fresh store from the plugin's one `InstancePre`,
//! with the world's imports and nothing else, its fuel and its memory
//! limit. Native only.
//!
//! The component speaks the world through the wire's codec ([`value`]),
//! so its kernel calls are served by Tier 2's `serve::kernel_call` on the
//! evaluating feature's kernel. That kernel is borrowed and a store's data
//! is `'static`, so the component runs on a scoped thread of its own and
//! asks for each kernel call over a channel, the evaluator's thread
//! answering, as a Tier 2 process asks over its pipe.

mod value;

use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};

use arrix_core::Diagnostic;
use arrix_doc::{FeatureArgs, FeatureType, FeatureTypeId, Registry, TypeOutput};
use arrix_plugin_api::{API_VERSION, ChoiceValue, FeatureTypeSpec, Kernel};
use serde_json::{Map, Value, json};
use wasmtime::component::{Component, InstancePre, Linker, ResourceType, Val};
use wasmtime::{Config, Engine, ResourceLimiter, Store, Trap};

use crate::manifest::Manifest;
use crate::tier0::{HostError, check, error, register_types};
use crate::tier2::process::{CallError, Served};
use crate::tier2::{MANIFEST, serve};
use crate::wire::rpc::{Request, RpcError};
use crate::wire::*;
use value::{BodyRep, from_json, to_json};

/// The fuel of one call when the manifest does not raise it (ADR-0035 §5).
pub const DEFAULT_FUEL: u64 = 1 << 30;

/// A store's linear memory, in bytes, and table elements (ADR-0035 §5).
const MEMORY: usize = 256 << 20;
const TABLE: usize = 100_000;

/// The engine every Tier 1 plugin compiles on.
fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.consume_fuel(true);
        config.cranelift_nan_canonicalization(true);
        Engine::new(&config).expect("the Tier 1 engine's configuration is valid")
    })
}

fn interface(name: &str) -> String {
    format!("arrix:plugin/{name}@{API_VERSION}")
}

/// A call's channel to the evaluator's thread, which serves its kernel.
struct Calls {
    ask: Sender<Ask>,
    answers: Receiver<Served>,
}

/// What the component's thread sends the evaluator's.
enum Ask {
    Kernel(Request),
    Done(Result<Value, Ended>),
}

/// Why a call ended without its result.
enum Ended {
    /// A kernel call that ended it (refused, malformed or interrupted).
    Call(CallError),
    Fuel,
    Memory,
    Trap(String),
}

/// A store's data: one call's channel, and what a trap needs explained.
struct State {
    calls: Option<Calls>,
    ended: Option<CallError>,
    past_limit: bool,
}

impl ResourceLimiter for State {
    fn memory_growing(
        &mut self,
        _: usize,
        desired: usize,
        _: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.past_limit |= desired > MEMORY;
        Ok(desired <= MEMORY)
    }

    fn table_growing(
        &mut self,
        _: usize,
        desired: usize,
        _: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.past_limit |= desired > TABLE;
        Ok(desired <= TABLE)
    }
}

/// One Tier 1 plugin, shared by its feature types.
struct Plugin {
    manifest: Manifest,
    pre: InstancePre<State>,
    fuel: u64,
}

impl Plugin {
    fn id(&self) -> String {
        self.manifest.id.to_string()
    }

    /// Calls the `feature` export `func` with `args` (the wire's params by
    /// name) in a fresh store, its kernel calls asked over `calls`.
    fn call(
        &self,
        func: &str,
        args: &Map<String, Value>,
        calls: Option<Calls>,
    ) -> Result<Value, Ended> {
        let mut store = Store::new(
            engine(),
            State {
                calls,
                ended: None,
                past_limit: false,
            },
        );
        store.limiter(|s| s);
        store.set_fuel(self.fuel).expect("fuel is configured");
        let run = |store: &mut Store<State>| -> Result<Value, wasmtime::Error> {
            let instance = self.pre.instantiate(&mut *store)?;
            let feature = instance
                .get_export_index(&mut *store, None, &interface("feature"))
                .ok_or_else(|| wasmtime::format_err!("no `feature` export"))?;
            let f = instance
                .get_export_index(&mut *store, Some(&feature), func)
                .and_then(|i| instance.get_func(&mut *store, i))
                .ok_or_else(|| wasmtime::format_err!("no `feature.{func}` export"))?;
            let ty = f.ty(&*store);
            let mut params = Vec::new();
            for (name, ty) in ty.params() {
                let v = args.get(name).unwrap_or(&Value::Null);
                params.push(from_json(v, &ty, store).map_err(|e| wasmtime::format_err!("{e}"))?);
            }
            let mut results = vec![Val::Bool(false); ty.results().len()];
            f.call(&mut *store, &params, &mut results)?;
            to_json(&results[0], store).map_err(|e| wasmtime::Error::new(Protocol(e)))
        };
        run(&mut store).map_err(|e| {
            let state = store.data_mut();
            if let Some(Protocol(w)) = e.downcast_ref::<Protocol>() {
                Ended::Call(CallError::Protocol(format!("its result: {w}")))
            } else if let Some(c) = state.ended.take() {
                Ended::Call(c)
            } else if e.downcast_ref::<Trap>() == Some(&Trap::OutOfFuel) {
                Ended::Fuel
            } else if state.past_limit {
                Ended::Memory
            } else {
                Ended::Trap(format!("{e:#}"))
            }
        })
    }
}

/// A value the component handed the host that the codec refuses.
#[derive(Debug)]
struct Protocol(WireError);

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for Protocol {}

impl Ended {
    fn diagnostic(&self, plugin: &str, during: &str) -> Diagnostic {
        let (code, what) = match self {
            Ended::Call(c) => return c.diagnostic(plugin, during),
            Ended::Fuel => ("plugin.fuel", "ran out of fuel".to_owned()),
            Ended::Memory => (
                "plugin.memory",
                format!("grew past its {} MiB of memory", MEMORY >> 20),
            ),
            Ended::Trap(why) => ("plugin.trap", format!("trapped: {why}")),
        };
        error(code, format!("plugin {plugin} {what} ({during})"))
    }
}

/// The world's imports: the `body` resource, and each kernel function
/// asking the evaluator's thread to serve it.
fn linker() -> Result<Linker<State>, wasmtime::Error> {
    let mut linker = Linker::new(engine());
    linker.instance(&interface("types"))?;
    let mut kernel = linker.instance(&interface("kernel"))?;
    kernel.resource("body", ResourceType::host::<BodyRep>(), |_, _| Ok(()))?;
    for m in METHODS
        .iter()
        .filter(|m| m.direction == Direction::PluginToHost)
    {
        let name = m.name.strip_prefix("kernel.").expect("a kernel method");
        let method = m.name;
        kernel.func_new(name, move |mut store, ty, params, results| {
            let served = ask(&mut store, method, &ty, params);
            let answer = served.and_then(|v| {
                let ty = ty
                    .results()
                    .next()
                    .expect("a kernel function returns a result");
                from_json(&v, &ty, &mut store).map_err(|e| CallError::Protocol(e.to_string()))
            });
            match answer {
                Ok(v) => {
                    results[0] = v;
                    Ok(())
                }
                Err(c) => {
                    store.data_mut().ended = Some(c);
                    wasmtime::bail!("the kernel call `{method}` ended the call")
                }
            }
        })?;
    }
    Ok(linker)
}

/// One kernel call, its params put on the wire and served by the
/// evaluator's thread.
fn ask(
    store: &mut wasmtime::StoreContextMut<'_, State>,
    method: &str,
    ty: &wasmtime::component::types::ComponentFunc,
    params: &[Val],
) -> Result<Value, CallError> {
    let mut args = Map::new();
    for ((name, _), v) in ty.params().zip(params) {
        let v = to_json(v, store).map_err(|e| CallError::Protocol(format!("`{method}`: {e}")))?;
        args.insert(name.to_owned(), v);
    }
    let Some(calls) = &store.data().calls else {
        return Err(CallError::Capability(format!(
            "it called `{method}` while describing"
        )));
    };
    let request = Request {
        id: None,
        method: method.to_owned(),
        params: Value::Object(args),
    };
    let gone = || CallError::Crash("the host stopped serving".to_owned());
    calls.ask.send(Ask::Kernel(request)).map_err(|_| gone())?;
    match calls.answers.recv().map_err(|_| gone())? {
        Served::Answer(Ok(v)) => Ok(v),
        Served::Answer(Err(RpcError { message, .. })) => {
            Err(CallError::Protocol(format!("`{method}`: {message}")))
        }
        Served::End(c) => Err(c),
    }
}

/// One feature type of a Tier 1 plugin.
struct Tier1Type {
    spec: FeatureTypeSpec,
    version: String,
    plugin: Arc<Plugin>,
}

impl FeatureType for Tier1Type {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.spec
    }

    /// The component's `evaluate` in a store of its own, on a thread of
    /// its own, its kernel calls served here on `kernel`.
    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        let plugin = &self.plugin;
        let during = format!("evaluating {}", self.spec.id);
        let protocol = |e: WireError| error("plugin.protocol", format!("{during}: {e}"));
        let choices: Vec<_> = args
            .choices
            .iter()
            .map(|(name, value)| ChoiceValue {
                name: name.clone(),
                value: value.clone(),
            })
            .collect();
        let params = json!({
            "type-id": self.spec.id,
            "params": list_to(&args.params, param_value_to_json).map_err(protocol)?,
            "choices": list_to(&choices, choice_value_to_json).map_err(protocol)?,
            "inputs": list_to(&args.inputs, resolved_input_to_json).map_err(protocol)?,
        });
        let Value::Object(params) = params else {
            unreachable!("an object literal")
        };
        let allowed = plugin.manifest.capabilities.kernel;
        let (ask, asked) = channel();
        let (answer, answers) = channel();
        let result = std::thread::scope(|s| {
            s.spawn(move || {
                let calls = Calls {
                    ask: ask.clone(),
                    answers,
                };
                let _ = ask.send(Ask::Done(plugin.call("evaluate", &params, Some(calls))));
            });
            loop {
                match asked.recv().expect("the component's thread answers once") {
                    Ask::Kernel(r) => {
                        let _ = answer.send(serve::kernel_call(kernel, allowed, &r));
                    }
                    Ask::Done(result) => break result,
                }
            }
        });
        let result = result.map_err(|e| e.diagnostic(&plugin.id(), &during))?;
        serve::result_from(&result, feature_output_from_json)
            .map_err(protocol)?
            .map(TypeOutput::from)
    }

    fn plugin_version(&self) -> Option<&str> {
        Some(&self.version)
    }
}

/// Loads the Tier 1 plugin in `dir` (its `arrix-plugin.toml` and the
/// component its `[tier1]` table names; a core module is made a component
/// here) and registers every feature type it describes, or none of them.
/// A component that imports anything outside the world is refused.
pub fn register_tier1(
    registry: &mut Registry,
    dir: &Path,
) -> Result<Vec<FeatureTypeId>, HostError> {
    let failed = |plugin: &str, message: String| HostError::Process {
        plugin: plugin.to_owned(),
        message,
    };
    let text = std::fs::read_to_string(dir.join(MANIFEST)).map_err(|e| {
        failed(
            &dir.display().to_string(),
            format!("its {MANIFEST} could not be read: {e}"),
        )
    })?;
    let manifest = Manifest::parse(&text)?;
    check(&manifest, 1)?;
    let id = manifest.id.to_string();
    let tier1 = manifest
        .tier1
        .clone()
        .expect("a tier 1 manifest has [tier1]");
    let path = dir.join(&tier1.component);
    let bytes = std::fs::read(&path).map_err(|e| {
        failed(
            &id,
            format!("its component {} could not be read: {e}", path.display()),
        )
    })?;
    let component =
        compile(&bytes).map_err(|e| failed(&id, format!("{}: {e}", tier1.component)))?;
    let pre = linker()
        .and_then(|l| l.instantiate_pre(&component))
        .map_err(|e| failed(&id, format!("{}: {e:#}", tier1.component)))?;
    let plugin = Plugin {
        fuel: tier1.fuel.unwrap_or(DEFAULT_FUEL),
        manifest,
        pre,
    };
    let specs = plugin
        .call("describe", &Map::new(), None)
        .map_err(|e| failed(&id, e.diagnostic(&id, "describing").message))
        .and_then(|v| {
            list_from(&v, feature_type_spec_from_json)
                .map_err(|e| failed(&id, format!("its `feature.describe` is malformed: {e}")))
        })?;
    let m = plugin.manifest.clone();
    let version = m.version.to_string();
    let plugin = Arc::new(plugin);
    register_types(registry, &m, specs, |spec| {
        Arc::new(Tier1Type {
            spec,
            version: version.clone(),
            plugin: plugin.clone(),
        })
    })
}

/// The component in `bytes`, made one first if it is a core module, and
/// refused if it imports anything the world does not.
fn compile(bytes: &[u8]) -> Result<Component, String> {
    let encoded;
    let bytes = if wasmparser::Parser::is_core_wasm(bytes) {
        encoded = wit_component::ComponentEncoder::default()
            .module(bytes)
            .and_then(|e| e.validate(true).encode())
            .map_err(|e| format!("it could not be made a component: {e:#}"))?;
        &encoded[..]
    } else {
        bytes
    };
    let component = Component::new(engine(), bytes).map_err(|e| format!("{e:#}"))?;
    let world = [interface("types"), interface("kernel")];
    let ty = component.component_type();
    let foreign: Vec<_> = ty
        .imports(engine())
        .map(|(name, _)| name)
        .filter(|name| !world.iter().any(|w| w == name))
        .map(|name| format!("`{name}`"))
        .collect();
    if !foreign.is_empty() {
        return Err(format!(
            "it imports {}, outside the world, which a sandbox does not grant",
            foreign.join(", ")
        ));
    }
    Ok(component)
}
