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
//!
//! The evaluator's interrupt reaches a running component through the
//! engine's epoch, ticked every [`TICK`] by one thread: a store's deadline
//! callback looks at the flag on each tick and traps if it is raised
//! (ADR-0035 §7). The clock decides only when the flag is looked at;
//! fuel is the only budget in a result.

mod value;

use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use arrix_core::{Diagnostic, Id};
use arrix_doc::{
    CommandType, ExporterType, FeatureArgs, FeatureType, FeatureTypeId, Interrupt, Registry,
    TypeOutput,
};
use arrix_plugin_api::{
    API_VERSION, ChoiceValue, CommandGroup, CommandInput, CommandSpec, Context, ExportFile,
    ExporterSpec, FeatureTypeSpec, Kernel, ParamValue, Snapshot, Subject,
};
use serde_json::{Map, Value, json};
use wasmtime::component::{Component, InstancePre, Linker, ResourceType, Val};
use wasmtime::{Config, Engine, ResourceLimiter, Store, Trap, UpdateDeadline};

use crate::manifest::Manifest;
use crate::tier0::{
    HostError, check, error, register_commands, register_exporters, register_types,
};
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

/// How often a running component looks at the interrupt flag: a tenth of
/// the cancellation budget (docs/CONCURRENCY-WASM.md §Budgets).
const TICK: Duration = Duration::from_millis(5);

/// The engine every Tier 1 plugin compiles on, and the thread that ticks
/// its epoch, started with it.
fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.consume_fuel(true);
        config.epoch_interruption(true);
        config.cranelift_nan_canonicalization(true);
        let engine = Engine::new(&config).expect("the Tier 1 engine's configuration is valid");
        let ticked = engine.clone();
        std::thread::Builder::new()
            .name("arrix-tier1-epoch".to_owned())
            .spawn(move || {
                loop {
                    std::thread::sleep(TICK);
                    ticked.increment_epoch();
                }
            })
            .expect("the epoch thread starts");
        engine
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
    /// A `kernel` call (while evaluating) or a `document` call (while
    /// exporting).
    Call(Request),
    Done(Result<Value, Ended>),
}

/// Why a call ended without its result.
enum Ended {
    /// A kernel call that ended it (refused, malformed or interrupted).
    Call(CallError),
    Fuel,
    /// The evaluator's interrupt, raised while it ran.
    Interrupted,
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

    /// Calls the export `func` of interface `iface` with `args` (the wire's params by
    /// name) in a fresh store, its kernel calls asked over `calls`, until
    /// it returns or `interrupt` is raised.
    fn call(
        &self,
        iface: &str,
        func: &str,
        args: &Map<String, Value>,
        calls: Option<Calls>,
        interrupt: Interrupt,
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
        store.set_epoch_deadline(1);
        store.epoch_deadline_callback(move |_| {
            Ok(if interrupt.is_raised() {
                UpdateDeadline::Interrupt
            } else {
                UpdateDeadline::Continue(1)
            })
        });
        let run = |store: &mut Store<State>| -> Result<Value, wasmtime::Error> {
            let instance = self.pre.instantiate(&mut *store)?;
            let export = instance
                .get_export_index(&mut *store, None, &interface(iface))
                .ok_or_else(|| wasmtime::format_err!("no `{iface}` export"))?;
            let f = instance
                .get_export_index(&mut *store, Some(&export), func)
                .and_then(|i| instance.get_func(&mut *store, i))
                .ok_or_else(|| wasmtime::format_err!("no `{iface}.{func}` export"))?;
            let ty = f.ty(&*store);
            let mut params = Vec::new();
            for (name, ty) in ty.params() {
                let v = args.get(name).unwrap_or(&Value::Null);
                params.push(from_json(v, &ty, store).map_err(|e| wasmtime::format_err!("{e}"))?);
            }
            let mut results = vec![Val::Bool(false); ty.results().len()];
            f.call(&mut *store, &params, &mut results)?;
            let ty = ty
                .results()
                .next()
                .expect("a world function returns a result");
            to_json(&results[0], &ty, store).map_err(|e| wasmtime::Error::new(Protocol(e)))
        };
        run(&mut store).map_err(|e| {
            let state = store.data_mut();
            if let Some(Protocol(w)) = e.downcast_ref::<Protocol>() {
                Ended::Call(CallError::Protocol(format!("its result: {w}")))
            } else if let Some(c) = state.ended.take() {
                Ended::Call(c)
            } else if e.downcast_ref::<Trap>() == Some(&Trap::OutOfFuel) {
                Ended::Fuel
            } else if e.downcast_ref::<Trap>() == Some(&Trap::Interrupt) {
                Ended::Interrupted
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
            // A stop like a kernel call's, which the evaluator reads as the
            // caller's and not the feature's.
            Ended::Interrupted => ("kernel.interrupted", "was interrupted".to_owned()),
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
        let Some(name) = m.name.strip_prefix("kernel.") else {
            continue;
        };
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
    // The `document` import: served from the export's snapshot, and only
    // while an export runs (ADR-0037 §5).
    let mut document = linker.instance(&interface("document"))?;
    for m in METHODS
        .iter()
        .filter(|m| m.direction == Direction::PluginToHost)
    {
        let Some(name) = m.name.strip_prefix("document.") else {
            continue;
        };
        let method = m.name;
        document.func_new(name, move |mut store, ty, params, results| {
            let served = ask(&mut store, method, &ty, params);
            let answer = served.and_then(|v| {
                let ty = ty
                    .results()
                    .next()
                    .expect("a document function returns a result");
                from_json(&v, &ty, &mut store).map_err(|e| CallError::Protocol(e.to_string()))
            });
            match answer {
                Ok(v) => {
                    results[0] = v;
                    Ok(())
                }
                Err(c) => {
                    store.data_mut().ended = Some(c);
                    wasmtime::bail!("the document call `{method}` ended the call")
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
    for ((name, pty), v) in ty.params().zip(params) {
        let v =
            to_json(v, &pty, store).map_err(|e| CallError::Protocol(format!("`{method}`: {e}")))?;
        args.insert(name.to_owned(), v);
    }
    let Some(calls) = &store.data().calls else {
        return Err(CallError::Capability(format!(
            "it called `{method}` outside an evaluation or an export"
        )));
    };
    let request = Request {
        id: None,
        method: method.to_owned(),
        params: Value::Object(args),
    };
    let gone = || CallError::Crash("the host stopped serving".to_owned());
    calls.ask.send(Ask::Call(request)).map_err(|_| gone())?;
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
        let interrupt = args.interrupt.clone();
        let (ask, asked) = channel();
        let (answer, answers) = channel();
        let result = std::thread::scope(|s| {
            s.spawn(move || {
                let calls = Calls {
                    ask: ask.clone(),
                    answers,
                };
                let _ = ask.send(Ask::Done(plugin.call(
                    "feature",
                    "evaluate",
                    &params,
                    Some(calls),
                    interrupt,
                )));
            });
            loop {
                match asked.recv().expect("the component's thread answers once") {
                    Ask::Call(r) => {
                        // `document` is for exports: an evaluation has the kernel only.
                        let served = if r.method.starts_with("document.") {
                            Served::End(CallError::Capability(format!(
                                "it called `{}` during an evaluation, which reads the kernel only",
                                r.method
                            )))
                        } else {
                            serve::kernel_call(kernel, allowed, &r)
                        };
                        let _ = answer.send(served);
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

/// One exporter of a Tier 1 plugin.
struct Tier1Exporter {
    spec: ExporterSpec,
    plugin: Arc<Plugin>,
}

impl ExporterType for Tier1Exporter {
    fn spec(&self) -> &ExporterSpec {
        &self.spec
    }

    /// The component's `run` in a fresh store, with fuel and the epoch
    /// interrupt, on a thread of its own; its `document` calls are served
    /// here from `snapshot` (ADR-0037 §5, §9).
    fn export(
        &self,
        snapshot: &mut dyn Snapshot,
        subject: &Subject,
        params: &[ParamValue],
        choices: &[ChoiceValue],
    ) -> Result<Vec<ExportFile>, Diagnostic> {
        let plugin = &self.plugin;
        let during = format!("exporting {}", self.spec.id);
        let protocol = |e: WireError| error("plugin.protocol", format!("{during}: {e}"));
        let args = json!({
            "exporter": self.spec.id,
            "subject": subject_to_json(subject).map_err(protocol)?,
            "params": list_to(params, param_value_to_json).map_err(protocol)?,
            "choices": list_to(choices, choice_value_to_json).map_err(protocol)?,
        });
        let Value::Object(args) = args else {
            unreachable!("an object literal")
        };
        let allowed = plugin.manifest.capabilities.document_read;
        let (ask, asked) = channel();
        let (answer, answers) = channel();
        let result = std::thread::scope(|s| {
            s.spawn(move || {
                let calls = Calls {
                    ask: ask.clone(),
                    answers,
                };
                let _ = ask.send(Ask::Done(plugin.call(
                    "exchange",
                    "run",
                    &args,
                    Some(calls),
                    Interrupt::default(),
                )));
            });
            loop {
                match asked.recv().expect("the component's thread answers once") {
                    Ask::Call(r) => {
                        let _ = answer.send(serve::document_call(snapshot, allowed, &r));
                    }
                    Ask::Done(result) => break result,
                }
            }
        });
        let result = result.map_err(|e| e.diagnostic(&plugin.id(), &during))?;
        serve::result_from(&result, |v| list_from(v, file_from_json)).map_err(protocol)?
    }
}

/// One command of a Tier 1 plugin.
struct Tier1Command {
    spec: CommandSpec,
    plugin: Arc<Plugin>,
}

impl CommandType for Tier1Command {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    /// The component's `expand` in a fresh store, with fuel and the epoch
    /// interrupt, on a thread of its own; its `document` calls are served
    /// here from `snapshot`, the base generation's (ADR-0038 §4).
    fn expand(
        &self,
        snapshot: &mut dyn Snapshot,
        context: &Context,
        params: &[ParamValue],
        choices: &[ChoiceValue],
        inputs: &[CommandInput],
        ids: &[Id],
    ) -> Result<CommandGroup, Diagnostic> {
        let plugin = &self.plugin;
        let during = format!("expanding {}", self.spec.id);
        let protocol = |e: WireError| error("plugin.protocol", format!("{during}: {e}"));
        let args = json!({
            "command": self.spec.id,
            "context": context_to_json(context).map_err(protocol)?,
            "params": list_to(params, param_value_to_json).map_err(protocol)?,
            "choices": list_to(choices, choice_value_to_json).map_err(protocol)?,
            "inputs": list_to(inputs, command_input_to_json).map_err(protocol)?,
            "ids": list_to(ids, id_to).map_err(protocol)?,
        });
        let Value::Object(args) = args else {
            unreachable!("an object literal")
        };
        let allowed = plugin.manifest.capabilities.document_read;
        let (ask, asked) = channel();
        let (answer, answers) = channel();
        let result = std::thread::scope(|s| {
            s.spawn(move || {
                let calls = Calls {
                    ask: ask.clone(),
                    answers,
                };
                let _ = ask.send(Ask::Done(plugin.call(
                    "commands",
                    "expand",
                    &args,
                    Some(calls),
                    Interrupt::default(),
                )));
            });
            loop {
                match asked.recv().expect("the component's thread answers once") {
                    Ask::Call(r) => {
                        let _ = answer.send(serve::document_call(snapshot, allowed, &r));
                    }
                    Ask::Done(result) => break result,
                }
            }
        });
        let result = result.map_err(|e| e.diagnostic(&plugin.id(), &during))?;
        serve::result_from(&result, command_group_from_json).map_err(protocol)?
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
    let component = compile(&bytes, manifest.capabilities.document_read)
        .map_err(|e| failed(&id, format!("{}: {e}", tier1.component)))?;
    let pre = linker()
        .and_then(|l| l.instantiate_pre(&component))
        .map_err(|e| failed(&id, format!("{}: {e:#}", tier1.component)))?;
    let plugin = Plugin {
        fuel: tier1.fuel.unwrap_or(DEFAULT_FUEL),
        manifest,
        pre,
    };
    let specs = plugin
        .call(
            "feature",
            "describe",
            &Map::new(),
            None,
            Interrupt::default(),
        )
        .map_err(|e| failed(&id, e.diagnostic(&id, "describing").message))
        .and_then(|v| {
            list_from(&v, feature_type_spec_from_json)
                .map_err(|e| failed(&id, format!("its `feature.describe` is malformed: {e}")))
        })?;
    // A component without an `exchange` export has no exporters.
    let exporters = if exports(&component, "exchange") {
        plugin
            .call(
                "exchange",
                "exporters",
                &Map::new(),
                None,
                Interrupt::default(),
            )
            .map_err(|e| failed(&id, e.diagnostic(&id, "describing").message))
            .and_then(|v| {
                list_from(&v, exporter_spec_from_json)
                    .map_err(|e| failed(&id, format!("its `exchange.exporters` is malformed: {e}")))
            })?
    } else {
        Vec::new()
    };
    // A component without a `commands` export has no commands.
    let commands = if exports(&component, "commands") {
        plugin
            .call(
                "commands",
                "commands",
                &Map::new(),
                None,
                Interrupt::default(),
            )
            .map_err(|e| failed(&id, e.diagnostic(&id, "describing").message))
            .and_then(|v| {
                list_from(&v, command_spec_from_json)
                    .map_err(|e| failed(&id, format!("its `commands.commands` is malformed: {e}")))
            })?
    } else {
        Vec::new()
    };
    let m = plugin.manifest.clone();
    let version = m.version.to_string();
    let plugin = Arc::new(plugin);
    // Types, exporters and commands register together, or none do.
    let mut next = registry.clone();
    let mut ids = register_types(&mut next, &m, specs, |spec| {
        Arc::new(Tier1Type {
            spec,
            version: version.clone(),
            plugin: plugin.clone(),
        })
    })?;
    ids.extend(register_exporters(&mut next, &m, exporters, |spec| {
        Arc::new(Tier1Exporter {
            spec,
            plugin: plugin.clone(),
        })
    })?);
    ids.extend(register_commands(&mut next, &m, commands, |spec| {
        Arc::new(Tier1Command {
            spec,
            plugin: plugin.clone(),
        })
    })?);
    *registry = next;
    Ok(ids)
}

/// Whether `component` exports the interface `name`.
fn exports(component: &Component, name: &str) -> bool {
    let name = interface(name);
    component
        .component_type()
        .exports(engine())
        .any(|(export, _)| export == name)
}

/// The component in `bytes`, made one first if it is a core module, and
/// refused if it imports anything the world does not.
fn compile(bytes: &[u8], document_read: bool) -> Result<Component, String> {
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
    // `document` is the world's, granted by `document-read` (ADR-0037 §5).
    let mut world = vec![interface("types"), interface("kernel")];
    if document_read {
        world.push(interface("document"));
    }
    let ty = component.component_type();
    let foreign: Vec<_> = ty
        .imports(engine())
        .map(|(name, _)| name)
        .filter(|name| !world.iter().any(|w| w == name))
        .map(|name| format!("`{name}`"))
        .collect();
    if !document_read
        && ty
            .imports(engine())
            .any(|(n, _)| n == interface("document"))
    {
        return Err(
            "it imports `document`, and its manifest does not grant `document-read`".to_owned(),
        );
    }
    if !foreign.is_empty() {
        return Err(format!(
            "it imports {}, outside the world, which a sandbox does not grant",
            foreign.join(", ")
        ));
    }
    Ok(component)
}
