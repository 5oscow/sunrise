//! The `furnace_smelt` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0222

pub trait Hooks {
    /// A furnace finished smelting `item` into `result` at `pos`. Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_furnace_smelt( &mut self, pos: [i32; 3], item: &str, result: &str, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_furnace_smelt",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn furnace_smelt();
}
