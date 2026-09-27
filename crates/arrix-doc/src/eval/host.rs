//! The plugin API's `Kernel` over `arrix-kernel`, scoped to one evaluating
//! feature: every body it builds is named as that feature's, and a feature
//! sees only the bodies it built itself, by its own handles.

use arrix_core::{Diagnostic, FeatureId, Frame, PersistentName, Profile, Ref, Severity};
use arrix_kernel::{CallIndex, Kernel, KernelBody, KernelError};
use arrix_plugin_api::{Body, MassProperties};

/// A kernel failure as the user sees it: its category as the code, and the
/// name it is about, if any, to highlight.
pub(crate) fn kernel_diagnostic(e: &KernelError) -> Diagnostic {
    let d = Diagnostic {
        severity: Severity::Error,
        code: e.code(),
        message: e.to_string(),
        refs: Vec::new(),
        candidates: Vec::new(),
    };
    match e {
        KernelError::UnknownName { name } | KernelError::WrongKind { name, .. } => {
            d.with_refs([Ref::Topo(name.clone())])
        }
        _ => d,
    }
}

pub(crate) struct FeatureKernel<'k> {
    kernel: &'k mut Kernel,
    feature: FeatureId,
    /// The feature's bodies; a plugin's handle is an index here.
    bodies: Vec<KernelBody>,
    /// The call the last kernel failure is recorded at.
    pub(crate) failed: Option<CallIndex>,
}

impl<'k> FeatureKernel<'k> {
    pub(crate) fn new(kernel: &'k mut Kernel, feature: FeatureId) -> Self {
        Self {
            kernel,
            feature,
            bodies: Vec::new(),
            failed: None,
        }
    }

    /// The kernel's body behind a handle this feature was given.
    pub(crate) fn body(&self, body: &Body) -> Option<KernelBody> {
        self.bodies.get(body.handle() as usize).copied()
    }

    fn resolve(&self, body: &Body) -> Result<KernelBody, Diagnostic> {
        self.body(body).ok_or_else(|| {
            Diagnostic::new(
                Severity::Error,
                "kernel.unknown-body",
                format!(
                    "body {} was not built by this feature's evaluation",
                    body.handle()
                ),
            )
            .expect("a literal code is well-formed")
        })
    }

    fn fail(&mut self, e: &KernelError) -> Diagnostic {
        if let Some(call) = e.call() {
            self.failed = Some(call);
        }
        kernel_diagnostic(e)
    }

    /// A new handle of this feature's to `body`.
    fn hand_out(&mut self, body: KernelBody) -> Body {
        let handle = u32::try_from(self.bodies.len()).expect("under 2³² bodies");
        self.bodies.push(body);
        Body::from_handle(handle)
    }

    /// Unwraps a kernel result, keeping where a failure is recorded.
    fn take<T>(&mut self, r: Result<T, KernelError>) -> Result<T, Diagnostic> {
        r.map_err(|e| self.fail(&e))
    }
}

impl arrix_plugin_api::Kernel for FeatureKernel<'_> {
    fn extrude(&mut self, profile: &Profile, distance: f64) -> Result<Body, Diagnostic> {
        let r = self.kernel.extrude(self.feature, profile, distance);
        let body = self.take(r)?;
        Ok(self.hand_out(body))
    }

    fn names(&mut self, body: &Body) -> Result<Vec<PersistentName>, Diagnostic> {
        let b = self.resolve(body)?;
        let r = self.kernel.names(b).map(|n| n.iter().cloned().collect());
        self.take(r)
    }

    fn face_frame(&mut self, body: &Body, face: &PersistentName) -> Result<Frame, Diagnostic> {
        let b = self.resolve(body)?;
        let r = self.kernel.face_frame(b, face);
        self.take(r)
    }

    fn measure(&mut self, body: &Body) -> Result<MassProperties, Diagnostic> {
        let b = self.resolve(body)?;
        let r = self.kernel.mass_properties(b);
        let p = self.take(r)?;
        Ok(MassProperties {
            volume: p.volume,
            area: p.area,
            centroid: p.centroid,
        })
    }

    fn fuse(&mut self, target: &Body, tool: &Body) -> Result<Body, Diagnostic> {
        let (a, b) = (self.resolve(target)?, self.resolve(tool)?);
        let r = self.kernel.fuse(self.feature, a, b);
        let body = self.take(r)?;
        Ok(self.hand_out(body))
    }

    fn cut(&mut self, target: &Body, tool: &Body) -> Result<Body, Diagnostic> {
        let (a, b) = (self.resolve(target)?, self.resolve(tool)?);
        let r = self.kernel.cut(self.feature, a, b);
        let body = self.take(r)?;
        Ok(self.hand_out(body))
    }
}
