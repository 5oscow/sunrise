//! The `player_egg_throw` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0286

pub trait Hooks {
    /// A server player threw an egg, after its projectile was launched and the use was applied.
    /// Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_player_egg_throw(&mut self, player: u128, name: &str)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_player_egg_throw",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn player_egg_throw();
}
