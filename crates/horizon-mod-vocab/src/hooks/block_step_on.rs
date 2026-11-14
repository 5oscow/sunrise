//! The `block_step_on` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0085

pub trait Hooks {
    /// `Block.stepOn` for a mod block, on its authoritative server level.
    /// No self: the simulation owns the entity during this callback.
    #[hook(
        signature = "pub(in crate::game) fn native_block_step_on( dimension: &str, entity: i32, kind: &str, state: BlockStateId, position: [i32; 3], )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_block_step_on",
        side = "server",
        contract = "listen",
        neoforge = "none (vanilla Block.stepOn; NeoForge adds no event or extension for it)"
    )]
    fn block_step_on();
}
