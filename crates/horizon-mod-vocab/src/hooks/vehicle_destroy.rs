//! The `vehicle_destroy` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0271

pub trait Hooks {
    /// A boat or minecart is about to be destroyed at `pos`. TNT minecarts
    /// that prime instead of breaking are not heard. Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_vehicle_destroy(&mut self, kind: &str, pos: [f64; 3])",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_vehicle_destroy",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event; Bukkit's VehicleDestroyEvent)"
    )]
    fn vehicle_destroy();
}
