//! The `player_swap_with_equipment_slot` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0314

pub trait Hooks {
    /// A server player swapped their main and off hands, after both inventory slots changed.
    /// Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_player_swap_with_equipment_slot( &mut self, player: u128, name: &str, slot: &str, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_player_swap_with_equipment_slot",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn player_swap_with_equipment_slot();
}
