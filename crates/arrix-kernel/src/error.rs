//! What a kernel call can fail with, and the stable failure category each
//! failure counts under (docs/ARCHITECTURE.md §The kernel choke point,
//! "Failure categories").

use arris::ops::{Fault, OpError, Reason};
use arrix_core::{DiagnosticCode, PersistentName, TopoKind};

use crate::{CallIndex, KernelBody};

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum KernelError {
    /// Arris refused the call recorded at `call`, which stays in the
    /// evaluation's records. `code` is the failure's category.
    #[error("kernel call {} failed ({code}): {message}", .call.0)]
    Op {
        call: CallIndex,
        code: DiagnosticCode,
        message: String,
    },
    /// The call succeeded, but an entity of its output has no name, or
    /// two share one: a kernel or naming bug, kept as a record like any
    /// other failure.
    #[error("kernel call {} made an entity naming cannot place: {message}", .call.0)]
    Naming { call: CallIndex, message: String },
    #[error("{0:?} is not a body of this evaluation")]
    UnknownBody(KernelBody),
    #[error("{name} names nothing in the body")]
    UnknownName { name: PersistentName },
    #[error("{name} names a {found:?}, where a {wanted:?} was wanted")]
    WrongKind {
        name: PersistentName,
        wanted: TopoKind,
        found: TopoKind,
    },
}

impl KernelError {
    /// The stable key this failure counts under.
    pub fn code(&self) -> DiagnosticCode {
        let code = match self {
            KernelError::Op { code, .. } => return code.clone(),
            KernelError::Naming { .. } => "kernel.naming",
            KernelError::UnknownBody(_) => "kernel.unknown-body",
            KernelError::UnknownName { .. } => "kernel.unknown-name",
            KernelError::WrongKind { .. } => "kernel.wrong-kind",
        };
        DiagnosticCode::new(code).expect("a literal code is well-formed")
    }

    /// The call this failure is recorded at, when a kernel call ran.
    pub fn call(&self) -> Option<CallIndex> {
        match self {
            KernelError::Op { call, .. } | KernelError::Naming { call, .. } => Some(*call),
            _ => None,
        }
    }

    pub(crate) fn op(call: CallIndex, e: &OpError) -> Self {
        let code = DiagnosticCode::new(category(e)).expect("every category is well-formed");
        KernelError::Op {
            call,
            code,
            message: e.to_string(),
        }
    }
}

/// `kernel.<error>[.<reason>]`: Arris's typed error and, where it has one,
/// its reason, as lower-kebab words.
fn category(e: &OpError) -> String {
    match e {
        OpError::InvalidInput { .. } => "kernel.invalid-input".into(),
        OpError::Unsupported { .. } => "kernel.unsupported".into(),
        OpError::Degenerate { reason, .. } => format!("kernel.degenerate.{}", reason_word(reason)),
        OpError::Profile(_) => "kernel.profile".into(),
        OpError::Tolerance { .. } => "kernel.tolerance".into(),
        OpError::NotFound(_) => "kernel.not-found".into(),
        OpError::Internal(fault) => format!("kernel.internal.{}", fault_word(fault)),
    }
}

fn reason_word(reason: &Reason) -> &'static str {
    match reason {
        Reason::NonFinite { .. } => "non-finite",
        Reason::NotPositive { .. } => "not-positive",
        Reason::ZeroThickness => "zero-thickness",
        Reason::ProfileCrossesAxis => "profile-crosses-axis",
        Reason::AxisNotInProfilePlane => "axis-not-in-profile-plane",
        Reason::AngleAboveTurn => "angle-above-turn",
        Reason::SpindleTorus => "spindle-torus",
        Reason::EllipticRevolve { .. } => "elliptic-revolve",
        Reason::DirectionNotNormal => "direction-not-normal",
        Reason::NotSolid => "not-solid",
        Reason::Empty => "empty",
        Reason::TangentContact => "tangent-contact",
        Reason::BesideSingularity => "beside-singularity",
        Reason::NonManifold => "non-manifold",
        Reason::NoEdges => "no-edges",
        Reason::RepeatedEdge => "repeated-edge",
        Reason::EdgeNotInBody => "edge-not-in-body",
        Reason::BlendTooLarge => "blend-too-large",
        Reason::TangentChain => "tangent-chain",
        Reason::VertexBlend => "vertex-blend",
        Reason::NotProjectable => "not-projectable",
        Reason::DegenerateEdge => "degenerate-edge",
        Reason::ProjectionCollapses => "projection-collapses",
        Reason::NotPlanar => "not-planar",
        Reason::OutOfDomain => "out-of-domain",
        Reason::Singular => "singular",
    }
}

fn fault_word(fault: &Fault) -> &'static str {
    match fault {
        Fault::Checker(_) => "checker",
        Fault::Builder(_) => "builder",
        Fault::Frame(_) => "frame",
        Fault::Classify(_) => "classify",
        Fault::Geometry(_) => "geometry",
        Fault::Seam { .. } => "seam",
        Fault::Split(_) => "split",
        Fault::CommonBlock { .. } => "common-block",
        Fault::Lumps(_) => "lumps",
        Fault::Invariant { .. } => "invariant",
        Fault::Unmade { .. } => "unmade",
        Fault::NoNormal { .. } => "no-normal",
        Fault::ProfileCurve(_) => "profile-curve",
    }
}
