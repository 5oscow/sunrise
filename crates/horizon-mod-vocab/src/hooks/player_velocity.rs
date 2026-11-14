//! The `player_velocity` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0320

pub trait Hooks {
    /// A server applied `velocity` to a player, when syncing changed motion to that player's client.
    #[hook(
        signature = "pub(in crate::game) fn native_player_velocity( &mut self, player: u128, name: &str, velocity: [f64; 3], )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_player_velocity",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn player_velocity();
}
