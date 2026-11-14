//! The `brewing_complete` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0182

pub trait Hooks {
    /// A brewing stand finished brewing at `pos`, after its bottle contents
    /// were changed. Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_brewing_complete(&mut self, pos: [i32; 3])",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_brewing_complete",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn brewing_complete();
}
