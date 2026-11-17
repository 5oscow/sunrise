//! OptiFine CTM needs one pass before model baking and one after it.

use crate::{
    connected::ConnectedTextures, models::StateModels, pack_hooks::ConnectedTextureRules,
    resource_source::ResourceSource, sprites::Sprites,
};
use sunrise_assets::model::Direction;
use sunrise_core::position::BlockPos;
use sunrise_optifine::ctm::Rule;
use sunrise_world::registry::BlockRegistry;

pub(super) struct Rules(pub(super) Vec<Rule>);

impl ConnectedTextureRules for Rules {
    fn keep_sprites_apart(&self, sprites: &mut Sprites) {
        sprites.keep_apart(ConnectedTextures::matched_sprites(&self.0));
    }

    fn bake(
        self: Box<Self>,
        root: &ResourceSource,
        registry: &BlockRegistry,
        sprites: &mut Sprites,
        models: &StateModels,
    ) -> ConnectedTextures {
        // Connect-by-tile rules look at the first quad facing the named side.
        let mut found = Vec::new();
        ConnectedTextures::load(root, self.0, registry, sprites, |state, face| {
            let side = Direction::ALL[face as usize];
            models
                .models_at(state, BlockPos::new(0, 0, 0), &mut found)
                .ok()?;
            found
                .iter()
                .flat_map(|m| &m.faces)
                .find(|f| f.quad.facing == side)
                .map(|f| f.layer)
        })
    }
}
