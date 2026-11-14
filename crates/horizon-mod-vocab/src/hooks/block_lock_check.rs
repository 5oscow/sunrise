//! The `block_lock_check` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0179

pub trait Hooks {
    /// A guest's block inventory lock check at `pos`. `locked` is the
    /// vanilla result and may be changed. The guest's state is not swapped in.
    #[hook(
        signature = "pub(in crate::game) fn native_block_lock_check( &mut self, player: u128, name: &str, pos: [i32; 3], locked: &mut bool, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_block_lock_check",
        side = "server",
        contract = "cancel",
        out = "locked",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn block_lock_check();
}
