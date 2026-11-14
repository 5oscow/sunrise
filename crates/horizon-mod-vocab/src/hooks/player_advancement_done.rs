//! The `player_advancement_done` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0275

pub trait Hooks {
    /// A server player completed an advancement, after progress was recorded.
    #[hook(
        signature = "pub(in crate::game) fn native_player_advancement_done( &mut self, player: u128, name: &str, advancement: &str, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_player_advancement_done",
        side = "server",
        contract = "listen",
        neoforge = "AdvancementEvent.AdvancementEarnEvent",
        cancellable = false
    )]
    fn player_advancement_done();
}
