//! The `should_apply_effect_tick_this_tick` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0062

pub trait Hooks {
    /// `MobEffect.shouldApplyEffectTickThisTick` for a content mod's effect
    /// `effect` (an `EffectKey` for `ruby:bleeding`), each tick a player or a mob has it, in
    /// `MobEffectInstance.tickServer` before it counts down: `tick_count` is
    /// the ticks left (the entity's tick count when infinite). Set `apply`
    /// and `native_apply_effect_tick` follows. Server-side.
    #[hook(
        signature = "pub(in crate::game) fn native_should_apply_effect_tick_this_tick( &self, effect: EffectKey, tick_count: i32, amplifier: i32, apply: &mut bool, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_should_apply_effect_tick_this_tick",
        side = "server",
        contract = "listen",
        host = "read",
        neoforge = "none (vanilla MobEffect.shouldApplyEffectTickThisTick; NeoForge adds no event or extension for it)"
    )]
    fn should_apply_effect_tick_this_tick();
}
