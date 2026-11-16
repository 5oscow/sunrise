# Native mods: building and shipping

A native mod is Rust source in a `.smod`. The launcher compiles it on the player's machine, with its own pinned Rust
toolchain, against the prebuilt game of the exact Sunrise build the mod pins in `[native]`. The game itself is never
rebuilt for a mod: the mods are compiled and linked against that prebuilt game, and the instance then plays the
result.

Native code runs with the player's rights, outside any sandbox. The first install shows a trust notice, and the
launcher builds native mods only on an install that allows native code (`--trust-native-code`, revocable in
**Settings > Storage**). A mod that must run in a sandbox is a [WebAssembly mod](wasm-mods.md).

The pages in this section follow a mod from its source to a player's game and a server:

<!-- section-pages -->

## Living knockback entry

`living_knockback` fires once per full-entry call, before resistance and motion changes.
The read-only `entry` is `KnockbackEntry::Short` when the short overload delegated the push,
otherwise `KnockbackEntry::Full`. To model doubling strength at both entries, double it
once, then double it again only on `Short`: an input of 0.4 becomes 1.6 for `Short`
and 0.8 for `Full`. There is no second hook dispatch. Two non-commuting listeners run
in dispatch order, each completing its rewrites before the next; this does not simulate
two separate passes over the listener list. Cancellation still leaves motion unchanged.

Version 2 also supplies `kill_credit` and `kill_credit_main`: the pushed living entity's
kill credit and that entity's current main-hand stack, read after accepted-hit memories
are committed and before the push. A remembered player takes precedence over the last
hurting mob, including a tame wolf's owner. An unresolved remembered player yields 0 and
no stack, not a fallback to the mob. After the remembered player expires, the recent mob
may receive credit. This context is distinct from `attacker` and its captured source weapon. Both fields are additions: NeoForge's
`LivingKnockBackEvent` doesn't carry them.

## Living death contract version 2

[`living_death` version 2](https://github.com/sunrise-org/sunrise/blob/main/docs/native-mods.md#living-death-version-2) supplies
the accepted killing source and its captured weapon stack. Cancellation keeps the
health a listener leaves, suppresses loot, experience and death messages, and does
not heal the body. An unhealed cancelled body is removed at tick 20 without loot
or another death dispatch.

### Custom statistics

Declare custom statistic identities in `Hooks::register`, using the same
`Registrar` route for native and wasm mods:

```rust,ignore
fn register(&self, registrar: &mut dyn Registrar, _: &RegistrationContext) {
    registrar.register_custom_stat("steps").unwrap();
}
```

For a mod named `rewrite`, this declares `rewrite:steps` in
`minecraft:custom_stat`. An explicit `rewrite:steps` or another manifest-owned
namespace is also accepted; `minecraft`, foreign namespaces, malformed IDs and
duplicates are refused. Ignoring the returned `Result<(), String>` does not
bypass installation failure. A mod's explicit registration refusal rolls back
its declarations, and the installed inventory freezes before worlds open.

These declarations populate the integrated command/statistic inventory and
append after vanilla's IDs in LAN and client built-in registry reads, preserving
vanilla IDs. Servers declaring custom statistics require Sunrise clients with
the same ordered statistic inventory; configuration rejects mismatches before
play. Register the target before rewriting an award to it; an arbitrary
identifier alone does not register a statistic.

`/scoreboard objectives add steps minecraft.custom:rewrite.steps` creates an
objective for this registered counter.

## Moving colliders

A native mod gives an entity boxes that every other entity collides with:

```rust
entity.set_collider(host, Some(Collider {
    boxes: vec![ColliderBox { min: [-1.5, -1.0, -10.0], max: [1.5, 0.0, 10.0] }],
    rotation: [0.0, 0.0, 0.0, 1.0],
}))?;
entity.set_collider_transform(host, turned)?; // the rotation alone, as the entity turns
```

The boxes are in the entity's own frame (its position is the origin), turned by `rotation`, a unit
quaternion with any yaw, pitch and roll. Other entities collide with them as solid ground and walls:
the entity's box is swept against each turned box (a separating-axis test with the time of impact), it
steps up onto a top within its step height, and standing on a tilted top follows the surface. The
collider is solid to every mover except spectators, entities with no physics and the entities in its
own vehicle stack (the root vehicle and every passenger, at any depth, the collider's entity among them).
**Contact effects are not applied yet:** a box is solid ground and walls and nothing more, so standing or walking on one has no block's step sound, sprint particles, friction or bounce ([#1909](https://github.com/sunrise-org/sunrise/issues/1909)).

The game checks a collider when it is set and refuses by name (`ColliderError`), keeping the old one:
every value finite, the rotation a unit quaternion within 1e-3, at most `Collider::MAX_BOXES` (32,768)
boxes, and every box within `Collider::MAX_REACH` (512) blocks of the entity on each axis. Boxes are kept
as `f32`; read back, they are the rounded values both sides collide with. A box whose minimum is above
its maximum is read with the corners swapped, and a box with no volume (zero thickness on an axis)
collides with nothing.

A collider is saved with the entity. The server sends it only to Sunrise clients that track the entity:
the whole collider when they start to (in messages of at most 64 KiB), then only what changed, by box
index, and the rotation alone for `set_collider_transform`. **A vanilla client does not see a collider**:
it sees the entity and walks through it. A mod that needs players to stand on one requires the Sunrise
client at join. Carrying whoever stands on a moving collider is a later step; this one only makes it solid.
