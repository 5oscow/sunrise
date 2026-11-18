//! Serialization of a mod block entity's owned payload.

pub trait Hooks {
    /// Write live mod state into `tag`, a copy of the instance's stored payload
    /// (`BlockEntity.saveAdditional`). Edits affect only the output; the entity,
    /// its inventory, identity, position, and engine-managed attachments and
    /// shapes are not changed. Parts of the entity's type run in dispatch order.
    ///
    /// Asked on the logical server thread for chunk saves (including unload),
    /// command reads, and copies into items, loot or templates. The read-only
    /// level host cannot write or trigger another save. Use `tag` for stored
    /// fields even during unload, when the level no longer exposes the chunk.
    /// This is not the one-time attachment notice `block_entity_on_load`, nor
    /// the client sync payload `block_entity_update_tag`.
    #[hook(
        side = "server",
        contract = "listen",
        scope = "block_entity",
        key = "id",
        target = "sunrise_client::game::native_hooks::block_entity_save_additional::GameLevel<'_>::native_block_entity_save_additional",
        neoforge = "none (vanilla BlockEntity.saveAdditional; NeoForge adds no event for this moment)"
    )]
    fn block_entity_save_additional();
}
