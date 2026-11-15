# Baked models and their keys

Every successful resource load bakes each block state's models and the standalone models once,
into one store for that load. That store belongs to the game and never changes while it is
published: the mesher, block entity drawing and a mod all read the same baked results. A mod
holds keys into it and copies of its quads, never the models themselves. This is the first part
of #861 (the [design](https://github.com/arismoko/horizon/blob/main/docs/design/mod-api/baked-models.md));
block-entity model data and editing a baking result build on it.

## Keys

A `BakedModelKey` names one baked model in one published store: its resource generation and its
slot. Ask for them in `model_baking_completed`, when the new store is published, and keep them
until the next one:

```rust,ignore
fn model_baking_completed(&self, host: &mut dyn ClientHost, _: &mut ModelBakingCompletedEvent) {
    let assets = host.render_assets();
    // Every model the state can draw: each weighted alternative and each selected multipart part.
    let keys = assets.block_state_models(self.state)?;
    // A standalone model's handle names a baked model too.
    let part = BakedModelKey::from(assets.model(&Identifier::parse("mymod:block/part")?)?);
    let view = assets.baked_model(keys[0])?;
    for quad in &view.quads {
        // quad.positions, quad.uvs, quad.sprite, quad.facing, quad.cull, quad.tint_index ...
    }
}
```

| Call | Answers |
| --- | --- |
| `block_state_models(state)` | Every model the state can select, in bake order without repeats, whichever one a position's seed picks. Empty for a state no blockstate file binds. `UnknownModel` for an id past the state table. |
| `baked_model(key)` | A copy of the model: `ambient_occlusion`, the `particle` sprite and its `quads`. `StaleModel` once a reload superseded the key; `UnknownModel` for a slot the store lacks. |

`BakedModelView` and `BakedQuad` are copies: changing them changes nothing the game draws. Each
`baked_model` call copies every quad, so read what a mod needs once per load, not each frame.

## Generations

A successful reload (F3+T, a pack turned on or off, a server's pack) bakes a new store from the
resources as they are now, never from the previous store, and publishes it before
`model_baking_completed`. Keys from the previous load then answer `StaleModel`; ask again. A
failed reload publishes nothing, and the keys of the last successful load stay valid. A dedicated
server bakes no models: its store is empty.

Native and wasm mods get the same keys, copies and refusals.
