//! Server-side serialization; `data()` still reads storage without re-entering the hook.
use super::{BlockEntity, Game, GameLevel, Pos};
use crate::game::native_level_signals::ReadLevel;
use mod_api::{BlockEntityRef, BlockEntityTypeKey, HookId, LevelRef};
use sunrise_client::save::block_entity_encode;
use sunrise_core::nbt::{Compound, Limits, Tag, validate_saved_compound};
use sunrise_mod_runtime::attachments::ATTACHMENTS_KEY;

fn save_additional(
    level: LevelRef,
    pos: Pos,
    generation: Option<u64>,
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
    let Some(generation) = generation else { return };
    let entity =
        mod_api::__private::game::block_entity_ref(level, [pos.0, pos.1, pos.2], generation);
    // The raw attachment field stays available for the disk overlay (including
    // opaque or malformed rows), but is never part of the Part's payload.
    let key = ATTACHMENTS_KEY.into();
    let mut output: Compound = tag
        .iter()
        .filter(|(name, _)| **name != key)
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    fire(id, entity, &mut output);
    output.remove(&key);
    let mut output = crate::game::native_block_entity_refs::mod_payload(output);
    // The chunk root and its block_entities list enclose the saved compound.
    if let Err(error) = validate_saved_compound(&output, 2, Limits::saved()) {
        log::warn!(
            "block_entity_save_additional: refused invalid payload for {kind} at {pos:?}: {error}"
        );
        return;
    }
    if let Some(attachments) = tag.remove(&key) {
        output.insert(key, attachments);
    }
    *tag = output;
}

impl Game<'_> {
    pub(in crate::game) fn capture_block_entity_payload(
        &self,
        pos: Pos,
        kind: &str,
        tag: &mut Compound,
    ) {
        // A detached unload column still has its live slots until after capture.
        self.capture_block_entity_payload_at(
            pos,
            self.world.block_entities.generation(&pos),
            kind,
            tag,
        );
    }

    fn capture_block_entity_payload_at(
        &self,
        pos: Pos,
        generation: Option<u64>,
        kind: &str,
        tag: &mut Compound,
    ) {
        if self.is_logical_server()
            && mod_api::__private::has_part_hook(HookId::BlockEntitySaveAdditional)
            && let Some(level) = self.level(self.dim.id())
        {
            let host = ReadLevel { game: self, level };
            save_additional(level, pos, generation, kind, tag, |id, entity, tag| {
                mod_api::dispatch_block_entity_save_additional(&host, id, entity, tag);
            });
        }
    }

    /// Save the retained block entity only once the removed block's loot will roll.
    /// The handle still names the captured generation, but its level slot may be gone.
    pub(in crate::game) fn encode_removed_block_entity(
        &self,
        pos: Pos,
        entity: &BlockEntity,
        generation: Option<u64>,
        block: &str,
        items: &sunrise_client::items::Items,
    ) -> Tag {
        block_entity_encode::encode_with(pos, entity, block, items, |kind, tag| {
            self.capture_block_entity_payload_at(pos, generation, kind, tag);
        })
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
                save_additional(
                    level,
                    pos,
                    self.world.block_entities.generation(&pos),
                    kind,
                    tag,
                    |id, entity, tag| {
                        self.native_block_entity_save_additional(id, entity, tag);
                    },
                );
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
