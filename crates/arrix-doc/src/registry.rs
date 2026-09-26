//! Feature types by id (docs/DATA-MODEL.md §Features). Built-ins register
//! under `core.` through the same shape a plugin's feature is adapted onto,
//! so the evaluator has one path for both. A record whose type is not here
//! is not an error: it evaluates frozen.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrix_plugin_api::FeatureTypeSpec;

use crate::document::{FeatureTypeId, InvalidFeatureTypeId};

/// A feature type as the evaluator sees it, built-in or plugin.
pub trait FeatureType: Send + Sync {
    /// Its form, inputs and output slots; `spec().id` is its id.
    fn spec(&self) -> &FeatureTypeSpec;
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    #[error(transparent)]
    Id(#[from] InvalidFeatureTypeId),
    #[error("feature type {0} is registered twice")]
    Duplicate(FeatureTypeId),
}

#[derive(Clone, Default)]
pub struct Registry {
    types: BTreeMap<FeatureTypeId, Arc<dyn FeatureType>>,
}

impl Registry {
    pub fn register(&mut self, ty: Arc<dyn FeatureType>) -> Result<FeatureTypeId, RegistryError> {
        let id = FeatureTypeId::new(ty.spec().id.clone())?;
        if self.types.contains_key(&id) {
            return Err(RegistryError::Duplicate(id));
        }
        self.types.insert(id.clone(), ty);
        Ok(id)
    }

    pub fn get(&self, id: &FeatureTypeId) -> Option<&Arc<dyn FeatureType>> {
        self.types.get(id)
    }

    /// The registered ids, sorted.
    pub fn ids(&self) -> impl Iterator<Item = &FeatureTypeId> {
        self.types.keys()
    }
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.types.keys()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ty(FeatureTypeSpec);

    impl FeatureType for Ty {
        fn spec(&self) -> &FeatureTypeSpec {
            &self.0
        }
    }

    fn ty(id: &str) -> Arc<dyn FeatureType> {
        Arc::new(Ty(FeatureTypeSpec {
            id: id.into(),
            version: 1,
            title: id.into(),
            params: vec![],
            inputs: vec![],
            outputs: vec![],
        }))
    }

    #[test]
    fn registers_built_ins_and_plugin_types_alike() {
        let mut r = Registry::default();
        r.register(ty("gears.spur")).unwrap();
        let plane = r.register(ty("core.datum-plane")).unwrap();
        assert_eq!(plane.plugin(), None);
        assert_eq!(r.get(&plane).unwrap().spec().version, 1);
        let ids: Vec<_> = r.ids().map(FeatureTypeId::as_str).collect();
        assert_eq!(ids, ["core.datum-plane", "gears.spur"]);
        assert_eq!(
            FeatureTypeId::new("gears.spur")
                .unwrap()
                .plugin()
                .unwrap()
                .as_str(),
            "gears"
        );
    }

    #[test]
    fn refuses_a_duplicate_or_a_malformed_id() {
        let mut r = Registry::default();
        r.register(ty("core.datum-plane")).unwrap();
        assert!(matches!(
            r.register(ty("core.datum-plane")),
            Err(RegistryError::Duplicate(_))
        ));
        for bad in [
            "datum-plane",
            "core.",
            "Core.x",
            "gears.Spur",
            "gears.spur.x",
            ".x",
        ] {
            assert!(
                matches!(r.register(ty(bad)), Err(RegistryError::Id(_))),
                "{bad}"
            );
        }
        assert!(serde_json::from_str::<FeatureTypeId>("\"gears\"").is_err());
    }
}
