//! Server-side serialization; `data()` still reads storage without re-entering the hook.
use super::{BlockEntity, Game, GameLevel, Pos};
use crate::game::native_level_signals::ReadLevel;
use mod_api::{BlockEntityRef, BlockEntityTypeKey, HookId, LevelRef};
use sunrise_client::{containers::SHAPES_KEY, save::block_entity_encode, world::World};
use sunrise_core::nbt::{Compound, Tag};
use sunrise_mod_runtime::attachments::ATTACHMENTS_KEY;

fn save_additional(
    world: &World,
    level: LevelRef,
    pos: Pos,
    kind: &str,
    tag: &mut Compound,
    fire: impl FnOnce(BlockEntityTypeKey, BlockEntityRef, &mut Compound),
) {
    if kind.is_empty()
        || !mod_api::__private::block_entity_type_listens(HookId::BlockEntitySaveAdditional, kind)
    {
        return;
    }
    let Some(id) = mod_api::__private::block_entity_type_key(kind) else {
        return;
    };
    // An unloading column has already left the live chunk map, but its entity
    // slots are retained until after the save and unload notices.
    let Some(generation) = world.block_entities.generation(&pos) else {
        return;
    };
    let entity =
        mod_api::__private::game::block_entity_ref(level, [pos.0, pos.1, pos.2], generation);
    let reserved = ["id", "x", "y", "z", "Items", SHAPES_KEY, ATTACHMENTS_KEY];
    let stored = reserved.map(|key| tag.get(&key.into()).cloned());
    fire(id, entity, tag);
    for (key, value) in reserved.into_iter().zip(stored) {
        if let Some(value) = value {
            tag.insert(key.into(), value);
        } else {
            tag.remove(&key.into());
        }
    }
}

impl Game<'_> {
    pub(in crate::game) fn capture_block_entity_payload(
        &self,
        pos: Pos,
        kind: &str,
        tag: &mut Compound,
    ) {
        if self.is_logical_server()
            && mod_api::__private::has_part_hook(HookId::BlockEntitySaveAdditional)
            && let Some(level) = self.level(self.dim.id())
        {
            let host = ReadLevel { game: self, level };
            save_additional(&self.world, level, pos, kind, tag, |id, entity, tag| {
                mod_api::dispatch_block_entity_save_additional(&host, id, entity, tag);
            });
        }
    }

    /// Save the live entity for a command, a template, an item or a loot copy.
    pub(in crate::game) fn encode_live_block_entity(
        &self,
        pos: Pos,
        entity: &BlockEntity,
        block: &str,
        items: &sunrise_client::items::Items,
    ) -> Tag {
        block_entity_encode::encode_with(pos, entity, block, items, |kind, tag| {
            self.capture_block_entity_payload(pos, kind, tag);
        })
    }
}

impl GameLevel<'_> {
    /// `BlockEntity.saveAdditional` for a live block entity used in a loot roll.
    pub(in crate::game) fn encode_live_block_entity(
        &self,
        pos: Pos,
        entity: &BlockEntity,
        block: &str,
    ) -> Tag {
        block_entity_encode::encode_with(pos, entity, block, self.items, |kind, tag| {
            if self.native_authoritative
                && let Some(level) = self.api_level
            {
                save_additional(self.world, level, pos, kind, tag, |id, entity, tag| {
                    self.native_block_entity_save_additional(id, entity, tag);
                });
            }
        })
    }

    pub(in crate::game) fn native_block_entity_save_additional(
        &self,
        id: mod_api::BlockEntityTypeKey,
        entity: BlockEntityRef,
        tag: &mut Compound,
    ) {
        self.fire_block_entity_save_additional(id, entity, tag);
    }
}
