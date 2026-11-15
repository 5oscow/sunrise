//! Resolved state mesh recipes (`section_mesh_recipes`, docs/design/mod-api/section-meshing.md):
//! the dense table by state the world thread freezes once per resource load from the mods'
//! checked recipes, which the meshing workers only read. No mod code runs on a worker for it.
use super::{B, P, Padded, SectionMesh, TerrainVertex, pi};
use crate::connected::TileLayer;
use crate::fast_hash::FxHashMap;

/// No cull face: drawn whatever is beside the block.
const NO_CULL: u8 = 6;

/// One checked quad, its sprite resolved to its atlas layer.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedQuad {
    positions: [[f32; 3]; 4],
    uvs: [[f32; 2]; 4],
    color: u32,
    layer: u16,
    cull: u8,
    translucent: bool,
}

impl ResolvedQuad {
    /// A quad already checked by `StateMeshRecipe::check`: corners -1 to 2, UVs 0 to 1, all
    /// finite, `cull` 0 to 5, and `layer` the atlas layer of its sprite.
    pub fn new(
        positions: [[f32; 3]; 4],
        uvs: [[f32; 2]; 4],
        color: u32,
        layer: u16,
        cull: Option<u8>,
        translucent: bool,
    ) -> Self {
        debug_assert!(cull.is_none_or(|c| c < 6));
        Self {
            positions,
            uvs,
            color,
            layer,
            cull: cull.unwrap_or(NO_CULL),
            translucent,
        }
    }
}

/// Every mod's recipes for one state, composed: replacement drops the model once, every
/// mod's quads are drawn, and declared occlusions intersect.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolvedRecipe {
    replace: bool,
    occludes: Option<u8>,
    quads: Vec<ResolvedQuad>,
}

impl ResolvedRecipe {
    /// The faces through which the cell occludes its neighbours, when a recipe decides them:
    /// what was declared, or nothing for a replacement that declares nothing. `None` keeps the
    /// block's own occlusion.
    #[inline]
    pub(super) fn occlusion(&self) -> Option<u8> {
        match (self.replace, self.occludes) {
            (_, Some(faces)) => Some(faces),
            (true, None) => Some(0),
            (false, None) => None,
        }
    }
    /// Whether the block model's own quads are dropped.
    #[inline]
    pub(super) fn replaces(&self) -> bool {
        self.replace
    }
    /// How many quads it adds.
    pub fn quads(&self) -> usize {
        self.quads.len()
    }
}

/// The dense table by state: what the meshing workers read for every cell.
#[derive(Debug, PartialEq)]
pub struct MeshRecipes {
    /// By state id: the index in `recipes`, or `u32::MAX` for none.
    by_state: Box<[u32]>,
    recipes: Box<[ResolvedRecipe]>,
}

impl MeshRecipes {
    /// The recipe of state `id`.
    #[inline]
    pub(super) fn get(&self, id: u32) -> Option<&ResolvedRecipe> {
        let index = *self.by_state.get(id as usize)?;
        self.recipes.get(index as usize)
    }
}

/// The mods' recipes as the world thread composes them, before they are frozen.
pub struct MeshRecipesBuilder {
    states: usize,
    recipes: FxHashMap<u32, ResolvedRecipe>,
}

impl MeshRecipesBuilder {
    /// An empty table for `states` block states.
    pub fn new(states: usize) -> Self {
        Self {
            states,
            recipes: FxHashMap::default(),
        }
    }
    /// The state's recipe so far.
    pub fn get(&self, state: u32) -> Option<&ResolvedRecipe> {
        self.recipes.get(&state)
    }
    /// Composes one checked recipe into state `state` (below the state count).
    pub fn add(&mut self, state: u32, replace: bool, occludes: Option<u8>, quads: &[ResolvedQuad]) {
        debug_assert!((state as usize) < self.states);
        let recipe = self.recipes.entry(state).or_default();
        recipe.replace |= replace;
        recipe.occludes = match (recipe.occludes, occludes) {
            (Some(a), Some(b)) => Some(a & b),
            (a, b) => a.or(b),
        };
        recipe.quads.extend_from_slice(quads);
    }
    /// The frozen table; `None` when no state has a recipe, the no-recipe path.
    pub fn finish(self) -> Option<MeshRecipes> {
        if self.recipes.is_empty() {
            return None;
        }
        let mut by_state = vec![u32::MAX; self.states].into_boxed_slice();
        let mut recipes = Vec::with_capacity(self.recipes.len());
        let mut ordered: Vec<_> = self.recipes.into_iter().collect();
        ordered.sort_unstable_by_key(|(state, _)| *state);
        for (state, recipe) in ordered {
            by_state[state as usize] = recipes.len() as u32;
            recipes.push(recipe);
        }
        Some(MeshRecipes {
            by_state,
            recipes: recipes.into_boxed_slice(),
        })
    }
}

/// How many quads the recipes add to the centre cells of `pad`.
pub(super) fn added_quads(recipes: &MeshRecipes, pad: &Padded) -> usize {
    let mut total = 0;
    for y in 0..16 {
        for z in 0..16 {
            for x in 0..16 {
                if let Some(recipe) = recipes.get(pad.blocks[pi(x, y, z)]) {
                    total += recipe.quads.len();
                }
            }
        }
    }
    total
}

/// After the solid rows are read: a cell whose recipe occludes less than every face is no
/// longer solid, so the see-through graph and the enclosed-cell skip err toward drawing.
pub(super) fn open_recipes(recipes: &MeshRecipes, pad: &Padded, rows: &mut [u32]) {
    for y in -1..=16 {
        for z in -1..=16 {
            let row = (y + B) as usize * P + (z + B) as usize;
            for x in -1..=16 {
                let id = pad.blocks[pi(x, y, z)];
                if recipes
                    .get(id)
                    .and_then(ResolvedRecipe::occlusion)
                    .is_some_and(|faces| faces != 0b11_1111)
                {
                    rows[row] &= !(1 << (x + B));
                }
            }
        }
    }
}

/// The recipe's quads of the cell at `pos` (light `light`, packed sky high), those whose cull
/// face is not `visible` dropped.
pub(super) fn emit(
    mesher: &super::Mesher<'_>,
    recipe: &ResolvedRecipe,
    pos: [i32; 3],
    light: u8,
    visible: &[bool; 6],
    out: &mut SectionMesh,
) {
    let packed = u16::from(light >> 4) * 16 | (u16::from(light & 15) * 16) << 8;
    let base = pos.map(|v| v as f32);
    for quad in &recipe.quads {
        if quad.cull != NO_CULL && !visible[quad.cull as usize] {
            continue;
        }
        let [a, r, g, b] = quad.color.to_be_bytes().map(u32::from);
        let color = r | g << 8 | b << 16 | a << 24;
        let vertices = std::array::from_fn(|i| {
            let p = quad.positions[i];
            let at = [base[0] + p[0], base[1] + p[1], base[2] + p[2]];
            TerrainVertex::new(at, quad.uvs[i], 0, color, packed)
        });
        let tile = TileLayer::Layer {
            layer: quad.layer,
            translucent: quad.translucent,
            opaque: false,
        };
        mesher.quad_as(
            out,
            false,
            tile,
            vertices,
            false,
            Some(quad.color & 0xFF_FFFF),
        );
    }
}

#[cfg(test)]
#[path = "mesh_recipes_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "mesh_recipes_mesh_tests.rs"]
mod mesh_tests;
