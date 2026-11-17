//! A `shaders/` pack as an external pipeline (`external::ExternalPipeline`): what the renderer
//! asks of it at the frame's points, answered from the pack's programs.

use super::external::{Draws, ExternalPipeline, FrameState, ShadowCasters, ShadowView};
use super::pack_post::PackPost;

impl ExternalPipeline for PackPost {
    fn draws(&self, _state: &FrameState<'_>) -> Draws {
        Draws {
            reads_world: true,
            sky: self.has_sky(),
            sky_clear: self.sky_clear(),
            celestial: self.has_celestial(),
            terrain: self.has_terrain(),
            water: self.has_water(),
            shadow: self.has_shadow(),
            clouds: self.has_clouds(),
            outline: self.has_lines(),
            hand: self.has_hand(),
            weather: self.has_weather(),
            entities: self.has_entities(),
            skins_entities: self.skins_entities(),
        }
    }

    fn shadow_request(&self, state: &FrameState<'_>) -> Option<ShadowView> {
        self.shadow_view(state.frame)
    }

    fn shadow_casters(&self) -> ShadowCasters {
        let c = PackPost::shadow_casters(self);
        ShadowCasters {
            others: (c.entities && !c.only_player) || c.blocks,
            player: c.entities,
            mobs: c.entities && !c.only_player,
        }
    }
}
