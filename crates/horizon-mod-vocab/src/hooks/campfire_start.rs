//! The `campfire_start` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0186

pub trait Hooks {
    /// An item was placed on a campfire at `pos` and began cooking.
    #[hook(
        signature = "pub(in crate::game) fn native_campfire_start(&mut self, pos: [i32; 3], item: &str)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_campfire_start",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn campfire_start();
}
