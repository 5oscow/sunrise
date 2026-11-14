//! The `player_stop_using_item` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0313

pub trait Hooks {
    /// A server player released the use-item key, or a mod's `LivingRef::release_using_item` did,
    /// before the item finished being used. The player's state is swapped in.
    #[hook(
        signature = "pub(in crate::game) fn native_player_stop_using_item(&mut self, player: u128, name: &str)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_player_stop_using_item",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event; Bukkit's PlayerStopUsingItem)"
    )]
    fn player_stop_using_item();
}
