//! Feature types a mod registers: vanilla's `Feature` with its own `place`, as a configured
//! feature with `"type": "modid:tree"` names it. Handed over once at startup and looked up by
//! `FeatureDefinition::decode` before its built-in fallback, so a mod feature runs through every
//! route a built-in one does: biome decoration on the worldgen workers, nested features,
//! growth, bone meal, `/place feature` and `LevelRef::place_feature`. Runs on worker threads,
//! so a type keeps no mutable state and its target carries no `Host`.
use crate::heightmap::HeightmapType;
use crate::registry::{BlockRegistry, BlockStateId};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use sunrise_core::nbt::{Compound, Tag};
use sunrise_core::random::RandomSource;

/// A feature type's engine side, as the executable's adapter sees it. `Send + Sync` because
/// neighbouring chunks decorate at once on worker threads; the same random gives the same
/// blocks.
pub trait ModFeatureType: Send + Sync {
    /// Prepare a configured feature's JSON once at load (the settings sit inline next to
    /// `type`). The game owns the JSON-to-NBT boundary; an error refuses the definition.
    fn prepare_config(&self, config: &serde_json::Value) -> Result<Arc<Tag>, String>;
    /// `Feature.place`: true when the feature says it placed. A write it made before
    /// answering false is kept, as vanilla keeps it. An `Err` is the mod's failure: the
    /// placement stops there with a `FeatureExecutionError::Mod`.
    fn place(
        &self,
        target: &mut dyn ModFeatureTarget,
        random: &mut dyn ModFeatureRandom,
        origin: [i32; 3],
        // Converted once and retained by the loaded definition.
        config: &Arc<Tag>,
    ) -> Result<bool, String>;
}

/// The level a mod feature places in, object-safe: the caller's own world (a decoration
/// region, the live level), never a second block store. A read the world can't answer is
/// `None`, a write it refuses is `false`; neither loads a chunk.
pub trait ModFeatureTarget {
    /// The numeric block registry the states are of, for the adapter's state index.
    fn registry(&self) -> Option<&'static BlockRegistry>;
    fn block(&mut self, position: [i32; 3]) -> Option<BlockStateId>;
    /// `LevelWriter.setBlock` with vanilla's update `flags`.
    fn set_block(&mut self, position: [i32; 3], state: BlockStateId, flags: i32) -> bool;
    /// Whether the block at `position` is in block tag `tag`.
    fn block_in_tag(&mut self, position: [i32; 3], tag: &str) -> Option<bool>;
    fn min_y(&mut self) -> Option<i32>;
    fn max_y(&mut self) -> Option<i32>;
    fn height(&mut self, kind: HeightmapType, x: i32, z: i32) -> Option<i32>;
    /// Block-entity data for a block just placed; worlds without block-entity storage drop it.
    fn set_block_entity(&mut self, position: [i32; 3], nbt: Compound) -> bool;
}

/// The caller's random, object-safe: the same stream the placement chain and the built-in
/// features draw from, in vanilla's order.
pub trait ModFeatureRandom {
    /// `nextInt(bound)`; 0 without a draw for a bound below 1.
    fn int(&mut self, bound: i32) -> i32;
    fn long(&mut self) -> i64;
    fn float(&mut self) -> f32;
    fn double(&mut self) -> f64;
    fn boolean(&mut self) -> bool;
}

/// Any `RandomSource` as a [`ModFeatureRandom`].
pub(crate) struct Draws<'a, R>(pub(crate) &'a mut R);
impl<R: RandomSource> ModFeatureRandom for Draws<'_, R> {
    fn int(&mut self, bound: i32) -> i32 {
        if bound < 1 {
            return 0;
        }
        self.0.next_int_bound(bound).unwrap_or(0)
    }
    fn long(&mut self) -> i64 {
        self.0.next_long()
    }
    fn float(&mut self) -> f32 {
        self.0.next_float()
    }
    fn double(&mut self) -> f64 {
        self.0.next_double()
    }
    fn boolean(&mut self) -> bool {
        self.0.next_boolean()
    }
}

/// A placement world as a [`ModFeatureTarget`].
pub(crate) struct Target<'a, W>(pub(crate) &'a mut W);
impl<W> ModFeatureTarget for Target<'_, W>
where
    W: super::feature::FeatureWorld + super::placement::PlacementContext,
{
    fn registry(&self) -> Option<&'static BlockRegistry> {
        self.0.block_registry()
    }
    fn block(&mut self, position: [i32; 3]) -> Option<BlockStateId> {
        self.0.block(position).ok()
    }
    fn set_block(&mut self, position: [i32; 3], state: BlockStateId, flags: i32) -> bool {
        self.0.set_block(position, state, flags).unwrap_or(false)
    }
    fn block_in_tag(&mut self, position: [i32; 3], tag: &str) -> Option<bool> {
        self.0.block_matches_tag(position, tag).ok()
    }
    fn min_y(&mut self) -> Option<i32> {
        self.0.min_y().ok()
    }
    fn max_y(&mut self) -> Option<i32> {
        self.0.max_y().ok()
    }
    fn height(&mut self, kind: HeightmapType, x: i32, z: i32) -> Option<i32> {
        self.0.height(kind, x, z).ok()
    }
    fn set_block_entity(&mut self, position: [i32; 3], nbt: Compound) -> bool {
        self.0.set_block_entity(position, nbt).is_ok()
    }
}

/// A configured feature of a mod's type: the type and its config, converted once at load.
#[derive(Clone)]
pub struct ModFeature {
    pub id: String,
    pub ty: Arc<dyn ModFeatureType>,
    pub config: Arc<Tag>,
}
impl std::fmt::Debug for ModFeature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModFeature")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// The installed types, `modid:tree` → type. Set once, before any world opens (the executable
/// hands the mods' registrations over), and read while decoding features.
pub type ModFeatureTypes = BTreeMap<String, Arc<dyn ModFeatureType>>;

static MOD_TYPES: OnceLock<ModFeatureTypes> = OnceLock::new();

#[cfg(test)]
thread_local! {
    /// The types a test installs, in place of the process-wide table.
    static TEST_TYPES: std::cell::Cell<Option<&'static ModFeatureTypes>> =
        const { std::cell::Cell::new(None) };
}

/// Install the process-wide table once; a second installation keeps the first.
pub fn install_mod_feature_types(types: ModFeatureTypes) -> Result<(), ModFeatureTypes> {
    MOD_TYPES.set(types)
}

/// The installed feature types: empty until the executable installs the mods', and a test's own
/// while one is installed.
pub fn mod_feature_types() -> &'static ModFeatureTypes {
    #[cfg(test)]
    if let Some(types) = TEST_TYPES.get() {
        return types;
    }
    MOD_TYPES.get().unwrap_or(&EMPTY)
}

static EMPTY: ModFeatureTypes = BTreeMap::new();

#[cfg(test)]
/// Runs `with` over a test's own feature types, as decoding reads them.
pub fn with_test_feature_types<T>(types: &ModFeatureTypes, with: impl FnOnce() -> T) -> T {
    let leaked: &'static ModFeatureTypes = Box::leak(Box::new(types.clone()));
    let prev = TEST_TYPES.replace(Some(leaked));
    let out = with();
    TEST_TYPES.set(prev);
    out
}

#[cfg(test)]
#[path = "mod_features/tests.rs"]
mod tests;
