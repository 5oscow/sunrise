//! The `block_failed_dispense` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0176

pub trait Hooks {
    /// A dispenser at `pos` failed to dispense `item`. Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_block_failed_dispense( &mut self, dimension: &str, pos: [i32; 3], item: &str, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_block_failed_dispense",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn block_failed_dispense();
}
