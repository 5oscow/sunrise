//! The `block_shear_entity` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0181

pub trait Hooks {
    /// A dispenser sheared `entity` of kind `kind` at the block in front of
    /// its own block at `pos`, after the shearing effects. Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_block_shear_entity( &mut self, dimension: &str, pos: [i32; 3], entity: u32, kind: &str, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_block_shear_entity",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn block_shear_entity();
}
