//! The `vault_change_state` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0339

pub trait Hooks {
    /// A vault at `pos` changed to `state`. Its block state is updated.
    /// Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_vault_change_state( &mut self, dimension: &str, pos: [i32; 3], state: &str, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_vault_change_state",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn vault_change_state();
}
