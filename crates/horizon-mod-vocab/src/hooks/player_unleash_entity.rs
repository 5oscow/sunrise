//! The `player_unleash_entity` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0319

pub trait Hooks {
    /// A server player removed an entity's leash, after it was cleared.
    #[hook(
        signature = "pub(in crate::game) fn native_player_unleash_entity( &mut self, player: u128, name: &str, entity_id: u32, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_player_unleash_entity",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn player_unleash_entity();
}
