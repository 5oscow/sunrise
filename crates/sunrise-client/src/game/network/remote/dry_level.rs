use super::*;

/// The client's world for a guess at what a click does: reads go to the
/// world, writes stay here and are dropped.
pub(super) struct DryLevel<'w> {
    base: &'w dyn sunrise_world::level::LevelAccess,
    pub(super) changed: sunrise_world::level::PreviewBlocks,
    random: sunrise_core::random::LegacyRandom,
    /// The sounds the use plays (a door's creak).
    pub(super) sounds: Vec<DrySound>,
    /// The live level's ticks a mod's placement and survival hooks read, where the guess
    /// schedules into scratch ones; `None` where it runs on the live ticks.
    pub(super) reads: Option<&'w sunrise_world::level::Ticks>,
    /// Whether the guess asks mods' placement and survival hooks: a click does, a look at what
    /// a click would do (the controller guide) leaves them to the templates' rules.
    pub(super) asks_mods: bool,
}

impl<'w> DryLevel<'w> {
    pub(super) fn new(base: &'w dyn sunrise_world::level::LevelAccess) -> Self {
        Self {
            base,
            changed: HashMap::new(),
            random: sunrise_core::random::LegacyRandom::new(0),
            sounds: Vec::new(),
            reads: None,
            asks_mods: true,
        }
    }
}

impl sunrise_world::level::BlockEntityNeighbors for DryLevel<'_> {}

impl sunrise_world::level::LevelAccess for DryLevel<'_> {
    fn before_smoke_source(&self, state: BlockStateId, smoke: &mut Option<bool>) {
        self.base.before_smoke_source(state, smoke);
    }
    fn cake_can_eat(&self, vanilla: bool) -> bool {
        self.base.cake_can_eat(vanilla)
    }
    fn emit(&mut self, event: sunrise_world::level::LevelEvent) {
        if let sunrise_world::level::LevelEvent::Sound {
            pos,
            sound,
            volume,
            pitch,
        } = event
        {
            self.sounds.push((pos, sound, volume, pitch));
        }
    }
    fn block_state(&self, pos: sunrise_core::position::BlockPos) -> BlockStateId {
        self.changed
            .get(&pos)
            .copied()
            .unwrap_or_else(|| self.base.block_state(pos))
    }
    fn set_raw(
        &mut self,
        pos: sunrise_core::position::BlockPos,
        state: BlockStateId,
    ) -> Option<BlockStateId> {
        let previous = self.block_state(pos);
        self.changed.insert(pos, state);
        Some(previous)
    }
    fn is_loaded(&self, pos: sunrise_core::position::BlockPos) -> bool {
        self.base.is_loaded(pos)
    }
    /// `Level.isClientSide`: the preview keeps its live level's side.
    fn is_client_side(&self) -> bool {
        self.base.is_client_side()
    }
    /// A block this guess changed has no block entity left, so it reads as a
    /// missing one; every other position asks the level it stands in for.
    fn shulker_is_closed(&self, pos: sunrise_core::position::BlockPos) -> Option<bool> {
        if self.changed.contains_key(&pos) {
            return None;
        }
        self.base.shulker_is_closed(pos)
    }
    fn published_collision_shape(
        &self,
        pos: sunrise_core::position::BlockPos,
        state: sunrise_world::registry::BlockStateId,
    ) -> Option<sunrise_core::geometry::VoxelShape> {
        if self.changed.contains_key(&pos) {
            return None;
        }
        self.base.published_collision_shape(pos, state)
    }
    fn min_y(&self) -> i32 {
        self.base.min_y()
    }
    fn max_y(&self) -> i32 {
        self.base.max_y()
    }
    fn block_light(&self, pos: sunrise_core::position::BlockPos) -> i32 {
        self.base.block_light(pos)
    }
    fn sky_light(&self, pos: sunrise_core::position::BlockPos) -> i32 {
        self.base.sky_light(pos)
    }
    fn sky_darken(&self) -> i32 {
        self.base.sky_darken()
    }
    fn is_unobstructed(&self, pos: sunrise_core::position::BlockPos, state: BlockStateId) -> bool {
        self.base.is_unobstructed(pos, state)
    }
    fn random(&mut self) -> &mut sunrise_core::random::LegacyRandom {
        &mut self.random
    }
    /// A mod's bone meal answer is read, never grown here: the preview asks
    /// the level it stands in for, as `isValidBonemealTarget` is read on both
    /// sides.
    fn is_valid_bonemeal_target(
        &self,
        pos: sunrise_core::position::BlockPos,
        state: BlockStateId,
    ) -> Option<bool> {
        self.base.is_valid_bonemeal_target(pos, state)
    }
    /// A mod's placement and survival answers are the level's, asked as
    /// vanilla's client prediction asks them, over this guess's writes: a
    /// neighbour's shape update after the first one reads the blocks the guess
    /// set.
    #[allow(clippy::option_option)]
    fn before_state_for_placement(
        &self,
        ticks: &sunrise_world::level::Ticks,
        _: Option<&sunrise_world::level::PreviewBlocks>,
        state: BlockStateId,
        context: &sunrise_world::level::PlacementContext,
        fluid: sunrise_world::fluid::FluidState,
        answer: &mut Option<Option<BlockStateId>>,
    ) {
        if self.asks_mods {
            self.base.before_state_for_placement(
                self.reads.unwrap_or(ticks),
                Some(&self.changed),
                state,
                context,
                fluid,
                answer,
            );
        }
    }
    fn before_can_survive(
        &self,
        ticks: &sunrise_world::level::Ticks,
        _: Option<&sunrise_world::level::PreviewBlocks>,
        pos: sunrise_core::position::BlockPos,
        state: BlockStateId,
        survives: &mut Option<bool>,
    ) {
        if self.asks_mods {
            self.base.before_can_survive(
                self.reads.unwrap_or(ticks),
                Some(&self.changed),
                pos,
                state,
                survives,
            );
        }
    }
    fn before_can_sustain_plant(
        &self,
        ticks: &sunrise_world::level::Ticks,
        _: Option<&sunrise_world::level::PreviewBlocks>,
        state: BlockStateId,
        ground: BlockStateId,
        ground_pos: sunrise_core::position::BlockPos,
        sustains: &mut Option<bool>,
    ) {
        if self.asks_mods {
            self.base.before_can_sustain_plant(
                self.reads.unwrap_or(ticks),
                Some(&self.changed),
                state,
                ground,
                ground_pos,
                sustains,
            );
        }
    }
    fn before_may_place_on(
        &self,
        ticks: &sunrise_world::level::Ticks,
        _: Option<&sunrise_world::level::PreviewBlocks>,
        state: BlockStateId,
        ground: BlockStateId,
        ground_pos: sunrise_core::position::BlockPos,
        may: &mut Option<bool>,
    ) {
        if self.asks_mods {
            self.base.before_may_place_on(
                self.reads.unwrap_or(ticks),
                Some(&self.changed),
                state,
                ground,
                ground_pos,
                may,
            );
        }
    }
    fn game_time(&self) -> i64 {
        self.base.game_time()
    }
}
