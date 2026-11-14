//! The `sculk_bloom` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0330

pub trait Hooks {
    /// A sculk catalyst at `pos` has begun blooming after taking experience.
    /// The catalyst block state has changed. Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_sculk_bloom(&mut self, dimension: &str, pos: [i32; 3])",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_sculk_bloom",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn sculk_bloom();
}
