//! The WIT world read into a small model the generators walk: named type
//! definitions and the functions of each interface, in the WIT's order.
//! A construct the wire has no shape for (flags, maps, futures, streams,
//! integers the codec does not know) panics here, so a WIT addition the
//! generators cannot carry fails the freshness test instead of being
//! skipped.

use std::path::Path;

use wit_parser::{Handle, Resolve, Type, TypeDefKind, TypeId};

/// A type as it is written in a field, a parameter or a result.
///
/// A list of `u8`, `f32` or `u32` is a *run* (ADR-0022 §5): bulk numbers
/// cross as one base64 string of their little-endian bytes.
#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    Bool,
    /// Only as the element of a run (`list<u8>`).
    U8,
    U32,
    S64,
    /// Only as the element of a run (`list<f32>`).
    F32,
    F64,
    String,
    /// `types.id`: ArriX's 13-character id text (ADR-0022 §3).
    Id,
    Named(String),
    /// `own<r>` or `borrow<r>`: a handle, a number.
    Handle(String),
    List(Box<Ty>),
    Option(Box<Ty>),
    Tuple(Vec<Ty>),
    Result(Option<Box<Ty>>, Option<Box<Ty>>),
}

impl Ty {
    /// The element type of a run: `u8`, `f32` or `u32`.
    pub fn run(&self) -> Option<&'static str> {
        match self {
            Ty::List(t) => match **t {
                Ty::U8 => Some("u8"),
                Ty::F32 => Some("f32"),
                Ty::U32 => Some("u32"),
                _ => None,
            },
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
    pub docs: String,
}

#[derive(Clone, Debug)]
pub struct Case {
    pub name: String,
    pub ty: Option<Ty>,
    pub docs: String,
}

#[derive(Clone, Debug)]
pub enum Def {
    Id,
    Resource,
    Record(Vec<Field>),
    Variant(Vec<Case>),
    Enum(Vec<String>),
}

#[derive(Clone, Debug)]
pub struct TypeDef {
    pub name: String,
    pub def: Def,
    pub docs: String,
}

/// Which side serves a function: a `feature` export is the plugin's, a
/// `kernel` import the host's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    HostToPlugin,
    PluginToHost,
}

impl Direction {
    pub fn text(self) -> &'static str {
        match self {
            Direction::HostToPlugin => "host-to-plugin",
            Direction::PluginToHost => "plugin-to-host",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Func {
    pub interface: String,
    pub name: String,
    pub direction: Direction,
    pub params: Vec<(String, Ty)>,
    pub result: Option<Ty>,
    pub docs: String,
}

impl Func {
    /// `<interface>.<function>`, the JSON-RPC method (ADR-0022 §3).
    pub fn method(&self) -> String {
        format!("{}.{}", self.interface, self.name)
    }
}

pub struct World {
    pub version: String,
    pub types: Vec<TypeDef>,
    pub funcs: Vec<Func>,
}

impl World {
    pub fn read(wit_dir: &Path) -> World {
        let mut resolve = Resolve::default();
        let (package, _) = resolve.push_dir(wit_dir).unwrap();
        let pkg = &resolve.packages[package];
        let version = pkg
            .name
            .version
            .as_ref()
            .expect("the package is versioned")
            .to_string();
        let world = &resolve.worlds[pkg.worlds["plugin"]];
        let exported: Vec<_> = world
            .exports
            .keys()
            .map(|k| resolve.name_world_key(k))
            .collect();

        let mut types = Vec::new();
        let mut funcs = Vec::new();
        for (iface_name, iface) in pkg.interfaces.iter() {
            let iface = &resolve.interfaces[*iface];
            assert_eq!(iface.name.as_deref(), Some(iface_name.as_str()));
            let direction = if exported
                .iter()
                .any(|e| e.starts_with(&format!("arrix:plugin/{iface_name}@")))
            {
                Direction::HostToPlugin
            } else {
                Direction::PluginToHost
            };
            for (name, id) in &iface.types {
                let def = &resolve.types[*id];
                // A `use` makes an alias; it is defined where it is declared.
                if matches!(def.kind, TypeDefKind::Type(Type::Id(_))) {
                    continue;
                }
                types.push(TypeDef {
                    name: name.clone(),
                    def: read_def(&resolve, name, &def.kind, iface_name),
                    docs: docs(&def.docs),
                });
            }
            for (name, f) in &iface.functions {
                funcs.push(Func {
                    interface: iface_name.clone(),
                    name: name.clone(),
                    direction,
                    params: f
                        .params
                        .iter()
                        .map(|p| (p.name.clone(), ty(&resolve, &p.ty)))
                        .collect(),
                    result: f.result.as_ref().map(|t| ty(&resolve, t)),
                    docs: docs(&f.docs),
                });
            }
        }
        let mut names: Vec<_> = types.iter().map(|t| t.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            types.len(),
            "the wire names types by their bare name, so two interfaces may not share one"
        );
        World {
            version,
            types,
            funcs,
        }
    }

    pub fn find(&self, name: &str) -> &TypeDef {
        self.types
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("no type {name} in the world"))
    }
}

fn docs(d: &wit_parser::Docs) -> String {
    d.contents
        .as_deref()
        .unwrap_or("")
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned()
}

fn read_def(resolve: &Resolve, name: &str, kind: &TypeDefKind, iface: &str) -> Def {
    match kind {
        TypeDefKind::Type(Type::U64) if (iface, name) == ("types", "id") => Def::Id,
        TypeDefKind::Resource => Def::Resource,
        TypeDefKind::Record(r) => Def::Record(
            r.fields
                .iter()
                .map(|f| Field {
                    name: f.name.clone(),
                    ty: ty(resolve, &f.ty),
                    docs: docs(&f.docs),
                })
                .collect(),
        ),
        TypeDefKind::Variant(v) => Def::Variant(
            v.cases
                .iter()
                .map(|c| Case {
                    name: c.name.clone(),
                    ty: c.ty.as_ref().map(|t| ty(resolve, t)),
                    docs: docs(&c.docs),
                })
                .collect(),
        ),
        TypeDefKind::Enum(e) => Def::Enum(e.cases.iter().map(|c| c.name.clone()).collect()),
        other => panic!(
            "the wire has no shape for {name}: a {} (ADR-0022 §3)",
            other.as_str()
        ),
    }
}

fn ty(resolve: &Resolve, t: &Type) -> Ty {
    match t {
        Type::Bool => Ty::Bool,
        Type::U8 => Ty::U8,
        Type::U32 => Ty::U32,
        Type::S64 => Ty::S64,
        Type::F32 => Ty::F32,
        Type::F64 => Ty::F64,
        Type::String => Ty::String,
        Type::Id(id) => ty_id(resolve, *id),
        other => panic!("the wire has no shape for the primitive {other:?} (ADR-0022 §3)"),
    }
}

fn ty_id(resolve: &Resolve, id: TypeId) -> Ty {
    let def = &resolve.types[id];
    if let Some(name) = &def.name {
        // A `use` alias resolves to the type it names.
        if let TypeDefKind::Type(Type::Id(target)) = def.kind {
            return ty_id(resolve, target);
        }
        if name == "id" {
            return Ty::Id;
        }
        return Ty::Named(name.clone());
    }
    match &def.kind {
        TypeDefKind::List(t) => Ty::List(Box::new(ty(resolve, t))),
        TypeDefKind::Option(t) => Ty::Option(Box::new(ty(resolve, t))),
        TypeDefKind::Tuple(t) => Ty::Tuple(t.types.iter().map(|t| ty(resolve, t)).collect()),
        TypeDefKind::Result(r) => Ty::Result(
            r.ok.as_ref().map(|t| Box::new(ty(resolve, t))),
            r.err.as_ref().map(|t| Box::new(ty(resolve, t))),
        ),
        TypeDefKind::Handle(Handle::Own(r) | Handle::Borrow(r)) => match ty_id(resolve, *r) {
            Ty::Named(name) => Ty::Handle(name),
            other => panic!("a handle to {other:?}, which is no resource"),
        },
        other => panic!("the wire has no shape for an anonymous {}", other.as_str()),
    }
}
