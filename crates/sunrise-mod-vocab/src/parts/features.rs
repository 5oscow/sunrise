use super::*;
/// The feature types registered at startup, for the executable to hand to the engine's feature
/// decoding.
pub(crate) fn installed_feature_type_list() -> &'static [crate::world::feature_type::FeatureTypeInfo]
{
    installed().map_or(&[], |installed| installed.feature_types.as_slice())
}

impl dyn Registrar + '_ {
    /// Register feature type `name` (`slime_tree` or `mymod:slime_tree`), the way NeoForge's
    /// registry `FEATURE` registers once at startup: a configured feature with
    /// `"type": "modid:slime_tree"` then places by `ty`'s code, wherever the game places a
    /// feature (decoration, nested features, growth, bone meal, `/place feature`). The type's id
    /// is in a namespace owned by the calling mod's manifest; another mod's namespace, an empty
    /// name, an id registered twice and a vanilla feature type's id refuse the mods at startup,
    /// naming the mod.
    pub fn register_feature_type<T: crate::FeatureType>(&mut self, name: &str, ty: T) {
        self.add_feature_type(name, Arc::new(ty));
    }
}

/// Registrar operations for features.
pub trait FeaturesRegistrar {
    /// Register feature type `name` with `ty`; mods reach it through `register_feature_type`.
    ///
    /// On WebAssembly, worker calls run under the linked group's store lock and call budget.
    fn add_feature_type(&mut self, name: &str, ty: Arc<dyn crate::FeatureType>);
}
