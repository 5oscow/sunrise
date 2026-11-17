//! The `on_effect_added` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0063

pub trait Hooks {
    /// `MobEffect.applyEffectTick`: the content mod's resolved `EffectKey` `effect` acts on
    /// `entity` in `dimension` (`minecraft:overworld`). Clear `keep` and the
    /// effect ends now, before it counts down. `entity` is a mob's
    /// `Entity.getId`, None for a player (`host.players().acting_player()` says who).
    /// It runs inside the entity's tick: a mob's world borrows the game
    /// there, so the game is read-only, and it does not hold the ticking
    /// mobs, nor the player's effects. `holder` identifies the holder; Host
    /// reads of a mob holder are unavailable while it is ticking. Queued
    /// changes apply after the hook; clearing `keep` ends the effect.
    /// `active_effects` is the holder's live active-effect snapshot at this point in the
    /// pass; `has_effect(key)` reads it, reflecting removals earlier in the pass.
    /// The current level's game rules remain readable through `host.level(dimension)`.
    /// Version 2: only the content mod `mod_id` that declared `effect` is dispatched.
    /// All snapshots and queued changes cross the shared Wire boundary on both targets.
    /// Server-side.
    #[hook(
        side = "server",
        owner = "mod_id",
        version = 2,
        contract = "listen",
        host = "read",
        neoforge = "none (vanilla MobEffect.applyEffectTick; NeoForge adds no event or extension for it)"
    )]
    fn apply_effect_tick(
        mod_id: &str,
        effect: EffectKey,
        dimension: &str,
        entity: Option<i32>,
        amplifier: i32,
        holder: LivingRef,
        health: f32,
        max_health: f32,
        food: Option<PlayerFood>,
        active_effects: &[EffectKey],
        changes: &mut Vec<EffectTickChange>,
        keep: &mut bool,
    );
    /// `MobEffect.onEffectAdded`: `LivingEntity.addEffect` gave `entity`
    /// (as in `native_apply_effect_tick`) the content mod's effect `effect`,
    /// which it didn't have. Server-side.
    #[hook(
        side = "server",
        contract = "listen",
        neoforge = "none (vanilla MobEffect.onEffectAdded; NeoForge adds no event or extension for it)"
    )]
    fn on_effect_added();
}
