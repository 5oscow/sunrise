//! The engine's extension points for resource pack formats vanilla doesn't read (OptiFine's,
//! Polytone's: platform code, docs/layers.md). The engine asks [`formats`] once per pack load and
//! gets engine types back (tables, or an object it calls at most once a frame), never a format's
//! own types. The platform installs its formats at startup ([`install`]); with none installed,
//! packs load as vanilla reads them.

use crate::{
    connected::ConnectedTextures, fusion::FusionTextures, models::StateModels,
    resource_source::ResourceSource, sprites::Sprites,
};
use std::sync::{Arc, OnceLock};
use sunrise_world::registry::BlockRegistry;

/// Connected-texture rules that affect both sprite allocation and block-model baking.
/// Only the pack loader calls this; mesh workers read the baked tables directly.
pub trait ConnectedTextureRules {
    /// Reserve distinct layers for the sprites the rules match by name, before baking models.
    fn keep_sprites_apart(&self, sprites: &mut Sprites);

    /// Insert the rule tiles and resolve the rules against the baked models.
    fn bake(
        self: Box<Self>,
        root: &ResourceSource,
        registry: &BlockRegistry,
        sprites: &mut Sprites,
        models: &StateModels,
    ) -> ConnectedTextures;
}

/// What the pack formats beyond vanilla's add to a pack. Each method reads one overlay of a pack
/// (its folder at `root`); the default is "nothing in this format".
pub trait PackFormats: Send + Sync {
    /// Rules that need to reserve sprite layers before model baking.
    fn connected_texture_rules(
        &self,
        _root: &ResourceSource,
    ) -> Option<Box<dyn ConnectedTextureRules>> {
        None
    }

    /// Connecting model faces whose rules are resolved after model baking.
    fn connecting_models(
        &self,
        _root: &ResourceSource,
        _registry: &BlockRegistry,
        _sprites: &mut Sprites,
        _models: &mut StateModels,
    ) -> FusionTextures {
        FusionTextures::default()
    }

    /// The colours the overlay replaces where vanilla hardcodes them.
    fn colors(&self, _root: &ResourceSource) -> Option<crate::pack_palette::PackColors> {
        None
    }

    /// The name ending of the overlay's emissive textures (`stone_e.png` beside `stone.png`),
    /// if its format has them.
    fn emissive_suffix(&self, _root: &ResourceSource) -> Option<String> {
        None
    }

    /// The sky layers the overlay draws over the vanilla sky, if it has any.
    fn custom_sky(
        &self,
        _root: &crate::resource_source::ResourceSource,
    ) -> Option<Arc<dyn crate::custom_sky::SkyLayers>> {
        None
    }

    /// The durability bar colours the overlay sets by item, if any.
    fn item_bars(&self, _root: &ResourceSource) -> Option<Arc<dyn crate::pack_palette::ItemBars>> {
        None
    }
}

/// No format beyond vanilla's.
struct Vanilla;

impl PackFormats for Vanilla {}

static FORMATS: OnceLock<&'static dyn PackFormats> = OnceLock::new();

/// Installs the pack formats, before the first pack loads. The first call wins: later ones are
/// ignored, so each test or probe that needs the formats may install them.
pub fn install(formats: &'static dyn PackFormats) {
    let _ = FORMATS.set(formats);
}

/// The installed pack formats, or vanilla's alone.
pub fn formats() -> &'static dyn PackFormats {
    FORMATS.get().copied().unwrap_or(&Vanilla)
}
