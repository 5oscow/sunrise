//! The `entity_unleash` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0210

pub trait Hooks {
    /// A leashed mob's leash was removed, before any lead drop is handled
    /// (Bukkit's `EntityUnleashEvent`).
    #[hook(
        signature = "pub(in crate::game) fn native_entity_unleash(&mut self, entity: u32, kind: &str)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_entity_unleash",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event; Bukkit's EntityUnleashEvent)"
    )]
    fn entity_unleash();
}
