//! The `projectile_launch` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0327

pub trait Hooks {
    /// A server projectile was added to the level. `player` is set only for
    /// a player launch; other sources have an empty `name`.
    #[hook(
        signature = "pub(in crate::game) fn native_projectile_launch( &mut self, player: Option<u128>, name: &str, entity: u32, kind: &str, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_projectile_launch",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn projectile_launch();
}
