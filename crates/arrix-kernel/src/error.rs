//! What a kernel call can fail with, and the stable failure category each
//! failure counts under (docs/ARCHITECTURE.md §The kernel choke point,
//! "Failure categories").

use arris::io::MeshWriteError;
use arris::io::body::BodyError;
use arris::mesh::MeshError;
use arris::ops::{Fault, OpError, Reason};
use arris::{Interrupted, Stop};
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

    /// A call whose arguments Arris was never given because they are not
    /// a valid input (an axis with no direction).
    pub(crate) fn invalid_input(call: CallIndex, message: String) -> Self {
        KernelError::Op {
            call,
            code: DiagnosticCode::new("kernel.invalid-input").expect("a literal code"),
            message,
        }
    }

    /// A call Arris answered with something ArriX has no type for (a
    /// projection to an ellipse): `kernel.unsupported.<what>`.
    pub(crate) fn unsupported(call: CallIndex, what: &str, message: String) -> Self {
        KernelError::Op {
            call,
            code: DiagnosticCode::new(format!("kernel.unsupported.{what}"))
                .expect("a literal code"),
            message,
        }
    }

    pub(crate) fn mesh(call: CallIndex, e: &MeshError) -> Self {
        KernelError::Op {
            call,
            code: DiagnosticCode::new(mesh_category(e)).expect("every category is well-formed"),
            message: e.to_string(),
        }
    }

    /// `kernel.export.<error>`: why a format could not be written.
    pub(crate) fn export(call: CallIndex, word: &str, message: String) -> Self {
        KernelError::Op {
            call,
            code: DiagnosticCode::new(format!("kernel.export.{word}")).expect("a literal code"),
            message,
        }
    }

    pub(crate) fn mesh_write(call: CallIndex, e: &MeshWriteError) -> Self {
        let word = match e {
            MeshWriteError::TooManyTriangles { .. } => "too-many-triangles",
        };
        Self::export(call, word, e.to_string())
    }

    pub(crate) fn bytes(call: CallIndex, e: &BodyError) -> Self {
        let code = DiagnosticCode::new(body_category(e)).expect("every category is well-formed");
        KernelError::Op {
            call,
            code,
            message: e.to_string(),
        }
    }
}

/// `kernel.interrupted` for the poll, `kernel.interrupted.budget` for a
/// spent budget: one category whatever the call was, since the stop is
/// the caller's and not the input's.
fn interrupted(stop: &Interrupted) -> &'static str {
    match stop.by {
        Stop::Poll => "kernel.interrupted",
        Stop::Budget => "kernel.interrupted.budget",
    }
}

/// `kernel.mesh.<error>`: why a body could not be tessellated.
fn mesh_category(e: &MeshError) -> &'static str {
    match e {
        MeshError::IndexOutOfRange { .. } => "kernel.mesh.index-out-of-range",
        MeshError::RangeOutOfBounds { .. } => "kernel.mesh.range-out-of-bounds",
        MeshError::NonFinitePosition { .. } => "kernel.mesh.non-finite-position",
        MeshError::NotFound(_) => "kernel.mesh.not-found",
        MeshError::InvalidInput { .. } => "kernel.mesh.invalid-input",
        MeshError::Chord(_) => "kernel.mesh.chord",
        MeshError::Face { .. } => "kernel.mesh.face",
        MeshError::GridTooLarge { .. } => "kernel.mesh.grid-too-large",
        MeshError::Corners(_) => "kernel.mesh.corners",
        MeshError::NotInBody { .. } => "kernel.mesh.not-in-body",
        MeshError::WeldMismatch { .. } => "kernel.mesh.weld-mismatch",
        MeshError::Internal(_) => "kernel.mesh.internal",
        MeshError::Interrupted(stop) => interrupted(stop),
    }
}

/// `kernel.body.<error>`: why body bytes could not be written or read.
fn body_category(e: &BodyError) -> String {
    let word = match e {
        BodyError::Magic => "magic",
        BodyError::Version { .. } => "version",
        BodyError::Encode(_) => "encode",
        BodyError::Decode(_) => "decode",
        BodyError::Topo(_) => "topo",
        BodyError::Precision { .. } => "precision",
        BodyError::Rejected(_) => "rejected",
        BodyError::Interrupted(stop) => return interrupted(stop).into(),
    };
    format!("kernel.body.{word}")
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
        OpError::Unkeyed { .. } => "kernel.unkeyed".into(),
        OpError::Rejected(_) => "kernel.rejected".into(),
        OpError::Interrupted(stop) => interrupted(stop).into(),
    }
}

/// The leaf reason's name, lower-kebab (`BlendTooLarge` is `blend-too-large`).
/// Arris keeps `Reason::name` stable across regroupings, so the codes a
/// histogram counts under do not move when the groups do.
fn reason_word(reason: &Reason) -> String {
    let mut word = String::new();
    for (i, c) in reason.name().chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                word.push('-');
            }
            word.push(c.to_ascii_lowercase());
        } else {
            word.push(c);
        }
    }
    word
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
