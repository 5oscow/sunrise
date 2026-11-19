//! The `entity_tick_pre` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0154

pub trait Hooks {
    /// The entities `entities` are about to tick in the level `dimension`, and `kinds` holds
    /// the type of each (`minecraft:zombie`, `minecraft:item`), in the same order (NeoForge's
    /// `EntityTickEvent.Pre`, posted right before each entity's tick in `tickNonPassenger`).
    /// Told on the server for the mobs, item entities, minecarts, boats, general entity rows and
    /// projectiles that tick in an entity-ticking chunk (not a player's own boat or cart, which
    /// ticks with the player), and on a client for the mobs, other players, projectiles and
    /// tridents it ticks; `host` says which side. Minecarts, boats and general rows tick one by
    /// one, so each is told alone, right before its own tick: `entities` holds its one ID. On the
    /// server, a mob or item entity of a type a mod named with `Registrar::entity_tick_each`, or
    /// any of them once a mod called `Registrar::entity_tick_each_entity`, is told alone the same
    /// way. The other mobs and item entities, the projectiles and a client's families are told
    /// once for the family, before its first tick, also for a named type. Push an entity of
    /// `entities` onto `canceled` and it does not tick this tick and its `entity_tick_post` is
    /// not told; IDs not in `entities` are ignored, and an ID pushed twice counts once. Players,
    /// displays, orbs, primed TNT, falling blocks and armor stands are not told. A mod that hooks
    /// it also hooks `entity_tick_listened`. This runs for every ticking entity each tick, so
    /// keep it cheap, and name only the types that need their own moment. Contract version 2:
    /// version 1 was told for one entity at a time, with `entity`, `kind` and a `canceled` flag,
    /// on the server only. Diverges from upstream: one call for each family of the entities not
    /// told alone, and a list of canceled IDs.
    #[hook(
        side = "both",
        version = 2,
        contract = "collect",
        out = "canceled",
        neoforge = "EntityTickEvent.Pre",
        cancellable = true
    )]
    fn entity_tick_pre();
}
