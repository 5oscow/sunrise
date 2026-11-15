//! The `section_mesh_recipes` hook: declarations the mod API generator reads (see `hooks/mod.rs`).

pub trait Hooks {
    /// The geometry a mod gives blocks or exact states in terrain meshes: push one
    /// `StateMeshRecipe` per block or state into `recipes` (append quads, replace the model's
    /// quads, possibly with none, and declare which faces fully occlude). Asked on the client
    /// world thread whenever the resources load and when a mod calls
    /// `ClientHost::refresh_render_visibility`; the recipes are checked, resolved to the loaded
    /// atlas and frozen into a table by state that the meshing workers only read. A mod's recipes
    /// are taken whole or refused whole with one log line; two mods replacing one state refuse
    /// the later. With no mod listening, the mesher pays nothing.
    #[notes = r#"## Section mesh recipes

`section_mesh_recipes` gives a block, or one exact state, its own terrain geometry without a block model of its
own: a cable bus whose parts are quads, a frame that draws only its edges, a block hidden but for one decal.

```rust,ignore
fn section_mesh_recipes(&self, _: &dyn ClientHost, event: &mut SectionMeshRecipesEvent<'_>) {
    let top = MeshQuad::new(
        [[0.0, 1.001, 0.0], [0.0, 1.001, 1.0], [1.0, 1.001, 1.0], [1.0, 1.001, 0.0]],
        [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
        "mymod:block/rune",
    )
    .culled_by(1);
    event.recipes.push(StateMeshRecipe::append(MeshRecipeTarget::Block("mymod:altar".into()), vec![top]));
}
```

A recipe appends quads to the block's model or, with `replace`, draws them instead (none for nothing). Corners are
in blocks from the block's lowest corner, -1 to 2 on each axis; UVs 0 to 1 within the named block atlas sprite. A
quad with a cull face is dropped when that face is culled, as a model face's `cullface`; one without draws
whatever is beside the block. `occludes` declares the faces through which the block fully occludes its
neighbours; without it an appending recipe keeps the block's own occlusion and a replacing one occludes nothing.
Translucent quads go to the translucent pass and are sorted with the rest of the section.

Every mod's recipes for one state compose: replacement drops the model once, every mod's quads draw in mod
order, declared occlusions intersect. A second mod replacing the same state is refused (`E_MESH_RECIPE_REPLACED`,
naming the mod whose replacement stands). Before any of a mod's recipes is used, all are checked: more than 256
quads for one state (`E_MESH_RECIPE_TOO_MANY`), a corner or UV not finite (`E_MESH_RECIPE_NOT_FINITE`) or out of
range (`E_MESH_RECIPE_OUT_OF_RANGE`), a face above 5 or mask above 63 (`E_MESH_RECIPE_FACE`), a sprite the atlas
lacks (`E_MESH_RECIPE_SPRITE`) or a block or state that does not exist (`E_MESH_RECIPE_TARGET`) refuses that
mod's recipes whole, with one log line; its blocks draw their own models. A section whose recipes would pass
262144 quads meshes without them. The recipes are asked again on every resource load and on
`ClientHost::refresh_render_visibility`; the meshing workers never call the mod."#]
    #[hook(
        side = "client",
        contract = "collect",
        out = "recipes",
        answered_by = "mod",
        neoforge = "none (NeoForge bakes model quads; IDynamicBakedModel is the nearest)"
    )]
    fn section_mesh_recipes(recipes: &mut Vec<Answered<StateMeshRecipe>>);
}
