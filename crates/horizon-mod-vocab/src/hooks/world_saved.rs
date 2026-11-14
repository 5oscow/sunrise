//! The `world_saved` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0344

pub trait Hooks {
    /// The server completed a world save cycle. `wait` says chunk writes were
    /// flushed before returning.
    #[hook(
        signature = "pub(in crate::game) fn native_world_saved(&mut self, wait: bool)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_world_saved",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn world_saved();
}
