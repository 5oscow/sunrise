//! The `target_hit` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0334

pub trait Hooks {
    /// A projectile hit the target block at `pos`, which now has `strength`
    /// output power. Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_target_hit( &mut self, dimension: &str, pos: [i32; 3], strength: i32, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_target_hit",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn target_hit();
}
