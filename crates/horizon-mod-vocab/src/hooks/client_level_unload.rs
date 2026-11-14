//! The `client_level_unload` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0141

pub trait Hooks {
    /// The client dropped its level for `dimension` (`LevelEvent.Unload`
    /// from `Minecraft.setLevel` and `disconnect`): replaced by another
    /// dimension's, or on leaving the server. Once for each
    /// `client_level_load`. Not told when the game itself quits.
    /// Client-side.
    #[hook(
        signature = "pub(in crate::game) fn native_client_level_unload(&mut self, dimension: &str)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_client_level_unload",
        side = "client",
        contract = "listen",
        neoforge = "LevelEvent.Unload",
        cancellable = false
    )]
    fn client_level_unload();
}
