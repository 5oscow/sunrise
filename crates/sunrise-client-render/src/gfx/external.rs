//! The hook an external shader pipeline fills (#14035, layer plan M6). A shader pack's runtime
//! (a platform crate, later) implements [`ExternalPipeline`]; the renderer calls it at fixed
//! points of a world frame and draws vanilla's way wherever it doesn't answer. With none set
//! (`Renderer::set_external_pipeline`), each point is one `Option` test.
//!
//! Workspace-internal: the trait names the engine's frame, its mesh format and its pass order,
//! has one implementer in this workspace, carries no stability promise and is never exposed to
//! mods. A mesher or pass change updates its implementer in the same PR. HDR output
//! (`output.rs`, `frame_policy`) is not an implementer: it stays the one output stage after the
//! frame, and a pipeline draws into the frame it gives.
//!
//! Shadows are an engine frame point, on request: a pipeline that wants a shadow pass answers
//! [`ShadowView`], the engine culls the sections from it ([`ShadowCull`]) and hands back the
//! terrain seen from the light. Vanilla's path never asks, so it never computes shadow
//! visibility.

use glam::{Vec3, Vec4};

use super::{Cull, EffectiveOutput, Frame, Frustum, Sides, facing_runs, frustum};

/// What a world frame gives an external pipeline at its points: the frame's own values, and
/// the display's from the renderer's one output snapshot (`EffectiveOutput`: whether HDR is on,
/// paper white, peak), never worked out again, so each split-screen window keeps its own.
pub struct FrameState<'a> {
    pub frame: &'a Frame<'a>,
    pub output: &'a EffectiveOutput,
}

/// What an external pipeline draws itself this frame: vanilla's own draw of each is skipped. All
/// `false` is vanilla's frame. Asked once a world frame (`ExternalPipeline::draws`), so the
/// engine tests plain values at each point, not the pipeline.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Draws {
    /// It runs the world's post passes in place of `world_post`, so the world is drawn into
    /// targets of its own and kept with its depth.
    pub reads_world: bool,
    /// The sky disc, sunset, stars and void; with the colour the world clears to (`None`: fog).
    pub sky: bool,
    pub sky_clear: Option<[f32; 4]>,
    /// The sun and moon.
    pub celestial: bool,
    /// Terrain (solid and cutout), or nothing draws it.
    pub terrain: bool,
    /// Translucent terrain.
    pub water: bool,
    /// Terrain from the light: it has a shadow pass (`shadow_request`).
    pub shadow: bool,
    pub clouds: bool,
    /// The block outline.
    pub outline: bool,
    pub hand: bool,
    /// Rain and snow.
    pub weather: bool,
    /// Entities, from the vertices the renderer lists for them.
    pub entities: bool,
    /// Entity models posed on the GPU land in its buffer (`Frame::skin`).
    pub skins_entities: bool,
}

/// Which entities a pipeline's shadow pass draws (none without a shadow pass): what the renderer
/// must keep in the frame's lists though the camera culls them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShadowCasters {
    /// Entities or block entities other than the player.
    pub others: bool,
    /// The local player's model, built in first person too.
    pub player: bool,
    /// Mobs other than the player: vanilla's blob shadows give way to it.
    pub mobs: bool,
}

/// A world frame's entity vertices and what goes with them, for a pipeline that draws some of
/// them (`ExternalPipeline::prepare_entities`).
pub struct EntityInput<'a> {
    pub entities: &'a [super::Vertex],
    /// The runs of `entities` by what they draw (`Frame::entity_spans`).
    pub spans: &'a [crate::render_geometry::EntitySpan],
    pub crumbling: [f32; 4],
    pub portal: [f32; 4],
    /// The models posed on the GPU, when the pipeline skins them (`Draws::skins_entities`).
    pub skin: Option<&'a super::entity_skin::SkinFrame>,
    pub motion: (&'a [(u32, u64)], &'a [u32]),
}

/// Which of the frame's terrain draws: the camera's solid or translucent ones, or the shadow
/// camera's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainList {
    Solid,
    Translucent,
    ShadowSolid,
    ShadowTranslucent,
}

/// Draws the game's terrain of one list into a pass; calls its third argument with each arena
/// page before drawing from it. Indexed, or for a program with a geometry stage, without
/// indices and with that many instances (and its index buffer bound in set 5 by this layout).
pub type TerrainDraw<'a> = dyn FnMut(
        &mut wgpu::RenderPass<'_>,
        TerrainList,
        &dyn Fn(&mut wgpu::RenderPass<'_>, u32),
        Option<(u32, &wgpu::BindGroupLayout)>,
    ) + 'a;

/// This frame's block outline, when the pipeline draws it: the outline buffer (one instance
/// per edge) and the edges in it.
pub struct Lines {
    pub edges: wgpu::Buffer,
    pub count: u32,
}

/// This frame's clouds, when the pipeline draws them: their vertices, uniform, quad indices and
/// quad count.
pub struct Clouds {
    pub vertices: wgpu::Buffer,
    pub group: wgpu::BindGroup,
    pub indices: wgpu::Buffer,
    pub quads: u32,
}

/// What the renderer hands a pipeline before its post passes (`ExternalPipeline::feed_post`):
/// the frame's layers it draws itself, as the engine keeps them.
pub struct PostFeed<'a> {
    pub device: &'a wgpu::Device,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub uploads: &'a mut super::upload::Uploads,
    pub frame: &'a Frame<'a>,
    /// Whether it draws rain and snow this frame.
    pub weather: bool,
    /// The GUI atlas, which rain and snow take their sprites from.
    pub atlas: Option<&'a wgpu::Texture>,
    /// The terrain arena's pages, when the pipeline draws terrain flat this frame (`None` otherwise).
    pub flat_pages: Option<&'a super::arena::Arena>,
    pub quad_indices: &'a wgpu::Buffer,
    pub lines: Option<Lines>,
    pub clouds: Option<Clouds>,
}

/// What the renderer hands a pipeline to run the world's post passes (`ExternalPipeline::post`):
/// the world drawn into `color` and `depth`, the screen to draw the result onto, and the
/// terrain's draw lists.
pub struct PostContext<'a, 'd> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub frame: &'a Frame<'a>,
    pub color: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    pub target: &'a wgpu::TextureView,
    /// The world's size in pixels, and the window's.
    pub size: [u32; 2],
    pub window: [u32; 2],
    pub terrain: &'a mut TerrainDraw<'d>,
}

/// An external shader pipeline, called by the renderer at fixed points of a world frame (not
/// title frames). Every method has a default that leaves vanilla's way in place.
pub trait ExternalPipeline: Send {
    /// A world frame starts, before culling.
    fn begin_frame(&mut self, _state: &FrameState<'_>) {}
    /// What it draws itself this frame; vanilla draws the rest.
    fn draws(&self, _state: &FrameState<'_>) -> Draws {
        Draws::default()
    }
    /// The shadow pass this frame wants; `None` draws none.
    fn shadow_request(&self, _state: &FrameState<'_>) -> Option<ShadowView> {
        None
    }
    /// Takes the entities it draws (`Draws::entities`) out of `input` for this frame's draw and
    /// puts the rest, which the renderer draws, in `rest`.
    fn prepare_entities(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _input: &EntityInput<'_>,
        _rest: &mut Vec<super::Vertex>,
    ) {
    }
    /// Where entity models posed on the GPU go, with `Draws::skins_entities`.
    fn skin_target(&self) -> Option<&wgpu::Buffer> {
        None
    }
    /// The entities its shadow pass draws this frame.
    fn shadow_casters(&self) -> ShadowCasters {
        ShadowCasters::default()
    }
    /// Before the world's post passes: takes the layers it draws itself (`Draws`) from `feed`.
    fn feed_post(&mut self, _feed: PostFeed<'_>) {}
    /// Runs the world's post passes in place of `world_post` (TAA, upscaling, sharpening) over
    /// the world drawn into `ctx.color`, and draws the result onto `ctx.target`. Called only
    /// with `Draws::reads_world`.
    fn post(&mut self, _ctx: PostContext<'_, '_>) {}
    /// The window's drawable size changed.
    fn resize(&mut self, _size: [u32; 2]) {}
    /// Forget what was kept from earlier frames: the next is drawn as a fresh pipeline's first.
    fn reset_history(&mut self, _device: &wgpu::Device) {}
    /// Right after the frame's submission: what to run once the GPU has it (a readback of what
    /// the frame copied), if anything.
    fn submitted(&mut self) -> Option<Box<dyn FnOnce() + Send>> {
        None
    }
    /// The depth under the screen's centre, copied by a finished frame, taken after the GPU
    /// waited for it. Errors when the pipeline copied none.
    fn take_center_depth(&mut self, _device: &wgpu::Device) -> Result<(), String> {
        Err("the shader pack copied no centre depth".into())
    }
    /// The pipeline's pass timer, while it has one slot to time into (`Renderer::set_pass_timing`).
    fn timing(&mut self) -> Option<&mut Option<super::pack_timing::PassTiming>> {
        None
    }
}

/// What the shadow camera sees of the terrain, for the game's draw lists: its view and
/// projection in wgpu's clip space, the direction toward the light, and how far from the camera
/// terrain is drawn (`shadowDistanceRenderMul`).
#[derive(Clone, Copy, Debug)]
pub struct ShadowView {
    pub(crate) cull: glam::Mat4,
    pub(crate) light: glam::Vec3,
    pub(crate) distance: Option<f32>,
    /// Draw everything, every face, within this many blocks (`shadow.culling=false|reversed`):
    /// packs that voxelize the world in their shadow pass need all of it.
    pub(crate) unculled: Option<f32>,
    /// Every face of the drawn sections, not only those that can face the light. Iris 1.11.7's
    /// shadow map holds the faces turned away from the light too (#9033): under a hill at low sun
    /// a cave's ceiling, which faces away, is often all that shadows the cave.
    pub(crate) every_face: bool,
    /// Only the sections whose shadow can fall into the camera's view: those the camera's
    /// frustum, stretched toward the light, takes in (`shadow_casters`). Our probes' Iris 1.11.7
    /// maps fit keeping only those with `shadow.culling` unset (#8464): under a noon sun, only
    /// the terrain above the ground the camera sees; with a low sun ahead, none behind the camera.
    pub(crate) view_casters: bool,
    /// Whether the projection's near and far planes in `cull` drop sections too. Our probes'
    /// Iris 1.11.7 maps hold the terrain at every depth we put it along the light (from 535 blocks
    /// beyond the shadow camera to about 120 before it), whatever `shadowNearPlane` and
    /// `shadowFarPlane` the pack sets, or none (#10403): only its sides cull.
    pub(crate) depth_planes: bool,
}

impl ShadowView {
    /// Construct a basic `ShadowView` with default flags (not every face, no view casters, no depth planes).
    #[inline]
    pub fn new(cull: glam::Mat4, light: glam::Vec3) -> Self {
        Self {
            cull,
            light,
            distance: None,
            unculled: None,
            every_face: false,
            view_casters: false,
            depth_planes: false,
        }
    }

    #[inline]
    pub fn with_distance(mut self, distance: Option<f32>) -> Self {
        self.distance = distance;
        self
    }

    #[inline]
    pub fn with_unculled(mut self, unculled: Option<f32>) -> Self {
        self.unculled = unculled;
        self
    }

    #[inline]
    pub fn with_every_face(mut self, every_face: bool) -> Self {
        self.every_face = every_face;
        self
    }

    #[inline]
    pub fn with_view_casters(mut self, view_casters: bool) -> Self {
        self.view_casters = view_casters;
        self
    }

    #[inline]
    pub fn with_depth_planes(mut self, depth_planes: bool) -> Self {
        self.depth_planes = depth_planes;
        self
    }

    #[inline]
    pub fn cull(&self) -> glam::Mat4 {
        self.cull
    }

    #[inline]
    pub fn light(&self) -> glam::Vec3 {
        self.light
    }

    #[inline]
    pub fn distance(&self) -> Option<f32> {
        self.distance
    }

    #[inline]
    pub fn unculled(&self) -> Option<f32> {
        self.unculled
    }

    #[inline]
    pub fn every_face(&self) -> bool {
        self.every_face
    }

    #[inline]
    pub fn view_casters(&self) -> bool {
        self.view_casters
    }

    #[inline]
    pub fn depth_planes(&self) -> bool {
        self.depth_planes
    }
}

/// A shader pack's shadow camera's box, and with `casters` only what can shade the camera's view
/// (`shadow_casters`): a box must meet all of them.
#[derive(Clone, Copy, Debug)]
pub struct ShadowCull {
    pub(crate) light: Frustum,
    pub(crate) casters: Option<[Frustum; 2]>,
    /// `shadowDistance × shadowDistanceRenderMul`: only what lies within this many blocks of the
    /// camera along each axis, a cube around it. Our probe's Iris 1.11.7 maps keep the sections
    /// whose box, grown by `SHADOW_CASTER_MARGIN` as for the casters, meets that cube (#8464):
    /// out to its corners, and a section 1 block past its side but not one 1.5 blocks past.
    pub(crate) reach: Option<f32>,
}
impl ShadowCull {
    /// Construct a shadow cull for the shadow camera's box `light`.
    #[inline]
    pub fn new(light: Frustum) -> Self {
        Self {
            light,
            casters: None,
            reach: None,
        }
    }

    /// Set optional shadow casters (`[near, far]`) that can shade the view.
    #[inline]
    pub fn with_casters(mut self, casters: Option<[Frustum; 2]>) -> Self {
        self.casters = casters;
        self
    }

    /// Set the maximum reach cube distance along each axis.
    #[inline]
    pub fn with_reach(mut self, reach: Option<f32>) -> Self {
        self.reach = reach;
        self
    }

    /// The shadow camera's light frustum box.
    #[inline]
    pub fn light(&self) -> &Frustum {
        &self.light
    }

    /// The casters frustums, if any.
    #[inline]
    pub fn casters(&self) -> Option<&[Frustum; 2]> {
        self.casters.as_ref()
    }

    /// The reach limit in blocks along each axis, if set.
    #[inline]
    pub fn reach(&self) -> Option<f32> {
        self.reach
    }

    /// Whether the camera-relative box `min..max`, grown by the margin, meets (`whole`: lies
    /// within) `reach`'s cube.
    fn within_reach(&self, min: Vec3, max: Vec3, whole: bool) -> bool {
        let (min, max) = (min - SHADOW_CASTER_MARGIN, max + SHADOW_CASTER_MARGIN);
        self.reach.is_none_or(|d| {
            if whole {
                min.cmpge(Vec3::splat(-d)).all() && max.cmple(Vec3::splat(d)).all()
            } else {
                max.cmpge(Vec3::splat(-d)).all() && min.cmple(Vec3::splat(d)).all()
            }
        })
    }
}
/// How far past a section's 16 blocks the shadow pass looks for what can shade the view: our
/// probes' Iris 1.11.7 maps keep the sections whose box, grown by this much on every side, meets
/// `shadow_casters`' shape. Over 76 scenes at fields of view 30 to 110, Iris keeps every section
/// that needs up to 1.1224 blocks and drops every one that needs 1.1252 or more (#8464): one
/// growth, whatever the field of view, of 9/8 (a fit, not Iris's code). Without it the edges of
/// the wedge lose a column of terrain.
const SHADOW_CASTER_MARGIN: f32 = 1.125;
impl Cull for ShadowCull {
    fn intersects(&self, min: Vec3, max: Vec3) -> bool {
        let (grown_min, grown_max) = (min - SHADOW_CASTER_MARGIN, max + SHADOW_CASTER_MARGIN);
        self.within_reach(min, max, false)
            && self.light.intersects(min, max)
            && self
                .casters
                .is_none_or(|c| c.iter().all(|f| f.intersects(grown_min, grown_max)))
    }
    fn contains(&self, min: Vec3, max: Vec3) -> bool {
        let (grown_min, grown_max) = (min - SHADOW_CASTER_MARGIN, max + SHADOW_CASTER_MARGIN);
        self.within_reach(min, max, true)
            && self.light.contains(min, max)
            && self
                .casters
                .is_none_or(|c| c.iter().all(|f| f.contains(grown_min, grown_max)))
    }
    /// Our probe's Iris 1.11.7 maps fit testing the casters' whole sections (a guess that matched).
    fn tight(&self) -> bool {
        self.casters.is_none()
    }
}
/// The shadow camera's box that the shadow pass draws terrain in: the sides of `view.cull`, and
/// its near and far planes only with `depth_planes`.
pub(super) fn shadow_box(view: &ShadowView) -> Frustum {
    let mut planes = frustum(view.cull);
    if !view.depth_planes {
        // `frustum`'s faces at depth 0 and 1 take in everything.
        planes[4] = Vec4::W;
        planes[5] = Vec4::W;
    }
    Frustum::from_planes(planes)
}

/// The runs of a section's solid quads the shadow pass draws: every face within the unculled
/// distance, or everywhere for `every_face`; else the quads that can face the light.
pub(super) fn shadow_runs(
    sides: &Sides,
    view: &ShadowView,
    origin: [f32; 3],
    draw: impl FnMut(u32, u32),
) {
    let unculled = view.unculled.is_some_and(|d| {
        let o = Vec3::from(origin) + 8.0;
        o.x.abs().max(o.z.abs()) <= d + 8.0
    });
    let toward = (!unculled && !view.every_face).then_some(view.light * 1.0e5);
    facing_runs(sides, toward, origin, draw);
}
