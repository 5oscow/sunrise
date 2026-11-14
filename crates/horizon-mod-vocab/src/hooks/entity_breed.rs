//! The `entity_breed` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0199

pub trait Hooks {
    /// Two mobs completed a breeding action. `child` is absent when vanilla
    /// lays an egg instead of creating a baby.
    #[hook(
        signature = "pub(in crate::game) fn native_entity_breed( &mut self, parent: u32, partner: u32, child: Option<u32>, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_entity_breed",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn entity_breed();
}
