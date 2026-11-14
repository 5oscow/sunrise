//! The `entity_teleport` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0208

pub trait Hooks {
    /// A server entity was moved by a same-level teleport command, after its
    /// position changed. `entity` is its network id; `from` and `to` are
    /// coordinates in `dimension`. This hook reports command teleports that
    /// use Horizon's same-level target path; it does not change vanilla's
    /// movement.
    #[hook(
        signature = "pub(in crate::game) fn native_entity_teleport( &mut self, dimension: &str, entity: u32, from: [f64; 3], to: [f64; 3], )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_entity_teleport",
        side = "server",
        contract = "listen",
        neoforge = "none (NeoForge has no such event)"
    )]
    fn entity_teleport();
}
