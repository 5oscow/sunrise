//! Shared level input for the canonical signal evaluator and its block Parts.
use super::api_values::{TryIntoApi, TryIntoEngine};
use super::{Dim, Game};
use mod_api::{LevelHost, LevelRef};
use std::collections::HashMap;
use sunrise_client::{block_entity_anim::BlockEntityAnims, items::Items, world::World};
use sunrise_core::{geometry::Direction, position::BlockPos};
use sunrise_world::{
    level::{Signals, Ticks, signal_access::SignalAccess},
    registry::BlockStateId,
};

/// Shared reads for block Parts. A bare `LevelRef` borrows the level in place;
/// placement passes the already-resolved data of the addressed resident level.
pub(super) struct ReadLevel<'g, 'a, L = LevelRef> {
    pub game: &'g Game<'a>,
    pub level: L,
}

#[derive(Clone, Copy)]
pub(super) struct ResidentRead<'g> {
    pub level: LevelRef,
    pub dim: Dim,
    pub world: &'g World,
    pub anims: &'g BlockEntityAnims,
    pub lid_openers: &'g HashMap<(i32, i32, i32), i32>,
    /// Expose signal reads to placement, but not to a signal Part evaluating one.
    pub signals: Option<&'g Ticks>,
    pub collision_shapes: bool,
}

pub(super) trait ReadScope<'g> {
    fn resident(&self, game: &'g Game<'_>) -> ResidentRead<'g>;
}

impl<'g> ReadScope<'g> for LevelRef {
    fn resident(&self, game: &'g Game<'_>) -> ResidentRead<'g> {
        ResidentRead {
            level: *self,
            dim: game.dim,
            world: &game.world,
            anims: &game.be_anims,
            lid_openers: &game.lid_openers,
            signals: None,
            collision_shapes: false,
        }
    }
}

impl<'g> ReadScope<'g> for ResidentRead<'g> {
    fn resident(&self, _: &'g Game<'_>) -> ResidentRead<'g> {
        *self
    }
}

impl<'g, 'a, L: ReadScope<'g>> ReadLevel<'g, 'a, L> {
    fn resident(&self) -> ResidentRead<'g> {
        self.level.resident(self.game)
    }

    pub(super) fn signal_part_view(&self) -> ReadLevel<'g, 'a, ResidentRead<'g>> {
        ReadLevel {
            game: self.game,
            level: ResidentRead {
                signals: None,
                ..self.resident()
            },
        }
    }

    fn signals<R>(&self, read: impl FnOnce(&Signals<'_, Self>) -> R) -> Option<R> {
        let ticks = self.resident().signals?;
        Some(read(&self.game.res.behaviours().signals(self, ticks)))
    }
}

/// The output side passed to Parts is opposite SignalGetter's input direction.
/// Both the shared view and the write context apply the same output bound.
pub(super) fn adjust_signal(
    direction: Direction,
    vanilla: i32,
    fire: impl FnOnce(Direction, &mut i32),
) -> i32 {
    let mut signal = vanilla;
    fire(direction.opposite(), &mut signal);
    signal.clamp(0, 15)
}

/// Block-entity-driven sources, shared by the read and write views.
pub(super) fn block_entity_signal(
    world: &World,
    registry: &sunrise_world::registry::BlockRegistry,
    items: &Items,
    lid_openers: &HashMap<(i32, i32, i32), i32>,
    pos: BlockPos,
    state: BlockStateId,
) -> i32 {
    if let Some(sunrise_client::containers::BlockEntity::TestBlock(block)) =
        world.block_entities.get(&(pos.x, pos.y, pos.z))
    {
        return super::test_blocks::own_signal(registry, state, block.powered);
    }
    if let Some(count) = lid_openers.get(&(pos.x, pos.y, pos.z)) {
        return (*count).clamp(0, 15);
    }
    super::jukebox::jukebox_signals(world, items, pos).map_or(0, |(signal, _)| signal)
}

impl<'g, L: ReadScope<'g>> SignalAccess for ReadLevel<'g, '_, L> {
    fn block_state(&self, pos: BlockPos) -> BlockStateId {
        self.resident().world.block(pos.x, pos.y, pos.z)
    }
    fn shulker_is_closed(&self, pos: BlockPos) -> Option<bool> {
        self.resident()
            .anims
            .shulkers
            .get(&[pos.x, pos.y, pos.z])
            .map(|s| s.status == sunrise_client::block_entity_anim::ShulkerStatus::Closed)
    }
    fn published_collision_shape(
        &self,
        pos: BlockPos,
        state: BlockStateId,
    ) -> Option<sunrise_core::geometry::VoxelShape> {
        if !self.game.collision.publishes_shapes(state) {
            return None;
        }
        match self
            .resident()
            .world
            .block_entities
            .get(&(pos.x, pos.y, pos.z))?
        {
            sunrise_client::containers::BlockEntity::Mod(entity) => {
                entity.shapes().map(|shapes| shapes.collision().clone())
            }
            _ => None,
        }
    }
    fn adjust_signal_source(&self, state: BlockStateId, vanilla: bool) -> bool {
        if !super::api_values::block_state::block_listens(mod_api::HookId::IsSignalSource, state) {
            return vanilla;
        }
        use super::api_values::TryIntoApi;
        let Ok(state) = state.try_into_api() else {
            return vanilla;
        };
        let mut source = vanilla;
        mod_api::dispatch_is_signal_source(&self.signal_part_view(), state, &mut source);
        source
    }
    fn has_weak_power_hook(&self) -> bool {
        mod_api::__private::has_scoped_hook(mod_api::HookId::ShouldCheckWeakPower)
    }
    fn adjust_should_check_weak_power(
        &self,
        pos: BlockPos,
        state: BlockStateId,
        direction: Direction,
        vanilla: bool,
    ) -> bool {
        if !super::api_values::block_state::block_listens(
            mod_api::HookId::ShouldCheckWeakPower,
            state,
        ) {
            return vanilla;
        }
        use super::api_values::TryIntoApi;
        let Ok(state) = state.try_into_api() else {
            return vanilla;
        };
        let mut check = vanilla;
        mod_api::dispatch_should_check_weak_power(
            &self.signal_part_view(),
            self.resident().dim.level(),
            pos,
            state,
            direction,
            &mut check,
        );
        check
    }
    fn has_redstone_wire_connection_hook(&self) -> bool {
        mod_api::__private::has_scoped_hook(mod_api::HookId::ShouldRedstoneWireConnectTo)
    }
    fn adjust_should_redstone_wire_connect_to(
        &self,
        pos: BlockPos,
        state: BlockStateId,
        direction: Option<Direction>,
        vanilla: bool,
    ) -> bool {
        if !super::api_values::block_state::block_listens(
            mod_api::HookId::ShouldRedstoneWireConnectTo,
            state,
        ) {
            return vanilla;
        }
        use super::api_values::TryIntoApi;
        let Ok(state) = state.try_into_api() else {
            return vanilla;
        };
        let mut connected = vanilla;
        mod_api::dispatch_should_redstone_wire_connect_to(
            &self.signal_part_view(),
            self.resident().dim.level(),
            pos,
            state,
            direction,
            &mut connected,
        );
        connected
    }
    fn adjust_signal(
        &self,
        pos: BlockPos,
        state: BlockStateId,
        direction: Direction,
        direct: bool,
        vanilla: i32,
    ) -> i32 {
        if !super::api_values::block_state::block_listens(mod_api::HookId::BlockSignal, state) {
            return vanilla;
        }
        use super::api_values::TryIntoApi;
        let Ok(state) = state.try_into_api() else {
            return vanilla;
        };
        adjust_signal(direction, vanilla, |face, signal| {
            mod_api::dispatch_block_signal(
                &self.signal_part_view(),
                self.resident().dim.level(),
                pos,
                state,
                face,
                direct,
                signal,
            );
        })
    }
    fn block_entity_signal(&self, pos: BlockPos, state: BlockStateId, _: Direction) -> i32 {
        let level = self.resident();
        block_entity_signal(
            level.world,
            self.game.pack.assets.registry,
            self.game.items(),
            level.lid_openers,
            pos,
            state,
        )
    }
}

impl<'g, L: ReadScope<'g>> LevelHost for ReadLevel<'g, '_, L> {
    fn block_face_sturdy(
        &self,
        pos: [i32; 3],
        face: mod_api::BlockFace,
        support: mod_api::SupportType,
    ) -> Option<bool> {
        self.game
            .level_block_face_sturdy(self.resident().level, pos, face, support)
    }
    fn can_see_sky(&self, pos: [i32; 3]) -> Option<bool> {
        self.game.level_can_see_sky(self.resident().level, pos)
    }
    fn block_entity(&self, pos: [i32; 3]) -> Option<mod_api::BlockEntityRef> {
        self.game.block_entity(self.resident().level, pos)
    }
    fn container_size(&self, at: mod_api::BlockSide) -> Option<usize> {
        let level = self.resident();
        if at.level != level.level || !level.world.is_loaded_at(at.pos[0], at.pos[1], at.pos[2]) {
            return None;
        }
        self.game.container_size(at)
    }
    fn container_stack(&self, at: mod_api::BlockSide, slot: usize) -> Option<mod_api::ItemStack> {
        let level = self.resident();
        if at.level != level.level || !level.world.is_loaded_at(at.pos[0], at.pos[1], at.pos[2]) {
            return None;
        }
        self.game.container_stack(at, slot)?.try_into_api().ok()
    }
    fn block_entity_part_snapshot(
        &self,
        entity: mod_api::BlockEntityRef,
        namespace: &str,
        name: &str,
    ) -> Result<mod_api::AttachmentBatch, mod_api::PartStateError> {
        let level = self.resident();
        super::native_block_entity_refs::part_snapshot(
            level.world,
            Some(level.level),
            self.game.native_authoritative(),
            self.game.attachment_indexes.get(&level.dim),
            entity,
            namespace,
            name,
        )
    }
    fn block_entity_data(&self, entity: mod_api::BlockEntityRef) -> Option<mod_api::Compound> {
        use mod_api::__private::game::Fields;
        (entity.fields().level == self.resident().level)
            .then(|| self.game.block_entity_data(entity))
            .flatten()
    }
    fn block_state(&self, [x, y, z]: [i32; 3]) -> Option<mod_api::BlockState> {
        self.resident().world.block(x, y, z).try_into_api().ok()
    }
    fn state_block(&self, state: mod_api::BlockState) -> Option<&str> {
        self.game
            .pack
            .assets
            .registry
            .state(state.try_into_engine(&()).ok()?)
            .map(|s| &*s.name)
    }
    fn block_state_id(&self, name: &str) -> Option<mod_api::BlockState> {
        self.game.block_state_id(name)?.try_into_api().ok()
    }
    fn block_state_default(&self, block: mod_api::BlockKey) -> Option<mod_api::BlockState> {
        self.game.block_state_default(block)?.try_into_api().ok()
    }
    fn state_string(&self, state: mod_api::BlockState) -> Option<String> {
        self.game
            .block_state_string(state.try_into_engine(&()).ok()?)
    }
    fn min_y(&self) -> i32 {
        self.resident().world.shape.min_y
    }
    fn height(&self) -> i32 {
        self.resident().world.shape.height
    }
    fn block_in_tag(&self, block: mod_api::BlockKey, tag: &str) -> Option<bool> {
        super::native_world_read::block_in_tag(block, tag)
    }
    fn tags(&self) -> &dyn mod_api::Tags {
        self.game
    }
    fn fluid_state(&self, [x, y, z]: [i32; 3]) -> Option<mod_api::FluidState> {
        let world = self.resident().world;
        world
            .is_loaded_at(x, y, z)
            .then(|| world.block(x, y, z))
            .and_then(|state| self.game.state_fluid_state(state))
    }
    fn state_fluid_state(&self, state: mod_api::BlockState) -> Option<mod_api::FluidState> {
        self.game
            .state_fluid_state(state.try_into_engine(&()).ok()?)
    }
    fn fluid_in_tag(&self, fluid: &str, tag: &str) -> Option<bool> {
        Some(super::native_world_read::fluid_in_tag(
            self.game.items(),
            fluid,
            tag,
        ))
    }
    fn state_with_property(
        &self,
        state: mod_api::BlockState,
        key: &str,
        value: &str,
    ) -> Option<mod_api::BlockState> {
        super::native_hooks::with_property(
            self.game.pack.assets.registry,
            state.try_into_engine(&()).ok()?,
            key,
            value,
        )?
        .try_into_api()
        .ok()
    }
    fn brightness(&self, layer: mod_api::LightLayer, pos: [i32; 3]) -> i32 {
        super::native_level_refs::world_brightness(self.resident().world, layer, pos)
    }
    fn raw_brightness(&self, pos: [i32; 3], sky_darken: i32) -> i32 {
        // Java's unchecked subtraction: a listener may pass any darkening, including
        // `i32::MIN`, and the result wraps as vanilla's `int` arithmetic does.
        self.brightness(mod_api::LightLayer::Block, pos).max(
            self.brightness(mod_api::LightLayer::Sky, pos)
                .wrapping_sub(sky_darken),
        )
    }
    fn is_loaded(&self, [x, y, z]: [i32; 3]) -> bool {
        self.resident().world.is_loaded_at(x, y, z)
    }
    fn is_raining_at(&self, pos: [i32; 3]) -> bool {
        self.game
            .level_is_raining_at(self.resident().level, pos)
            .unwrap_or(false)
    }
    fn enabled_features(&self) -> mod_api::FeatureFlags {
        mod_api::FeatureFlags::new(self.game.enabled_feature_ids().clone())
    }
    fn signal(&self, [x, y, z]: [i32; 3], direction: Direction) -> Option<i32> {
        self.signals(|s| s.signal(BlockPos::new(x, y, z), direction))
    }
    fn direct_signal(&self, [x, y, z]: [i32; 3], direction: Direction) -> Option<i32> {
        self.signals(|s| s.direct_signal(BlockPos::new(x, y, z), direction))
    }
    fn direct_signal_to(&self, [x, y, z]: [i32; 3]) -> Option<i32> {
        self.signals(|s| s.direct_signal_to(BlockPos::new(x, y, z)))
    }
    fn best_neighbor_signal(&self, [x, y, z]: [i32; 3]) -> Option<i32> {
        self.signals(|s| s.best_neighbor_signal(BlockPos::new(x, y, z)))
    }
    fn has_neighbor_signal(&self, [x, y, z]: [i32; 3]) -> Option<bool> {
        self.signals(|s| s.has_neighbor_signal(BlockPos::new(x, y, z)))
    }
    fn collision_shape(&self, pos: [i32; 3]) -> Option<Vec<[f64; 6]>> {
        let level = self.resident();
        let [x, y, z] = pos;
        if !level.collision_shapes || !level.world.is_loaded_at(x, y, z) {
            return None;
        }
        Some(super::native_state_shapes::collision_boxes(
            self.game.collision,
            level.world,
            level.world.block(x, y, z),
            pos,
        ))
    }
}
