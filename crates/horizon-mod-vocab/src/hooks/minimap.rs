//! The `minimap` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0019

pub trait Hooks {
    /// The minimap a native mod shows: set `minimap` when it is none yet.
    #[hook(
        signature = "pub(in crate::game) fn native_minimap(&self, minimap: &mut Option<mod_api::Minimap>)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_minimap",
        side = "client",
        contract = "replace",
        out = "minimap",
        neoforge = "none (Horizon-only; NeoForge has no counterpart)"
    )]
    fn minimap();
}
