//! The `player_name_entity` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0299

pub trait Hooks {
    /// A server player named a mob with a name tag, after its name and persistence changed and the tag
    /// was consumed. Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_player_name_entity( &mut self, player: u128, name: &str, entity_id: u32, entity_name: &str, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_player_name_entity",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn player_name_entity();
}
