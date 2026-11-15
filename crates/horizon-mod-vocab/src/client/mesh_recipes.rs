//! State mesh recipes (`section_mesh_recipes`, the section meshing design): the geometry a client
//! mod gives a block or state's terrain mesh, resolved once per resource load on the world thread
//! into a dense table by state that the meshing workers only read.
#[allow(unused_imports)]
use crate::*;

/// The most quads one recipe may add, and so one block position: the design's per-position bound.
pub const MAX_RECIPE_QUADS: usize = 256;

/// What a [`StateMeshRecipe`] applies to.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(horizon_mod_api_macros::Wire)]
pub enum MeshRecipeTarget {
    /// Every state of a block (`minecraft:glass`, `mymod:frame`).
    Block(String),
    /// One exact state. An exact state's recipe follows its block's.
    State(BlockStateId),
}

/// One quad a recipe adds, in blocks from the block's lowest corner. Its corners go
/// counter-clockwise seen from the side it faces; a solid quad is culled from behind.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
#[derive(horizon_mod_api_macros::Wire)]
pub struct MeshQuad {
    /// The corners, each axis -1 to 2: a quad may reach one block past its own, never further.
    pub positions: [[f32; 3]; 4],
    /// Each corner's texture coordinate within the sprite, 0 to 1.
    pub uvs: [[f32; 2]; 4],
    /// The block atlas sprite it draws (`minecraft:block/glass`).
    pub sprite: String,
    /// ARGB the sprite's texels are multiplied by; `0xFFFFFFFF` leaves them as they are.
    pub color: u32,
    /// The face (0 down, 1 up, 2 north, 3 south, 4 west, 5 east) whose culling drops this quad, as a
    /// model face's `cullface` does; `None` draws it whatever is beside the block.
    pub cull: Option<u8>,
    /// Drawn in the translucent pass (sorted, blended) rather than the solid one.
    pub translucent: bool,
}

impl MeshQuad {
    /// A solid quad of `sprite` at `positions` with `uvs`, untinted and unculled.
    pub fn new(positions: [[f32; 3]; 4], uvs: [[f32; 2]; 4], sprite: impl Into<String>) -> Self {
        Self {
            positions,
            uvs,
            sprite: sprite.into(),
            color: 0xFFFF_FFFF,
            cull: None,
            translucent: false,
        }
    }
    /// The same quad, dropped when face `face` is culled.
    pub fn culled_by(mut self, face: u8) -> Self {
        self.cull = Some(face);
        self
    }
    /// The same quad in the translucent pass.
    pub fn translucent(mut self) -> Self {
        self.translucent = true;
        self
    }
    /// The same quad tinted `argb`.
    pub fn tinted(mut self, argb: u32) -> Self {
        self.color = argb;
        self
    }
}

/// The geometry a mod gives a block or state in terrain meshes.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
#[derive(horizon_mod_api_macros::Wire)]
pub struct StateMeshRecipe {
    /// What it applies to.
    pub target: MeshRecipeTarget,
    /// Whether the block model's own quads are dropped first (with no `quads`, the block draws
    /// nothing of its model). Two mods replacing the same state are refused, the later one.
    pub replace: bool,
    /// Quads drawn after the model's (or instead of them), at most [`MAX_RECIPE_QUADS`].
    pub quads: Vec<MeshQuad>,
    /// The faces (bit 0 down to 5 east) through which the block fully occludes its neighbours.
    /// `None` keeps the block's own occlusion, or, for a replacing recipe, occludes nothing.
    pub occludes: Option<u8>,
}

impl StateMeshRecipe {
    /// Adds `quads` to the model of `target`.
    pub fn append(target: MeshRecipeTarget, quads: Vec<MeshQuad>) -> Self {
        Self {
            target,
            replace: false,
            quads,
            occludes: None,
        }
    }
    /// Draws `quads` instead of the model of `target` (none for nothing at all).
    pub fn replace(target: MeshRecipeTarget, quads: Vec<MeshQuad>) -> Self {
        Self {
            target,
            replace: true,
            quads,
            occludes: None,
        }
    }
    /// The same recipe, occluding through the faces in `faces`.
    pub fn occluding(mut self, faces: u8) -> Self {
        self.occludes = Some(faces);
        self
    }
    /// Checks the recipe's bounds as the game does before it uses any of a mod's recipes (the
    /// sprite and the target are checked against the loaded resources there).
    pub fn check(&self) -> Result<(), MeshRecipeRefusal> {
        if self.quads.len() > MAX_RECIPE_QUADS {
            return Err(MeshRecipeRefusal::TooManyQuads {
                count: u32::try_from(self.quads.len()).unwrap_or(u32::MAX),
            });
        }
        if self.occludes.is_some_and(|faces| faces > 63) {
            return Err(MeshRecipeRefusal::FaceMask);
        }
        for (index, quad) in self.quads.iter().enumerate() {
            let index = index as u32;
            let finite = quad.positions.iter().flatten().all(|v| v.is_finite())
                && quad.uvs.iter().flatten().all(|v| v.is_finite());
            if !finite {
                return Err(MeshRecipeRefusal::NotFinite { quad: index });
            }
            let inside = quad
                .positions
                .iter()
                .flatten()
                .all(|v| (-1.0..=2.0).contains(v))
                && quad.uvs.iter().flatten().all(|v| (0.0..=1.0).contains(v));
            if !inside {
                return Err(MeshRecipeRefusal::OutOfRange { quad: index });
            }
            if quad.cull.is_some_and(|face| face > 5) {
                return Err(MeshRecipeRefusal::FaceMask);
            }
        }
        Ok(())
    }
}

/// Why a mod's recipes were refused. A refused recipe refuses all of that mod's recipes for the
/// resource load, logged once with the mod named, and its blocks draw their own models.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(horizon_mod_api_macros::Wire)]
pub enum MeshRecipeRefusal {
    /// `E_MESH_RECIPE_TOO_MANY`: more than [`MAX_RECIPE_QUADS`] quads for one position.
    TooManyQuads {
        /// How many there were.
        count: u32,
    },
    /// `E_MESH_RECIPE_NOT_FINITE`: a corner or UV that is not a number or infinite.
    NotFinite {
        /// The quad's index in the recipe.
        quad: u32,
    },
    /// `E_MESH_RECIPE_OUT_OF_RANGE`: a corner outside -1 to 2 or a UV outside 0 to 1.
    OutOfRange {
        /// The quad's index in the recipe.
        quad: u32,
    },
    /// `E_MESH_RECIPE_FACE`: a cull face above 5 or an occlusion mask above 63.
    FaceMask,
    /// `E_MESH_RECIPE_SPRITE`: a sprite the block atlas lacks.
    UnknownSprite {
        /// The sprite named.
        sprite: String,
    },
    /// `E_MESH_RECIPE_TARGET`: a block no registry holds, or a state id past the state table.
    UnknownTarget,
    /// `E_MESH_RECIPE_REPLACED`: the state already has another mod's replacement.
    Replaced {
        /// The mod whose replacement stands.
        by: String,
    },
    /// `E_MESH_RECIPE_PANIC`: the mod's listener panicked or trapped.
    Failed,
}

impl MeshRecipeRefusal {
    /// The refusal's stable code, `E_MESH_RECIPE_...`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooManyQuads { .. } => "E_MESH_RECIPE_TOO_MANY",
            Self::NotFinite { .. } => "E_MESH_RECIPE_NOT_FINITE",
            Self::OutOfRange { .. } => "E_MESH_RECIPE_OUT_OF_RANGE",
            Self::FaceMask => "E_MESH_RECIPE_FACE",
            Self::UnknownSprite { .. } => "E_MESH_RECIPE_SPRITE",
            Self::UnknownTarget => "E_MESH_RECIPE_TARGET",
            Self::Replaced { .. } => "E_MESH_RECIPE_REPLACED",
            Self::Failed => "E_MESH_RECIPE_PANIC",
        }
    }
}

impl std::fmt::Display for MeshRecipeRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code = self.code();
        match self {
            Self::TooManyQuads { count } => write!(f, "{code}: {count} quads, more than 256"),
            Self::NotFinite { quad } => write!(f, "{code}: quad {quad} is not finite"),
            Self::OutOfRange { quad } => write!(f, "{code}: quad {quad} is out of range"),
            Self::FaceMask => write!(f, "{code}: a face above 5 or a mask above 63"),
            Self::UnknownSprite { sprite } => {
                let end = sprite.floor_char_boundary(128);
                write!(f, "{code}: no block atlas sprite {}", &sprite[..end])
            }
            Self::UnknownTarget => write!(f, "{code}: no such block or state"),
            Self::Replaced { by } => write!(f, "{code}: the state is already replaced by {by}"),
            Self::Failed => write!(f, "{code}: the listener panicked or trapped"),
        }
    }
}

impl std::error::Error for MeshRecipeRefusal {}
