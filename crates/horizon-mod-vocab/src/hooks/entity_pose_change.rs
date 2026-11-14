//! The `entity_pose_change` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0205

pub trait Hooks {
    /// A guest player's synchronized pose changed from `from` to `to`.
    #[hook(
        signature = "pub(in crate::game) fn native_entity_pose_change( &mut self, player: u128, name: &str, from: &str, to: &str, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_entity_pose_change",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn entity_pose_change();
}
