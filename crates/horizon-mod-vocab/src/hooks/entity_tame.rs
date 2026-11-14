//! The `entity_tame` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0207

pub trait Hooks {
    /// A mob became tamed after its owner and persistent state were set.
    #[hook(
        signature = "pub(in crate::game) fn native_entity_tame( &mut self, entity: u32, kind: &str, owner: Option<u128>, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_entity_tame",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn entity_tame();
}
