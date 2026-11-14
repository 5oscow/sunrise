//! The `panel_event` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0021

pub trait Hooks {
    /// The player did `event` on mod `mod_id`'s panel (`native_panel_items`):
    /// act on it when it is yours.
    #[hook(
        signature = "pub(in crate::game) fn native_panel_event(&mut self, mod_id: &str, event: &PanelEvent)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_panel_event",
        side = "client",
        contract = "listen",
        neoforge = "none (Horizon-only; NeoForge has no counterpart)"
    )]
    fn panel_event();
}
