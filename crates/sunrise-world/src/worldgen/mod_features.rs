//! Feature types a mod registers: vanilla's `Feature` with its own `place`, as a configured
//! feature with `"type": "modid:tree"` names it. Handed over once at startup and looked up by
//! `FeatureDefinition::decode` before its built-in fallback, so a mod feature runs through every
//! route a built-in one does: biome decoration on the worldgen workers, nested features,
//! growth, bone meal, `/place feature` and `LevelRef::place_feature`. Runs on worker threads,
//! so a type keeps no mutable state and its target carries no `Host`.
//!
//! A call's writes are staged ([`Staging`]): the feature reads its own writes and those of the
//! root's earlier calls. A call that answers `Ok` hands its writes to its root
//! ([`Provisional`]), and the level sees them, in order, only when the whole root answers `Ok`;
//! a failure, a panic or an exhausted budget anywhere in the root discards every call's writes,
//! so a failed root leaves nothing of the mod's behind. A failure or a panic also disables the
//! type for the rest of the run, and every later placement of it fails by name. The features
//! of the same root that are not a mod's read the level as it was before the root's mod writes.
use crate::heightmap::HeightmapType;
use crate::registry::{BlockRegistry, BlockStateId};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use sunrise_core::nbt::Compound;
use sunrise_core::random::RandomSource;

/// The work units one root placement lends its mod features, together: every query, write and
/// block entity a callback asks for is one unit. Separate from the graph's own budget, so the
/// built-in features keep their capacity.
pub const CUSTOM_WORK_BUDGET: u32 = 65_536;

/// A configured feature's config, as its type's adapter decoded it at load.
pub type ModFeatureConfig = Arc<dyn Any + Send + Sync>;

/// A feature type's engine side, as the executable's adapter sees it. `Send + Sync` because
/// neighbouring chunks decorate at once on worker threads; the same random gives the same
/// blocks.
pub trait ModFeatureType: Send + Sync {
    /// Decode a configured feature's JSON once at load (the settings sit inline next to
    /// `type`) against the type's declared schema; an error refuses the definition, naming it.
    fn prepare_config(&self, config: &serde_json::Value) -> Result<ModFeatureConfig, String>;
    /// `Feature.place`: true when the feature says it placed. An `Err` is the mod's failure.
    fn place(
        &self,
        target: &mut dyn ModFeatureTarget,
        random: &mut dyn ModFeatureRandom,
        origin: [i32; 3],
        // Decoded once and retained by the loaded definition.
        config: &(dyn Any + Send + Sync),
    ) -> Result<bool, String>;
}

/// The level a mod feature places in, object-safe: the caller's own world (a decoration
/// region, the live level), never a second block store. A read the world can't answer is
/// `None`, a write it refuses is `false`; neither loads a chunk.
pub trait ModFeatureTarget {
    /// The numeric block registry the states are of, for the adapter's state index.
    fn registry(&self) -> Option<&'static BlockRegistry>;
    fn block(&mut self, position: [i32; 3]) -> Option<BlockStateId>;
    /// `LevelWriter.setBlock` with vanilla's update `flags`: false outside the writable
    /// region or the build height, and for the state already there.
    fn set_block(&mut self, position: [i32; 3], state: BlockStateId, flags: i32) -> bool;
    /// Whether the block at `position` is in block tag `tag`.
    fn block_in_tag(&mut self, position: [i32; 3], tag: &str) -> Option<bool>;
    fn min_y(&mut self) -> Option<i32>;
    fn max_y(&mut self) -> Option<i32>;
    fn height(&mut self, kind: HeightmapType, x: i32, z: i32) -> Option<i32>;
    /// Block-entity data for the block this call placed at `position`; false anywhere else.
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

/// Staged block writes and block entities, in order, with the latest state at each position
/// kept apart so a read is one lookup however many writes came before it.
#[derive(Default)]
pub(crate) struct Pending {
    writes: Vec<([i32; 3], BlockStateId, i32)>,
    latest: HashMap<[i32; 3], BlockStateId>,
    /// Block entities in the order given; a replaced one leaves a hole.
    entities: Vec<Option<([i32; 3], Compound)>>,
    entity_at: HashMap<[i32; 3], usize>,
}
impl Pending {
    fn write(&mut self, position: [i32; 3], state: BlockStateId, flags: i32) {
        self.writes.push((position, state, flags));
        self.latest.insert(position, state);
    }
    fn state(&self, position: [i32; 3]) -> Option<BlockStateId> {
        self.latest.get(&position).copied()
    }
    fn set_entity(&mut self, position: [i32; 3], nbt: Compound) {
        self.drop_entity(position);
        self.entity_at.insert(position, self.entities.len());
        self.entities.push(Some((position, nbt)));
    }
    fn drop_entity(&mut self, position: [i32; 3]) {
        if let Some(at) = self.entity_at.remove(&position) {
            self.entities[at] = None;
        }
    }
    /// A successful call's writes join the root's: a block entity of an earlier call is dropped
    /// with the block this call replaces.
    fn absorb(&mut self, call: Pending) {
        for &(position, ..) in &call.writes {
            self.drop_entity(position);
        }
        for (position, state, flags) in call.writes {
            self.write(position, state, flags);
        }
        for (position, nbt) in call.entities.into_iter().flatten() {
            self.set_entity(position, nbt);
        }
    }
    /// The root answered `Ok`: the writes, in order, then the block entities of the blocks among
    /// them the level took. A write the level refuses now drops its block entity with it.
    pub(crate) fn commit<W>(self, world: &mut W)
    where
        W: super::feature::FeatureWorld + super::placement::PlacementContext,
    {
        let mut written: HashSet<[i32; 3]> = HashSet::new();
        for (position, state, flags) in self.writes {
            if world.set_block(position, state, flags).unwrap_or(false) {
                written.insert(position);
            } else {
                written.remove(&position);
            }
        }
        for (position, nbt) in self.entities.into_iter().flatten() {
            if written.contains(&position) {
                let _ = world.set_block_entity(position, nbt);
            }
        }
    }
}

/// The writes of a root's successful mod calls, waiting for the root's answer.
pub(crate) type Provisional = Pending;

/// One call's view of a placement world: reads see the call's own staged writes and those of
/// the root's earlier calls, writes and block entities wait for the root
/// ([`finish`](Self::finish)), and every operation is charged to the root's custom budget. An
/// operation past the budget answers `None` or `false` and marks the call exhausted.
pub(crate) struct Staging<'a, W> {
    world: &'a mut W,
    budget: &'a Cell<u32>,
    earlier: &'a RefCell<Provisional>,
    own: Pending,
    pub(crate) exhausted: bool,
}
impl<'a, W> Staging<'a, W>
where
    W: super::feature::FeatureWorld + super::placement::PlacementContext,
{
    pub(crate) fn new(
        world: &'a mut W,
        budget: &'a Cell<u32>,
        earlier: &'a RefCell<Provisional>,
    ) -> Self {
        Self {
            world,
            budget,
            earlier,
            own: Pending::default(),
            exhausted: false,
        }
    }
    fn charge(&mut self) -> bool {
        match self.budget.get().checked_sub(1) {
            Some(left) => {
                self.budget.set(left);
                true
            }
            None => {
                self.exhausted = true;
                false
            }
        }
    }
    fn staged(&self, position: [i32; 3]) -> Option<BlockStateId> {
        self.own
            .state(position)
            .or_else(|| self.earlier.borrow().state(position))
    }
    fn current(&mut self, position: [i32; 3]) -> Option<BlockStateId> {
        match self.staged(position) {
            Some(state) => Some(state),
            None => self.world.block(position).ok(),
        }
    }
    /// The call answered `Ok`: its writes wait with its root's.
    pub(crate) fn finish(self) {
        self.earlier.borrow_mut().absorb(self.own);
    }
}
impl<W> ModFeatureTarget for Staging<'_, W>
where
    W: super::feature::FeatureWorld + super::placement::PlacementContext,
{
    fn registry(&self) -> Option<&'static BlockRegistry> {
        self.world.block_registry()
    }
    fn block(&mut self, position: [i32; 3]) -> Option<BlockStateId> {
        if !self.charge() {
            return None;
        }
        self.current(position)
    }
    fn set_block(&mut self, position: [i32; 3], state: BlockStateId, flags: i32) -> bool {
        if !self.charge() {
            return false;
        }
        let writable = self.world.ensure_can_write(position) == Ok(true)
            && self.world.inside_build_height(position) == Ok(true);
        // `Level.setBlock` answers false for the state already there.
        if !writable
            || self
                .current(position)
                .is_none_or(|current| current == state)
        {
            return false;
        }
        self.own.write(position, state, flags);
        true
    }
    fn block_in_tag(&mut self, position: [i32; 3], tag: &str) -> Option<bool> {
        if !self.charge() {
            return None;
        }
        match self.staged(position) {
            Some(state) => self.world.state_matches_tag(state, tag).ok(),
            None => self.world.block_matches_tag(position, tag).ok(),
        }
    }
    fn min_y(&mut self) -> Option<i32> {
        if !self.charge() {
            return None;
        }
        self.world.min_y().ok()
    }
    fn max_y(&mut self) -> Option<i32> {
        if !self.charge() {
            return None;
        }
        self.world.max_y().ok()
    }
    fn height(&mut self, kind: HeightmapType, x: i32, z: i32) -> Option<i32> {
        if !self.charge() {
            return None;
        }
        self.world.height(kind, x, z).ok()
    }
    fn set_block_entity(&mut self, position: [i32; 3], nbt: Compound) -> bool {
        if !self.charge() || self.own.state(position).is_none() {
            return false;
        }
        self.own.set_entity(position, nbt);
        true
    }
}

/// A registered type and whether a failure has disabled it.
pub struct ModFeatureEntry {
    pub ty: Arc<dyn ModFeatureType>,
    disabled: AtomicBool,
}
impl ModFeatureEntry {
    pub fn new(ty: Arc<dyn ModFeatureType>) -> Self {
        Self {
            ty,
            disabled: AtomicBool::new(false),
        }
    }
    pub fn disabled(&self) -> bool {
        self.disabled.load(Ordering::Relaxed)
    }
    pub(crate) fn disable(&self) {
        self.disabled.store(true, Ordering::Relaxed);
    }
}

/// A configured feature of a mod's type: the type and its config, decoded once at load.
#[derive(Clone)]
pub struct ModFeature {
    pub id: String,
    pub entry: Arc<ModFeatureEntry>,
    pub config: ModFeatureConfig,
}
impl std::fmt::Debug for ModFeature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModFeature")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// Why a mod feature's call placed nothing; its staged writes are discarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModFeatureError {
    /// The type's `place` answered `Err` (its reason); the type is disabled.
    Failed(String),
    /// The type's `place` panicked; the type is disabled.
    Panicked,
    /// The root's custom work budget ran out during the call.
    WorkLimit,
    /// An earlier failure disabled the type; it was not called.
    Disabled,
}
impl std::fmt::Display for ModFeatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(reason) => f.write_str(reason),
            Self::Panicked => f.write_str("panicked"),
            Self::WorkLimit => write!(f, "custom work budget of {CUSTOM_WORK_BUDGET} exceeded"),
            Self::Disabled => f.write_str("disabled by an earlier failure"),
        }
    }
}

impl ModFeature {
    /// One call of the type at `origin` over `world`, charged to `budget`: its writes join the
    /// root's `provisional` on `Ok` and reach the world when the root answers `Ok`.
    pub(crate) fn place<W>(
        &self,
        world: &mut W,
        random: &mut impl RandomSource,
        origin: [i32; 3],
        budget: &Cell<u32>,
        provisional: &RefCell<Provisional>,
    ) -> Result<bool, ModFeatureError>
    where
        W: super::feature::FeatureWorld + super::placement::PlacementContext,
    {
        if self.entry.disabled() {
            return Err(ModFeatureError::Disabled);
        }
        let mut staging = Staging::new(world, budget, provisional);
        let mut draws = Draws(random);
        let called = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.entry
                .ty
                .place(&mut staging, &mut draws, origin, &*self.config)
        }));
        match called {
            Err(_) => {
                self.entry.disable();
                Err(ModFeatureError::Panicked)
            }
            Ok(Err(reason)) => {
                self.entry.disable();
                Err(ModFeatureError::Failed(reason))
            }
            Ok(Ok(_)) if staging.exhausted => Err(ModFeatureError::WorkLimit),
            Ok(Ok(placed)) => {
                staging.finish();
                Ok(placed)
            }
        }
    }
}

/// The installed types, `modid:tree` → type. Set once, before any world opens (the executable
/// hands the mods' registrations over), and read while decoding features.
pub type ModFeatureTypes = BTreeMap<String, Arc<dyn ModFeatureType>>;
type Entries = BTreeMap<String, Arc<ModFeatureEntry>>;

static MOD_TYPES: OnceLock<Entries> = OnceLock::new();

#[cfg(test)]
thread_local! {
    /// The types a test installs, in place of the process-wide table.
    static TEST_TYPES: std::cell::Cell<Option<&'static Entries>> =
        const { std::cell::Cell::new(None) };
}

fn entries(types: ModFeatureTypes) -> Entries {
    types
        .into_iter()
        .map(|(id, ty)| (id, Arc::new(ModFeatureEntry::new(ty))))
        .collect()
}

/// Install the process-wide table once; a second installation keeps the first and hands its
/// own types back.
pub fn install_mod_feature_types(types: ModFeatureTypes) -> Result<(), ModFeatureTypes> {
    let mut offered = Some(types);
    MOD_TYPES.get_or_init(|| entries(offered.take().unwrap_or_default()));
    offered.map_or(Ok(()), Err)
}

/// The installed feature type `id`: none until the executable installs the mods', and a test's
/// own while one is installed.
pub fn mod_feature_type(id: &str) -> Option<&'static Arc<ModFeatureEntry>> {
    #[cfg(test)]
    if let Some(types) = TEST_TYPES.get() {
        return types.get(id);
    }
    MOD_TYPES.get()?.get(id)
}

#[cfg(test)]
/// Runs `with` over a test's own feature types, as decoding reads them.
pub fn with_test_feature_types<T>(types: &ModFeatureTypes, with: impl FnOnce() -> T) -> T {
    let leaked: &'static Entries = Box::leak(Box::new(entries(types.clone())));
    let prev = TEST_TYPES.replace(Some(leaked));
    let out = with();
    TEST_TYPES.set(prev);
    out
}

#[cfg(test)]
#[path = "mod_features/tests.rs"]
mod tests;
