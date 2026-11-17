//! A `shaders/` pack as an external pipeline (`external::ExternalPipeline`): what the renderer
//! asks of it at the frame's points, answered from the pack's programs.

use super::external::{
    Draws, EntityInput, ExternalPipeline, FrameState, PipelineFrame, PostContext, PostFeed,
    ShadowCasters, ShadowView,
};
use super::pack_post::{PackPost, TerrainPage};
use super::pack_timing::PassTiming as PackTiming;
use super::pack_weather::PackWeather;

/// A `shaders/` pack with what its frames keep beside the program: rain and snow, and the
/// terrain pages it reads this frame.
pub(super) struct PackPipeline {
    post: PackPost,
    weather: Option<PackWeather>,
    pages: Vec<TerrainPage>,
}

impl PackPipeline {
    pub(super) fn new(post: PackPost) -> Self {
        Self {
            post,
            weather: None,
            pages: Vec::new(),
        }
    }
}

impl ExternalPipeline for PackPipeline {
    fn frame_request(&self) -> PipelineFrame {
        PipelineFrame {
            terrain: PackPost::has_terrain(&self.post),
            shadow: PackPost::has_shadow(&self.post),
            skins_entities: PackPost::skins_entities(&self.post),
        }
    }

    fn draws(&self, _state: &FrameState<'_>) -> Draws {
        let p = &self.post;
        Draws {
            reads_world: true,
            sky: p.has_sky(),
            sky_clear: p.sky_clear(),
            celestial: p.has_celestial(),
            terrain: p.has_terrain(),
            water: p.has_water(),
            shadow: p.has_shadow(),
            clouds: p.has_clouds(),
            outline: p.has_lines(),
            hand: p.has_hand(),
            weather: p.has_weather(),
            entities: p.has_entities(),
            skins_entities: p.skins_entities(),
        }
    }

    fn shadow_request(&self, state: &FrameState<'_>) -> Option<ShadowView> {
        self.post.shadow_view(state.frame)
    }

    fn shadow_casters(&self) -> ShadowCasters {
        let c = PackPost::shadow_casters(&self.post);
        ShadowCasters {
            others: (c.entities && !c.only_player) || c.blocks,
            player: c.entities,
            mobs: c.entities && !c.only_player,
        }
    }

    fn prepare_entities(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input: &EntityInput<'_>,
        rest: &mut Vec<super::Vertex>,
    ) {
        PackPost::prepare_entities(
            &mut self.post,
            device,
            queue,
            input.entities,
            input.spans,
            input.crumbling,
            input.portal,
            rest,
            input.skin,
            input.motion,
        );
    }

    fn skin_target(&self) -> Option<&wgpu::Buffer> {
        PackPost::skin_target(&self.post)
    }

    fn feed_post(&mut self, feed: PostFeed<'_>) {
        let PostFeed {
            device,
            encoder,
            uploads,
            frame,
            weather,
            atlas,
            flat_pages,
            quad_indices,
            lines,
            clouds,
        } = feed;
        let weather = match atlas {
            Some(atlas) if weather => self
                .weather
                .get_or_insert_with(|| PackWeather::new(device))
                .prepare(
                    device,
                    encoder,
                    uploads,
                    atlas,
                    frame.weather,
                    frame.weather_time,
                    quad_indices,
                ),
            _ => None,
        };
        self.post.set_weather(weather);
        self.pages.clear();
        if let Some(arena) = flat_pages {
            self.pages.extend(
                arena
                    .page_buffers()
                    .map(|(p, b)| (p, b.clone(), arena.sources(p).cloned())),
            );
        }
        self.post.set_lines(lines);
        self.post.set_clouds(clouds);
    }

    fn post(&mut self, ctx: PostContext<'_, '_>) {
        self.post.run(
            ctx.device,
            ctx.queue,
            ctx.encoder,
            ctx.color,
            ctx.depth,
            ctx.target,
            ctx.frame,
            &ctx.frame.pack,
            ctx.size,
            ctx.window,
            &self.pages,
            ctx.terrain,
        );
        // Keep the capacity, not the buffers: a page the arena frees must not stay alive here
        // until the next shader-pack frame (or forever once the pack is off).
        self.pages.clear();
    }

    fn reset_history(&mut self, device: &wgpu::Device) {
        PackPost::reset_history(&mut self.post, device);
    }

    fn submitted(&mut self) -> Option<Box<dyn FnOnce() + Send>> {
        PackPost::submitted(&mut self.post).map(|f| Box::new(f) as Box<dyn FnOnce() + Send>)
    }

    fn take_center_depth(&mut self, device: &wgpu::Device) -> Result<(), String> {
        PackPost::take_center_depth(&mut self.post, device)
    }

    fn timing_slot(&mut self) -> Option<&mut Option<PackTiming>> {
        Some(&mut self.post.timing)
    }

    fn timing(&self) -> Option<&PackTiming> {
        self.post.timing.as_ref()
    }
}
