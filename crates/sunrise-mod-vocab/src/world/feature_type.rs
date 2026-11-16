//! Feature types a trait mod registers: vanilla's `Feature` with its own `place`, as
//! [`Registrar::register_feature_type`](crate::Registrar::register_feature_type) hands it over
//! once at startup (NeoForge's registry `FEATURE`). A configured feature with
//! `"type": "modid:tree"` then places by the mod's code, wherever the game places features:
//! biome decoration, a nested feature (a random selector's choice), a sapling's growth, bone
//! meal, `/place feature` and `LevelRef::place_feature`.
//!
//! There is no `Host` here: decoration runs on the worldgen worker threads, so the mod sees only
//! the small context below, deterministic for the random it is given. A type keeps no mutable
//! state, because two workers can decorate neighbouring chunks at once. Everything around the
//! feature stays the game's own: the placed feature's modifiers, the biome's steps and the
//! decoration seeds.
use crate::wire::{Reader, Wire, WireError, Writer};
use crate::world::structure_type::{HeightmapType, StructureRandom};
use crate::{BlockKey, BlockMetadata, BlockState, BlockUpdateFlags};
use std::sync::Arc;
use sunrise_core::nbt::{Compound, Tag};

/// A feature's placement failed: the placement stops there, and the game reports it with the
/// feature's id, as it reports a built-in feature's failure. A feature that merely did not place
/// (a tree with no room) answers `Ok(false)` instead.
#[derive(Debug)]
// exhaustive: mods build it themselves as a feature's error answer
#[derive(sunrise_mod_api_macros::Wire)]
pub struct FeatureError(pub String);
impl std::fmt::Display for FeatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for FeatureError {}
impl From<(crate::Identifier, String)> for FeatureError {
    fn from((source, reason): (crate::Identifier, String)) -> Self {
        Self(format!("{source}: {reason}"))
    }
}

/// A mod's feature type: vanilla's `Feature`, whose code answers for a configured feature with
/// `"type": "modid:tree"`. Implemented once, kept by the game; deterministic and stateless.
pub trait FeatureType: Send + Sync + 'static {
    /// `Feature.place(level, generator, random, origin)`: place the feature at `origin`,
    /// answering whether it placed. Blocks written before an `Ok(false)` are kept, as vanilla
    /// keeps them; draws are never given back. The configured feature's settings are
    /// [`context.config()`](FeatureContext::config).
    fn place(
        &self,
        context: &mut dyn FeatureContext,
        origin: [i32; 3],
    ) -> Result<bool, FeatureError>;
}

crate::wire::wire_handlers::handler!(dyn FeatureType, "FeatureType");

/// What a feature's [`place`](FeatureType::place) sees: vanilla's `WorldGenLevel` around the
/// origin (a decoration region on a worker, or the live level for growth and commands) and the
/// caller's random. A read the level can't answer (outside the region decoration may read) is
/// `None`; a write it refuses (outside the region it may write) is `false`. Neither loads a
/// chunk.
pub trait FeatureContext {
    /// Immutable, load-owned block metadata, safe to borrow on this worker.
    fn metadata(&self) -> &dyn BlockMetadata;
    /// The configured feature's NBT: its whole JSON object (`type` and the settings next to it),
    /// converted once when the definition loads.
    fn config(&self) -> Arc<Tag>;
    /// The caller's random: the same stream the placement modifiers before this feature drew
    /// from, so draws stay in vanilla's order.
    fn random(&mut self) -> &mut dyn StructureRandom;
    /// The level's minimum block Y.
    fn min_y(&mut self) -> Option<i32>;
    /// The level's maximum block Y.
    fn max_y(&mut self) -> Option<i32>;
    /// The block's default state; resolve it once, not once per placed block.
    fn state(&self, block: BlockKey) -> BlockState;
    /// A block state with partial properties; missing properties keep defaults.
    fn state_with(&self, block: BlockKey, properties: &[(&str, &str)]) -> BlockState;
    /// `BlockState.isAir`.
    fn is_air(&self, state: BlockState) -> bool;
    /// The block at `pos`, or `None` where the level can't be read.
    fn block(&mut self, pos: [i32; 3]) -> Option<BlockState>;
    /// `BlockState.is(tag)` of the block at `pos`: whether it is in block tag `tag`
    /// (`minecraft:leaves`), or `None` where the level can't be read.
    fn block_in_tag(&mut self, pos: [i32; 3], tag: &str) -> Option<bool>;
    /// `LevelWriter.setBlock(pos, state, flags)`, as `Feature.setBlock` (flags
    /// `BlockUpdateFlags::NEIGHBORS | CLIENTS`) and the tree features (`| KNOWN_SHAPE`) call it:
    /// whether it was written.
    fn set_block(&mut self, pos: [i32; 3], state: BlockState, flags: BlockUpdateFlags) -> bool;
    /// `WorldGenLevel.getHeight` of the heightmap kind at (x, z).
    fn height(&mut self, kind: HeightmapType, x: i32, z: i32) -> Option<i32>;
    /// The block entity's saved NBT (its `id` and fields) for the block just placed at `pos`,
    /// as `BlockEntity.loadWithComponents`: whether the level took it.
    fn block_entity(&mut self, pos: [i32; 3], nbt: Compound) -> bool;
}

/// A registered API behaviour; the game builds its engine adapter at startup.
pub(crate) struct FeatureTypeInfo {
    pub(crate) id: Box<str>,
    pub(crate) ty: Arc<dyn FeatureType>,
}

/// Registered feature behaviours, for the hidden game integration boundary.
// hidden: the game's startup adapter reads the registered behaviours; mods declare them through the registrar.
#[doc(hidden)]
pub fn feature_type_declarations() -> Vec<(String, Arc<dyn FeatureType>)> {
    crate::parts::installed_feature_type_list()
        .iter()
        .map(|info| (info.id.to_string(), info.ty.clone()))
        .collect()
}
