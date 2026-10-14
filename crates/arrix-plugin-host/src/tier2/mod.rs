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

use arrix_core::{Diagnostic, Id};
use arrix_doc::{
    CommandType, ExporterType, FeatureArgs, FeatureType, FeatureTypeId, Registry, TypeOutput,
};
use arrix_plugin_api::{
    API_VERSION, ChoiceValue, CommandGroup, CommandInput, CommandSpec, Context, ExportFile,
    ExporterSpec, FeatureTypeSpec, Kernel, ParamValue, Snapshot, Subject,
};
use serde_json::{Value, json};

use crate::manifest::Manifest;
use crate::tier0::{
    HostError, check, error, register_commands, register_exporters, register_types,
};
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

    /// Calls `method`, serving the plugin's requests as `serving` says:
    /// its kernel calls on the evaluating kernel, or its document calls on
    /// the export's snapshot. A call that fails ends the process; the next
    /// one starts another.
    fn call(&mut self, method: &str, params: Value, serving: Serving) -> Result<Value, CallError> {
        let timeout = self.timeout;
        let mut serving = serving;
        let caps = &self.manifest.capabilities;
        let (kernel_ok, document_ok) = (caps.kernel, caps.document_read);
        let mut serve = |request: &Request| match &mut serving {
            Serving::Kernel(k) => serve::kernel_call(&mut **k, kernel_ok, request),
            Serving::Document(d) => serve::document_call(&mut **d, document_ok, request),
            Serving::Nothing => Served::Answer(Err(RpcError::method_not_found(&request.method))),
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

/// What a call serves the plugin's requests from.
enum Serving<'a> {
    /// A feature's evaluation: the `kernel` interface.
    Kernel(&'a mut dyn Kernel),
    /// An export or an expansion: the `document` interface, from the
    /// snapshot.
    Document(&'a mut dyn Snapshot),
    /// A call that is not to be called back, such as a description.
    Nothing,
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
            .call("feature.evaluate", params, Serving::Kernel(kernel))
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

/// One exporter of a Tier 2 plugin.
struct Tier2Exporter {
    spec: ExporterSpec,
    plugin: Arc<Mutex<Plugin>>,
}

impl ExporterType for Tier2Exporter {
    fn spec(&self) -> &ExporterSpec {
        &self.spec
    }

    /// The plugin's `exchange.run`, its `document` calls served from
    /// `snapshot` while it runs.
    fn export(
        &self,
        snapshot: &mut dyn Snapshot,
        subject: &Subject,
        params: &[ParamValue],
        choices: &[ChoiceValue],
    ) -> Result<Vec<ExportFile>, Diagnostic> {
        let during = format!("exporting {}", self.spec.id);
        let mut plugin = self.plugin.lock().unwrap_or_else(|p| p.into_inner());
        let protocol = |e: WireError| error("plugin.protocol", format!("{during}: {e}"));
        let args = json!({
            "exporter": self.spec.id,
            "subject": subject_to_json(subject).map_err(protocol)?,
            "params": list_to(params, param_value_to_json).map_err(protocol)?,
            "choices": list_to(choices, choice_value_to_json).map_err(protocol)?,
        });
        let result = plugin
            .call("exchange.run", args, Serving::Document(snapshot))
            .map_err(|e| e.diagnostic(&plugin.id(), &during))?;
        serve::result_from(&result, |v| list_from(v, file_from_json)).map_err(|e| {
            plugin.process = None;
            protocol(e)
        })?
    }
}

/// One command of a Tier 2 plugin.
struct Tier2Command {
    spec: CommandSpec,
    plugin: Arc<Mutex<Plugin>>,
}

impl CommandType for Tier2Command {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    /// The plugin's `commands.expand`, its `document` calls served from
    /// `snapshot` while it runs (ADR-0038 §4).
    fn expand(
        &self,
        snapshot: &mut dyn Snapshot,
        context: &Context,
        params: &[ParamValue],
        choices: &[ChoiceValue],
        inputs: &[CommandInput],
        ids: &[Id],
    ) -> Result<CommandGroup, Diagnostic> {
        let during = format!("expanding {}", self.spec.id);
        let mut plugin = self.plugin.lock().unwrap_or_else(|p| p.into_inner());
        let protocol = |e: WireError| error("plugin.protocol", format!("{during}: {e}"));
        let args = json!({
            "command": self.spec.id,
            "context": context_to_json(context).map_err(protocol)?,
            "params": list_to(params, param_value_to_json).map_err(protocol)?,
            "choices": list_to(choices, choice_value_to_json).map_err(protocol)?,
            "inputs": list_to(inputs, command_input_to_json).map_err(protocol)?,
            "ids": list_to(ids, id_to).map_err(protocol)?,
        });
        let result = plugin
            .call("commands.expand", args, Serving::Document(snapshot))
            .map_err(|e| e.diagnostic(&plugin.id(), &during))?;
        serve::result_from(&result, command_group_from_json).map_err(|e| {
            plugin.process = None;
            protocol(e)
        })?
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
    let specs = match plugin.call("feature.describe", Value::Null, Serving::Nothing) {
        Ok(v) => list_from(&v, feature_type_spec_from_json)
            .map_err(|e| process(format!("its `feature.describe` is malformed: {e}")))?,
        Err(e) => return Err(process(e.diagnostic(&id, "describing").message)),
    };
    let exporters = match plugin.call("exchange.exporters", Value::Null, Serving::Nothing) {
        Ok(v) => list_from(&v, exporter_spec_from_json)
            .map_err(|e| process(format!("its `exchange.exporters` is malformed: {e}")))?,
        Err(e) => return Err(process(e.diagnostic(&id, "describing").message)),
    };
    let commands = match plugin.call("commands.commands", Value::Null, Serving::Nothing) {
        Ok(v) => list_from(&v, command_spec_from_json)
            .map_err(|e| process(format!("its `commands.commands` is malformed: {e}")))?,
        Err(e) => return Err(process(e.diagnostic(&id, "describing").message)),
    };
    let m = plugin.manifest.clone();
    let plugin = Arc::new(Mutex::new(plugin));
    // Types, exporters and commands register together, or none do.
    let mut next = registry.clone();
    let mut ids = register_types(&mut next, &m, specs, |spec| {
        Arc::new(Tier2Type {
            spec,
            version: version.clone(),
            plugin: plugin.clone(),
        })
    })?;
    ids.extend(register_exporters(&mut next, &m, exporters, |spec| {
        Arc::new(Tier2Exporter {
            spec,
            plugin: plugin.clone(),
        })
    })?);
    ids.extend(register_commands(&mut next, &m, commands, |spec| {
        Arc::new(Tier2Command {
            spec,
            plugin: plugin.clone(),
        })
    })?);
    *registry = next;
    Ok(ids)
}
