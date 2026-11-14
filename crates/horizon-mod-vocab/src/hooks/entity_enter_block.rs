//! The `entity_enter_block` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0202

pub trait Hooks {
    /// A bee entered the hive at `pos` and was stored there.
    #[hook(
        signature = "pub(in crate::game) fn native_entity_enter_block( &mut self, entity: u32, kind: &str, pos: [i32; 3], block: &str, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_entity_enter_block",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn entity_enter_block();
}
