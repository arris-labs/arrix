//! Tier 2 (docs/PLUGINS.md §Three tiers, ADR-0021): a plugin in a child
//! process of its own, spoken to over the wire (ADR-0022). Its feature
//! types are adapted onto the registry's `FeatureType` as a Tier 0
//! plugin's are; its `evaluate` builds through the evaluating feature's
//! kernel, called back over the wire, its bodies crossing as that
//! evaluation's handles. Native only.

pub(crate) mod process;
pub(crate) mod serve;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arrix_core::Diagnostic;
use arrix_doc::{FeatureArgs, FeatureType, FeatureTypeId, Registry, TypeOutput};
use arrix_plugin_api::{API_VERSION, ChoiceValue, FeatureTypeSpec, Kernel};
use serde_json::{Value, json};

use crate::manifest::Manifest;
use crate::tier0::{HostError, check, error, register_types};
use crate::wire::rpc::{PROTOCOL_VERSION, Request, RpcError};
use crate::wire::*;
use process::{CallError, Process, Served};

/// How long the host waits for a plugin's next line before the call is
/// the feature's `plugin.timeout`.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// The manifest's file name in a plugin's directory.
pub const MANIFEST: &str = "arrix-plugin.toml";

/// One Tier 2 plugin, shared by its feature types: its manifest, its
/// directory and its process while one runs.
struct Plugin {
    manifest: Manifest,
    dir: PathBuf,
    timeout: Duration,
    process: Option<Process>,
}

impl Plugin {
    /// The running process, started and greeted if none is.
    fn process(&mut self) -> Result<&mut Process, CallError> {
        if self.process.is_none() {
            let mut p = Process::spawn(&self.manifest.command, &self.dir)
                .map_err(|e| CallError::Crash(format!("it could not be started: {e}")))?;
            initialize(&mut p, &self.manifest, self.timeout)?;
            self.process = Some(p);
        }
        Ok(self.process.as_mut().expect("started above"))
    }

    /// Calls `method`, serving the plugin's kernel calls on `kernel`. A
    /// call that fails ends the process; the next one starts another.
    fn call(
        &mut self,
        method: &str,
        params: Value,
        kernel: Option<&mut dyn Kernel>,
    ) -> Result<Value, CallError> {
        let timeout = self.timeout;
        let mut kernel = kernel;
        let allowed = self.manifest.capabilities.kernel;
        let mut serve = |request: &Request| match kernel.as_deref_mut() {
            Some(k) => serve::kernel_call(k, allowed, request),
            None => Served::Answer(Err(RpcError::method_not_found(&request.method))),
        };
        let result = self
            .process()
            .and_then(|p| p.call(method, params, timeout, &mut serve));
        if result.is_err() {
            self.process = None;
        }
        result
    }

    fn id(&self) -> String {
        self.manifest.id.to_string()
    }
}

/// The handshake (ADR-0022 §7): the protocol's version and the plugin
/// API's, the plugin's answer naming the API its stubs speak, which its
/// manifest's range must hold.
fn initialize(p: &mut Process, m: &Manifest, timeout: Duration) -> Result<(), CallError> {
    let params = json!({"protocol": PROTOCOL_VERSION, "api": API_VERSION});
    let answer = p.call("initialize", params, timeout, &mut |r| {
        Served::Answer(Err(RpcError::method_not_found(&r.method)))
    })?;
    let protocol = answer.get("protocol").and_then(Value::as_u64);
    if protocol != Some(PROTOCOL_VERSION.into()) {
        return Err(CallError::Protocol(format!(
            "it speaks protocol {}, and this host {PROTOCOL_VERSION}",
            answer.get("protocol").unwrap_or(&Value::Null)
        )));
    }
    let api = answer.get("api").and_then(Value::as_str).unwrap_or("");
    match semver::Version::parse(api) {
        Ok(v) if m.api.matches(&v) => Ok(()),
        _ => Err(CallError::Protocol(format!(
            "its stubs speak plugin API {api:?}, outside its manifest's `api` {}",
            m.api
        ))),
    }
}

impl CallError {
    pub(crate) fn diagnostic(&self, plugin: &str, during: &str) -> Diagnostic {
        let (code, what) = match self {
            // The evaluator's own stop, as the kernel gave it.
            CallError::Interrupted(d) => return d.clone(),
            CallError::Capability(why) => ("plugin.capability", format!("was refused: {why}")),
            CallError::Crash(why) => ("plugin.crash", format!("crashed: {why}")),
            CallError::Timeout(t) => (
                "plugin.timeout",
                format!("said nothing for {} s", t.as_secs_f64()),
            ),
            CallError::Protocol(why) => ("plugin.protocol", format!("broke the protocol: {why}")),
        };
        error(code, format!("plugin {plugin} {what} ({during})"))
    }
}

/// One feature type of a Tier 2 plugin.
struct Tier2Type {
    spec: FeatureTypeSpec,
    version: String,
    plugin: Arc<Mutex<Plugin>>,
}

impl FeatureType for Tier2Type {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.spec
    }

    /// The plugin's `feature.evaluate`, its kernel calls served on
    /// `kernel` while it runs. Its handles are `kernel`'s, so a body
    /// crosses as a number valid for this evaluation only.
    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        let during = format!("evaluating {}", self.spec.id);
        let mut plugin = self.plugin.lock().unwrap_or_else(|p| p.into_inner());
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
        let result = plugin
            .call("feature.evaluate", params, Some(kernel))
            .map_err(|e| e.diagnostic(&plugin.id(), &during))?;
        serve::result_from(&result, feature_output_from_json)
            .map_err(|e| {
                plugin.process = None;
                protocol(e)
            })?
            .map(TypeOutput::from)
    }

    fn plugin_version(&self) -> Option<&str> {
        Some(&self.version)
    }
}

/// Starts the Tier 2 plugin in `dir` (its `arrix-plugin.toml`, its
/// command run there) and registers every feature type it describes, or
/// none of them. The process stays up for the registry's types to call.
pub fn register_tier2(
    registry: &mut Registry,
    dir: &Path,
) -> Result<Vec<FeatureTypeId>, HostError> {
    let text = std::fs::read_to_string(dir.join(MANIFEST)).map_err(|e| HostError::Process {
        plugin: dir.display().to_string(),
        message: format!("its {MANIFEST} could not be read: {e}"),
    })?;
    let manifest = Manifest::parse(&text)?;
    check(&manifest, 2)?;
    let version = manifest.version.to_string();
    let timeout = manifest.timeout.unwrap_or(DEFAULT_TIMEOUT);
    let mut plugin = Plugin {
        manifest,
        dir: dir.to_owned(),
        timeout,
        process: None,
    };
    let id = plugin.id();
    let process = |message: String| HostError::Process {
        plugin: id.clone(),
        message,
    };
    let specs = match plugin.call("feature.describe", Value::Null, None) {
        Ok(v) => list_from(&v, feature_type_spec_from_json)
            .map_err(|e| process(format!("its `feature.describe` is malformed: {e}")))?,
        Err(e) => return Err(process(e.diagnostic(&id, "describing").message)),
    };
    let m = plugin.manifest.clone();
    let plugin = Arc::new(Mutex::new(plugin));
    register_types(registry, &m, specs, |spec| {
        Arc::new(Tier2Type {
            spec,
            version: version.clone(),
            plugin: plugin.clone(),
        })
    })
}
