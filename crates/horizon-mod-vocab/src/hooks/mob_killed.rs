//! The `mob_killed` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0049

pub trait Hooks {
    /// `player`'s melee blow killed the mob `entity` of `kind` (other deaths
    /// aren't told yet). It only hears, after the blow landed. Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_mob_killed(&mut self, player: u128, entity: u32, kind: &str)",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_mob_killed",
        side = "server",
        contract = "listen",
        neoforge = "LivingDeathEvent"
    )]
    fn mob_killed();
}
