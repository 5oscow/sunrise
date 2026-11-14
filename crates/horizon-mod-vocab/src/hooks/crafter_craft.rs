//! The `crafter_craft` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0189

pub trait Hooks {
    /// A crafter at `pos` completed recipe `recipe`.
    #[hook(
        signature = "pub(in crate::game) fn native_crafter_craft(&mut self, pos: [i32; 3], recipe: &str)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_crafter_craft",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn crafter_craft();
}
