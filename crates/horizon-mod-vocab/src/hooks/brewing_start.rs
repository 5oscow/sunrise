//! The `brewing_start` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0184

pub trait Hooks {
    /// A brewing stand at `pos` began brewing `ingredient`, after fuel and the timer were set. Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_brewing_start(&mut self, pos: [i32; 3], ingredient: &str)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_brewing_start",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn brewing_start();
}
