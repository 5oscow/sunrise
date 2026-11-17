//! Resolved connected-texture output consumed by terrain meshing.

pub use crate::pack_formats::optifine_ctm::ConnectedTextures;

/// A tile as the mesher draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileLayer {
    Layer {
        layer: u16,
        /// The sprite has fractional alpha (drawn in the translucent pass).
        translucent: bool,
        /// Every texel is opaque (drawn without an alpha test).
        opaque: bool,
    },
    /// Try the next rule.
    Skip,
    /// The face's own sprite.
    Default,
}

/// What a face draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Its own sprite.
    Keep,
    Tile(TileLayer),
    /// `ctm_compact`: a tile per quarter of the sprite (top-left, top-right, bottom-left,
    /// bottom-right).
    Quarters([TileLayer; 4]),
    /// Nothing but its overlays: a Fusion face whose tile is empty, or of the overlay layout.
    Nothing,
}

/// A tile drawn over a face, after its own (`overlay*` methods).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Overlay {
    pub layer: u16,
    /// `layer=translucent`: drawn in the translucent pass, else with an alpha cutout.
    pub translucent: bool,
    /// `tintIndex` and the state whose tint it is (`tintBlock`, else the face's block).
    pub tint: Option<(i32, Option<u32>)>,
}
