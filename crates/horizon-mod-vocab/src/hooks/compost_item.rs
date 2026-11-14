//! The `compost_item` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0188

pub trait Hooks {
    /// A player added a compostable item to the composter at `pos`.
    #[hook(
        signature = "pub(in crate::game) fn native_compost_item(&mut self, pos: [i32; 3], item: &str)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_compost_item",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn compost_item();
}
