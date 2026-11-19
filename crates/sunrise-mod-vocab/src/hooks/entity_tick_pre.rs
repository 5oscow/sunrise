//! The `entity_tick_pre` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0154

pub trait Hooks {
    /// The entities `entities` are about to tick in the level `dimension`, and `kinds` holds
    /// the type of each (`minecraft:zombie`, `minecraft:item`), in the same order (NeoForge's
    /// `EntityTickEvent.Pre`, posted right before each entity's tick in `tickNonPassenger`).
    /// Told on the server for the mobs, players, item entities, minecarts, boats, general entity
    /// rows and projectiles that tick in an entity-ticking chunk, and the entities the server
    /// keeps without simulating them (displays, markers, interactions), whose tick is only told
    /// (not a player's own boat or cart, which ticks with the player); and on a client for the
    /// mobs, other players, projectiles, tridents and displays it ticks; `host` says which side.
    /// Minecarts, boats and general rows tick one by one, so each is told alone, right before its
    /// own tick: `entities` holds its one ID. An entity of a type a mod named with
    /// `Registrar::entity_tick_each`, or any entity once a mod called
    /// `Registrar::entity_tick_each_entity`, is told alone the same way, on either side; a
    /// passenger is told around its own tick, at its own place in the tick order. The others are
    /// told once for each family, before its first tick. A player's tick is `ServerPlayer.tick`:
    /// on the server each player's Pre comes before the first of its parts, and one canceled
    /// skips them for that player (its camera, its menus' checks, digging, advancements and
    /// attributes), not its body (`player_tick_pre`). Push an entity of `entities` onto
    /// `canceled` and it does not tick this tick and its `entity_tick_post` is not told; IDs not in
    /// `entities` are ignored, and an ID pushed twice counts once. Not told yet: on the server
    /// orbs, primed TNT, falling blocks, armor stands, paintings, item frames, leash knots, eyes of
    /// ender, cushions, tridents, fireworks, lightning, fishing hooks, ominous item spawners, end
    /// crystals, the dragon and its breath, evoker fangs, thrown potions and potion clouds; on a
    /// client, every other entity. A mod that hooks it also hooks `entity_tick_listened`. This runs
    /// for every ticking entity each tick, so keep it cheap, and name only the types that need
    /// their own moment.
    /// Contract version 2:
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
