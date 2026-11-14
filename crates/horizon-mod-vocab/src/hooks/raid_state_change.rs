//! The `raid_state_change` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0329

pub trait Hooks {
    /// A server raid at `pos` changed to `state` (`triggered`, `victory`,
    /// `loss` or `stopped`). `raid` identifies its owning level, including a
    /// parked level. A terminal raid already removed from storage cannot be read.
    #[hook(
        signature = "pub(in crate::game) fn native_raid_state_change( &mut self, raid: mod_api::RaidRef, pos: [i32; 3], state: &str, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_raid_state_change",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn raid_state_change();
}
