//! The `spawn_change` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0267

pub trait Hooks {
    /// The world's spawn point changed to `pos` and `angle`.
    #[hook(
        signature = "pub(in crate::game) fn native_spawn_change(&mut self, pos: [i32; 3], angle: [f32; 2])",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_spawn_change",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn spawn_change();
}
