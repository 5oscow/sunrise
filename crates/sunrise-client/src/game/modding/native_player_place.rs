//! Placement-state reads, placement writes and the `Block.setPlacedBy` step.
use super::{
    Dim, Game,
    api_values::{TryIntoApi, TryIntoEngine, block_state::block_listens},
    native_level_signals::{ReadLevel, ResidentRead},
    native_server::PlayerRead,
};
use mod_api::{
    BlockKey, BlockUpdateFlags, HookId, LevelRef, LevelSide, LevelWriteError, PlayerRef,
    PlayerWorldRefusal as Refusal,
};
use sunrise_client::{containers::BlockEntity, items::ItemStack};
use sunrise_core::{
    connection_kind::ConnectionKind,
    geometry::{Direction, Vec3, VoxelShape, border::BorderGeometry},
    position::BlockPos,
    random::LegacyRandom,
};
use sunrise_world::{
    fluid::FluidState,
    level::{
        BlockEntityNeighbors, LevelAccess, PlacementContext, PreviewBlocks, Ticks, classes::Class,
        signal_access::SignalAccess,
    },
    registry::BlockStateId,
};

/// A placement's write flags: neighbours and clients told, shapes updated unless known.
fn placement_flags(update_neighbour_shapes: bool) -> BlockUpdateFlags {
    let flags = BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS;
    if update_neighbour_shapes {
        flags
    } else {
        flags | BlockUpdateFlags::KNOWN_SHAPE
    }
}

impl Game<'_> {
    /// The one block write (`level_set_block`, its block entity included) with a placement's
    /// flags, then `Block.setPlacedBy` in the player's turn; nothing else of `BlockItem.place`.
    pub(super) fn player_place_block(
        &mut self,
        player: PlayerRef,
        level: LevelRef,
        pos: [i32; 3],
        state: BlockStateId,
        update_neighbour_shapes: bool,
    ) -> Result<(), Refusal> {
        if !self.is_logical_server() {
            return Err(Refusal::Client);
        }
        let dead = match self.player_handle_read(player).ok_or(Refusal::Stale)? {
            PlayerRead::Acting { .. } => self
                .player
                .as_ref()
                .is_none_or(|p| p.health.health() <= 0.0),
            PlayerRead::Guest(g) => g.dead,
        };
        if dead {
            return Err(Refusal::Dead);
        }
        if self.pack.assets.registry.state(state).is_none() {
            return Err(Refusal::UnknownState);
        }
        if self.native_level_world(level).is_none() {
            return Err(Refusal::StaleLevel);
        }
        if level.dimension() != self.dim.key() {
            return Err(Refusal::OtherLevel);
        }
        let [x, y, z] = pos;
        if !self.world.shape.contains_y(y)
            || !(-30_000_000..30_000_000).contains(&x)
            || !(-30_000_000..30_000_000).contains(&z)
        {
            return Err(Refusal::OutOfWorld { pos });
        }
        if !self.world.loaded(x, z) {
            return Err(Refusal::NotLoaded { pos });
        }
        if self.world.block(x, y, z) == state {
            return Err(Refusal::Unchanged);
        }
        let flags = placement_flags(update_neighbour_shapes);
        if !self
            .placement_allowed(pos, state, Some(flags))
            .map_err(|_| Refusal::InTransaction)?
        {
            return Err(Refusal::Fenced);
        }
        // The write is `level_set_block`'s, on the level as it stands (a guest's turn is not
        // the authoritative level); only `setPlacedBy` runs in the player's turn.
        match self.level_set_block(level, pos, state, flags) {
            Ok(true) => {}
            Ok(false) => return Err(Refusal::Unchanged),
            Err(_) => return Err(Refusal::Stale),
        }
        let ran = self.native_as_player_handle(player, |game| {
            game.run_mod_placement_callback(pos, state, None);
        });
        if ran { Ok(()) } else { Err(Refusal::Stale) }
    }
    pub(super) fn level_state_for_placement(
        &self,
        level: LevelRef,
        block: BlockKey,
        context: &mod_api::PlacementContext,
    ) -> Option<BlockStateId> {
        let world = self.native_level_world(level)?;
        let BlockPos { x, y, z } = context.pos;
        if !(-30_000_000..30_000_000).contains(&x)
            || !(-30_000_000..30_000_000).contains(&z)
            || !world.is_loaded_at(x, y, z)
        {
            return None;
        }
        let b = self.res.behaviours();
        let block = super::api_values::behavior_block(block, b)?;
        let (dim, ticks, lid_openers, anims, border) = if self.dim.key() == level.dimension() {
            (
                self.dim,
                &self.block_ticks,
                &self.lid_openers,
                &self.be_anims,
                &self.border,
            )
        } else {
            let dim = Dim::from_id(level.dimension_id());
            let (ticks, lid_openers) = self.levels.parked_of(dim.id())?.signal_state();
            (
                dim,
                ticks,
                lid_openers,
                self.levels.parked_anims(dim)?,
                self.levels.parked_border(dim)?,
            )
        };
        let item = match context.item {
            Some(key) => mod_api::__private::item_key_id(key)?,
            None => "",
        };
        let resolved = PlacementContext {
            pos: context.pos,
            hit_pos: context.hit_pos,
            face: context.face,
            hit: Vec3::new(context.hit[0], context.hit[1], context.hit[2]),
            inside: context.inside,
            yaw: context.yaw,
            pitch: context.pitch,
            sneaking: context.sneaking,
            item: item.to_owned(),
            replace_clicked: context.replace_clicked,
        };
        let mut probe = PlacementProbe {
            parts: ReadLevel {
                game: self,
                level: ResidentRead {
                    level,
                    dim,
                    world,
                    anims,
                    lid_openers,
                    signals: Some(ticks),
                    collision_shapes: true,
                },
            },
            border: border.geometry(),
            random: (*world.random).clone(),
            context,
        };
        b.state_for_resolved_placement(&mut probe, ticks, block, &resolved)
    }

    /// A chosen state written as a placement writes it, then `Block.setPlacedBy`, with no
    /// player: the write and the extra halves are those of `player_place_block`. `Ok(false)`
    /// when the write was refused or changed nothing.
    pub(super) fn level_place_block(
        &mut self,
        level: LevelRef,
        pos: [i32; 3],
        state: BlockStateId,
        update_neighbour_shapes: bool,
    ) -> Result<bool, LevelWriteError> {
        let flags = placement_flags(update_neighbour_shapes);
        if !self.native_level_write_position(level, pos)?
            || self.world.block(pos[0], pos[1], pos[2]) == state
            || !self.placement_allowed(pos, state, Some(flags))?
        {
            return Ok(false);
        }
        if !self.level_set_block(level, pos, state, flags)? {
            return Ok(false);
        }
        self.run_mod_placement_callback(pos, state, None);
        Ok(true)
    }
    /// `Block.setPlacedBy` of a block already standing at `pos`, with no placer. Its stack
    /// is consulted, not placed: the caller has already loaded any block entity data.
    pub(super) fn level_set_placed_by(
        &mut self,
        level: LevelRef,
        pos: [i32; 3],
        stack: Option<&ItemStack>,
    ) -> Result<bool, LevelWriteError> {
        if !self.native_level_write_position(level, pos)? {
            return Ok(false);
        }
        let state = self.world.block(pos[0], pos[1], pos[2]);
        if !self.placement_allowed(pos, state, None)? {
            return Ok(false);
        }
        self.run_mod_placement_callback(pos, state, stack);
        Ok(true)
    }

    fn placement_allowed(
        &mut self,
        pos: [i32; 3],
        state: BlockStateId,
        write_flags: Option<BlockUpdateFlags>,
    ) -> Result<bool, LevelWriteError> {
        if self.world.carrier_fences.has_locked_cells() {
            let at = BlockPos::new(pos[0], pos[1], pos[2]);
            if self.world.carrier_fences.locks_cell(&pos)
                || self
                    .preview_set_placed_by(at, state, write_flags.map(|flags| flags.bits()))
                    .keys()
                    .any(|p| self.world.carrier_fences.locks_cell(&[p.x, p.y, p.z]))
            {
                return Ok(false);
            }
        }
        if !self.transactions.is_empty() && self.wither_spawn_pattern(pos, state).is_some() {
            // The pattern's flag-2 writes, destroy events and neighbour pass are not journalled.
            return Err(LevelWriteError::InTransaction);
        }
        Ok(true)
    }

    /// Chosen-state mod placements did not run the simulation's block-item placement.
    /// Apply its state-only callbacks before the same game-level step used by an item.
    fn run_mod_placement_callback(
        &mut self,
        pos: [i32; 3],
        expected: BlockStateId,
        stack: Option<&ItemStack>,
    ) {
        let [x, y, z] = pos;
        let placed = self.world.block(x, y, z);
        let behaviours = self.res.behaviours();
        if behaviours.block(placed) != behaviours.block(expected) {
            return;
        }
        self.with_level(|b, level, ticks| {
            b.set_placed_by_state(level, ticks, BlockPos::new(x, y, z), placed);
        });
        let _ = self.set_placed_by(pos, None, stack);
        let _ = self.process_level_events();
    }

    /// The game-level part of `Block.setPlacedBy`, once the state-only part has run.
    /// A supplied player is already the acting player. A supplied stack is read only.
    pub(super) fn set_placed_by(
        &mut self,
        pos: [i32; 3],
        placer: Option<PlayerRef>,
        stack: Option<&ItemStack>,
    ) -> crate::Result<()> {
        let [x, y, z] = pos;
        let key = (x, y, z);
        let placed = self.world.block(x, y, z);
        if self.res.behaviours().data(placed).is(Class::Jukebox) {
            if let Some(stack) = stack {
                self.jukebox_placed(pos, stack);
            }
        } else {
            match self.world.block_entities.get(&key) {
                Some(BlockEntity::CommandBlock(_)) => self.command_block_placed(key, stack),
                // StructureBlock.createdBy accepts any living placer; this route's placers are players.
                Some(BlockEntity::StructureBlock(_)) if placer.is_some() => {
                    self.structure_block_placed(key)
                }
                // The sign editor is a ServerPlayer side effect, not a block-state or NBT write.
                Some(BlockEntity::Sign(sign))
                    if placer.is_some()
                        && !sign.waxed
                        && sign.front.has_editable_text()
                        && !self.fake_blocks(mod_api::FakeEffect::Menu) =>
                {
                    self.open_text_edit(key, true);
                }
                _ => {}
            }
        }
        self.check_wither_spawn(pos)
    }
}

/// Give the placement simulation the resident world's reads, without lending it
/// world storage, the level random or any effect queues.
struct PlacementProbe<'g, 'a> {
    parts: ReadLevel<'g, 'a, ResidentRead<'g>>,
    border: &'g BorderGeometry,
    random: LegacyRandom,
    context: &'g mod_api::PlacementContext,
}

impl BlockEntityNeighbors for PlacementProbe<'_, '_> {}

impl LevelAccess for PlacementProbe<'_, '_> {
    fn block_state(&self, pos: BlockPos) -> BlockStateId {
        self.parts.block_state(pos)
    }
    fn set_raw(&mut self, _: BlockPos, _: BlockStateId) -> Option<BlockStateId> {
        None
    }
    fn is_loaded(&self, pos: BlockPos) -> bool {
        self.parts.level.world.loaded(pos.x, pos.z)
    }
    fn min_y(&self) -> i32 {
        self.parts.level.world.bottom_y()
    }
    fn max_y(&self) -> i32 {
        self.parts.level.world.top_y()
    }
    fn random(&mut self) -> &mut LegacyRandom {
        &mut self.random
    }
    fn game_time(&self) -> i64 {
        self.parts.level.world.game_time
    }
    fn block_light(&self, pos: BlockPos) -> i32 {
        i32::from(self.parts.level.world.light(pos.x, pos.y, pos.z) & 15)
    }
    fn sky_light(&self, pos: BlockPos) -> i32 {
        i32::from(self.parts.level.world.light(pos.x, pos.y, pos.z) >> 4)
    }
    fn within_world_border(&self, pos: BlockPos) -> bool {
        self.border
            .is_within_bounds(f64::from(pos.x), f64::from(pos.z), 0.0)
    }
    fn dimension(&self) -> sunrise_core::dimension::Dimension {
        self.parts.level.dim.level()
    }
    fn redstone_experiments(&self) -> bool {
        self.parts
            .game
            .enabled_feature_ids()
            .iter()
            .any(|id| id == "minecraft:redstone_experiments")
    }
    fn shulker_is_closed(&self, pos: BlockPos) -> Option<bool> {
        self.parts.shulker_is_closed(pos)
    }
    fn published_collision_shape(&self, pos: BlockPos, state: BlockStateId) -> Option<VoxelShape> {
        self.parts.published_collision_shape(pos, state)
    }
    fn adjust_signal_source(&self, state: BlockStateId, vanilla: bool) -> bool {
        self.parts.adjust_signal_source(state, vanilla)
    }
    fn has_signal_hook(&self) -> bool {
        mod_api::__private::has_scoped_hook(HookId::BlockSignal)
            || mod_api::__private::has_scoped_hook(HookId::IsSignalSource)
    }
    fn has_weak_power_hook(&self) -> bool {
        self.parts.has_weak_power_hook()
    }
    fn adjust_should_check_weak_power(
        &self,
        pos: BlockPos,
        state: BlockStateId,
        direction: Direction,
        vanilla: bool,
    ) -> bool {
        self.parts
            .adjust_should_check_weak_power(pos, state, direction, vanilla)
    }
    fn has_redstone_wire_connection_hook(&self) -> bool {
        self.parts.has_redstone_wire_connection_hook()
    }
    fn adjust_should_redstone_wire_connect_to(
        &self,
        pos: BlockPos,
        state: BlockStateId,
        direction: Option<Direction>,
        vanilla: bool,
    ) -> bool {
        self.parts
            .adjust_should_redstone_wire_connect_to(pos, state, direction, vanilla)
    }
    fn adjust_signal(
        &self,
        pos: BlockPos,
        state: BlockStateId,
        direction: Direction,
        direct: bool,
        vanilla: i32,
    ) -> i32 {
        self.parts
            .adjust_signal(pos, state, direction, direct, vanilla)
    }
    fn block_entity_signal(&self, pos: BlockPos, state: BlockStateId, direction: Direction) -> i32 {
        self.parts.block_entity_signal(pos, state, direction)
    }
    fn has_connection_hook(&self) -> bool {
        mod_api::__private::has_scoped_hook(HookId::ConnectsTo)
            || mod_api::__private::has_scoped_hook(HookId::CanBeConnectedTo)
    }
    fn adjust_can_be_connected_to(
        &self,
        kind: ConnectionKind,
        pos: BlockPos,
        state: BlockStateId,
        connector: BlockStateId,
        direction: Direction,
        face_solid: bool,
        vanilla: bool,
    ) -> bool {
        if !block_listens(HookId::CanBeConnectedTo, state) {
            return vanilla;
        }
        let (Ok(state), Ok(connector)) = (state.try_into_api(), connector.try_into_api()) else {
            return vanilla;
        };
        let mut answer = None;
        if let Err(error) = sunrise_client::crash::contain(|| {
            mod_api::dispatch_can_be_connected_to(
                &self.parts.signal_part_view(),
                mod_api::__private::CanBeConnectedToCall {
                    dimension: self.dimension(),
                    pos,
                    state,
                    connector,
                    kind,
                    direction,
                    face_solid,
                    answer: &mut answer,
                },
            );
        }) {
            log::warn!("can_be_connected_to callback failed: {error}");
            return vanilla;
        }
        answer.unwrap_or(vanilla)
    }
    fn adjust_connects_to(
        &self,
        kind: ConnectionKind,
        pos: BlockPos,
        state: BlockStateId,
        neighbour: BlockStateId,
        direction: Direction,
        face_solid: bool,
        vanilla: bool,
    ) -> bool {
        if !block_listens(HookId::ConnectsTo, state) {
            return vanilla;
        }
        let (Ok(state), Ok(neighbour)) = (state.try_into_api(), neighbour.try_into_api()) else {
            return vanilla;
        };
        let mut connected = vanilla;
        mod_api::dispatch_connects_to(
            &self.parts.signal_part_view(),
            mod_api::__private::ConnectsToCall {
                dimension: self.dimension(),
                pos,
                state,
                kind,
                neighbour,
                direction,
                face_solid,
                connected: &mut connected,
            },
        );
        connected
    }
    #[allow(clippy::option_option)]
    fn before_state_for_placement(
        &self,
        _: &Ticks,
        _: Option<&PreviewBlocks>,
        state: BlockStateId,
        _: &PlacementContext,
        fluid: FluidState,
        answer: &mut Option<Option<BlockStateId>>,
    ) {
        if !block_listens(HookId::StateForPlacement, state) {
            return;
        }
        let (Ok(state), Some(fluid)) = (
            state.try_into_api(),
            super::native_world_read::api_fluid_state(fluid),
        ) else {
            return;
        };
        let mut candidate = None;
        mod_api::dispatch_state_for_placement(
            LevelSide::Server(&self.parts),
            self.dimension(),
            state,
            self.context,
            fluid,
            &mut candidate,
        );
        match candidate.try_into_engine(&()) {
            Ok(candidate) => *answer = candidate,
            Err(error) => log::warn!("mod block state write refused: {error:?}"),
        }
    }
    fn before_can_survive(
        &self,
        _: &Ticks,
        _: Option<&PreviewBlocks>,
        pos: BlockPos,
        state: BlockStateId,
        survives: &mut Option<bool>,
    ) -> bool {
        if !block_listens(HookId::CanSurvive, state) {
            return false;
        }
        let Ok(state) = state.try_into_api() else {
            return false;
        };
        let mut inert = false;
        mod_api::dispatch_can_survive(
            LevelSide::Server(&self.parts),
            self.dimension(),
            pos,
            state,
            survives,
            &mut inert,
        );
        inert
    }
    fn before_can_sustain_plant(
        &self,
        _: &Ticks,
        _: Option<&PreviewBlocks>,
        state: BlockStateId,
        ground: BlockStateId,
        ground_pos: BlockPos,
        sustains: &mut Option<bool>,
    ) -> bool {
        if !block_listens(HookId::CanSustainPlant, ground) {
            return false;
        }
        let (Ok(state), Ok(ground)) = (state.try_into_api(), ground.try_into_api()) else {
            return false;
        };
        let mut inert = false;
        mod_api::dispatch_can_sustain_plant(
            LevelSide::Server(&self.parts),
            self.dimension(),
            state,
            ground,
            ground_pos,
            sustains,
            &mut inert,
        );
        inert
    }
    fn before_may_place_on(
        &self,
        _: &Ticks,
        _: Option<&PreviewBlocks>,
        state: BlockStateId,
        ground: BlockStateId,
        ground_pos: BlockPos,
        may: &mut Option<bool>,
    ) -> bool {
        if !block_listens(HookId::MayPlaceOn, state) {
            return false;
        }
        let (Ok(state), Ok(ground)) = (state.try_into_api(), ground.try_into_api()) else {
            return false;
        };
        let mut inert = false;
        mod_api::dispatch_may_place_on(
            LevelSide::Server(&self.parts),
            self.dimension(),
            state,
            ground,
            ground_pos,
            may,
            &mut inert,
        );
        inert
    }
    fn before_fire_odds(
        &self,
        state: BlockStateId,
        pos: BlockPos,
        face: Direction,
        odds: &mut Option<(i32, i32)>,
    ) {
        if block_listens(HookId::FireOdds, state)
            && let Ok(state) = state.try_into_api()
        {
            mod_api::dispatch_fire_odds(
                LevelSide::Server(&self.parts.signal_part_view()),
                self.dimension(),
                state,
                pos,
                face,
                odds,
            );
        }
    }
    fn before_smoke_source(&self, state: BlockStateId, smoke: &mut Option<bool>) {
        if block_listens(HookId::SmokeSource, state)
            && let Ok(state) = state.try_into_api()
        {
            mod_api::dispatch_smoke_source(
                LevelSide::Server(&self.parts.signal_part_view()),
                self.dimension(),
                state,
                smoke,
            );
        }
    }
    fn fence_gate_in_wall(&self, state: BlockStateId) -> Option<bool> {
        if !block_listens(HookId::FenceGateInWall, state) {
            return None;
        }
        let mut answer = None;
        mod_api::dispatch_fence_gate_in_wall(
            LevelSide::Server(&self.parts.signal_part_view()),
            state.try_into_api().ok()?,
            &mut answer,
        );
        answer
    }
}
