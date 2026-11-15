# Block geometry in terrain meshes

`section_mesh_recipes` gives a block, or one exact state, its own terrain geometry without a block model of its
own: a cable bus whose parts are quads, a frame that draws only its edges, a block hidden but for one decal. The
mod pushes `StateMeshRecipe` values; the game checks them, resolves their sprites and targets, and freezes them
into a table by state that the meshing workers read. The mod's code never runs on a worker.

```rust,ignore
fn section_mesh_recipes(&self, _: &dyn ClientHost, event: &mut SectionMeshRecipesEvent<'_>) {
    let rune = MeshQuad::new(
        [[0.0, 1.001, 0.0], [0.0, 1.001, 1.0], [1.0, 1.001, 1.0], [1.0, 1.001, 0.0]],
        [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
        "mymod:block/rune",
    )
    .culled_by(1);
    event.recipes.push(StateMeshRecipe::append(MeshRecipeTarget::Block("mymod:altar".into()), vec![rune]));
    // A frame block that draws only its own quads and hides nothing behind it.
    event.recipes.push(StateMeshRecipe::replace(MeshRecipeTarget::Block("mymod:frame".into()), self.edges()));
}
```

## What a recipe does

- `append` draws the quads after the block model's; `replace` draws them instead, and with no quads the block
  draws nothing of its model. Recipes apply to blocks that have a block model.
- Corners are in blocks from the block's lowest corner, -1 to 2 on each axis; UVs are 0 to 1 within the named
  block atlas sprite; `color` tints the texels.
- A quad `culled_by(face)` is dropped when that face is culled, like a model face's `cullface`. Without a cull face
  it draws whatever is beside the block. Translucent quads go to the translucent pass with the rest of the
  section, and a `section_visibility` face mask removes culled-by quads with the faces they belong to.
- `occluding(faces)` declares the faces through which the block fully occludes its neighbours. Without it an
  appending recipe keeps the block's own occlusion and a replacing one occludes nothing.

Recipes for one state compose across mods: a replacement drops the model once, every mod's quads draw in mod
order, and declared occlusions intersect. A second mod replacing the same state is refused.

## Refusals

All of a mod's recipes are checked before any is used, and one fault refuses them all, logged once with the mod
named. Its blocks then draw their own models:

| Code | Cause |
|---|---|
| `E_MESH_RECIPE_TOO_MANY` | more than 256 quads for one state, counting every mod's |
| `E_MESH_RECIPE_NOT_FINITE` | a corner or UV that is NaN or infinite |
| `E_MESH_RECIPE_OUT_OF_RANGE` | a corner outside -1 to 2, a UV outside 0 to 1 |
| `E_MESH_RECIPE_FACE` | a cull face above 5, an occlusion mask above 63 |
| `E_MESH_RECIPE_SPRITE` | a sprite the block atlas lacks |
| `E_MESH_RECIPE_TARGET` | a block no registry holds, a state id past the state table |
| `E_MESH_RECIPE_REPLACED` | the state already has another mod's replacement |
| `E_MESH_RECIPE_PANIC` | the listener panicked: no recipes until the next ask |

A section whose recipes would pass 262144 quads meshes without them (`E_MESH_RECIPE_SECTION`, logged once).

## When it is asked

On the client world thread, whenever the resources load and when a mod calls
`ClientHost::refresh_render_visibility`. Every loaded section then meshes again with the new table, and meshes
made with the old one are not uploaded. With no mod listening, nothing is asked and the mesher runs as before.
