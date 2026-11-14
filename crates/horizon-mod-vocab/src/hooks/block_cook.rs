//! The `block_cook` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0172

pub trait Hooks {
    /// A block finished cooking `item` into `result` at `pos`.
    #[hook(
        signature = "pub(in crate::game) fn native_block_cook(&mut self, pos: [i32; 3], item: &str, result: &str)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_block_cook",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn block_cook();
}
