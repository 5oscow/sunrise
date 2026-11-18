//! The `entity_tick_post` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0155

pub trait Hooks {
    /// The entities `entities` ticked in the level `dimension`, and `kinds` holds the type of
    /// each, in the same order (NeoForge's `EntityTickEvent.Post`): those `entity_tick_pre`
    /// was told and did not cancel, also one that died or was removed in its tick. An entity
    /// `entity_tick_pre` was told alone is told alone here too, right after its own tick; a
    /// family is told once, after its last tick. Told where `entity_tick_pre` is, on the
    /// server and on a client (`host` says which side). Contract version 2: version 1 was told
    /// for one entity at a time, with `entity` and `kind`, on the server only. Diverges from
    /// upstream: one call for each family of the entities not told alone.
    #[hook(
        side = "both",
        version = 2,
        contract = "listen",
        neoforge = "EntityTickEvent.Post",
        cancellable = false
    )]
    fn entity_tick_post();
}
