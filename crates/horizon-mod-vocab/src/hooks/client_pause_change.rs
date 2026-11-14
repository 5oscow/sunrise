//! The `client_pause_change` hooks and helpers: declarations the mod API generator reads (see `hooks/mod.rs`).

pub trait Hooks {
    /// NeoForge's `ClientPauseChangeEvent.Pre`: cancel a proposed pause change.
    #[hook(
        signature = "pub(in crate::game) fn native_client_pause_change_pre( &mut self, paused: bool, cancel: &mut bool, )",
        side = "client",
        contract = "cancel",
        out = "cancel",
        neoforge = "ClientPauseChangeEvent.Pre",
        cancellable = true
    )]
    fn client_pause_change_pre();
    /// NeoForge's `ClientPauseChangeEvent.Post`: the pause state changed.
    #[hook(
        signature = "pub(in crate::game) fn native_client_pause_change_post(&mut self, paused: bool)",
        side = "client",
        contract = "listen",
        neoforge = "ClientPauseChangeEvent.Post",
        cancellable = false
    )]
    fn client_pause_change_post();
}
