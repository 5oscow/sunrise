//! The `player_riptide` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0307

pub trait Hooks {
    /// A server player launched with a riptide trident, after the velocity,
    /// spin attack and grounded lift have been applied. The player's state is
    /// swapped in. No reply is needed.
    #[hook(
        signature = "pub(in crate::game) fn native_player_riptide(&mut self, player: u128, name: &str)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_player_riptide",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event; Bukkit's PlayerRiptideEvent)"
    )]
    fn player_riptide();
}
