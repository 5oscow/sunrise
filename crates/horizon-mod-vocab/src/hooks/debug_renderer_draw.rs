//! The `debug_renderer_draw` hooks: declarations the mod API generator reads (see `hooks/mod.rs`).

pub trait Hooks {
    /// Draw one active debug renderer (`register_debug_renderers`), asked for each view of a world,
    /// after `submit_custom_geometry`. `id` is the renderer's id and every mod is asked about every id,
    /// so a mod ignores ids it does not own. `draw` shares its limits with that hook and clears every
    /// view, so nothing outlives a frame. A renderer that is not active is never asked, and neither is
    /// one without a world to draw in. Client-side.
    #[hook(
        signature = "pub(in crate::game) fn native_debug_renderer_draw( &self, id: &str, view: &mod_api::WorldRenderView, draw: &mut horizon_client::world_geometry::WorldGeometry, )",
        side = "client",
        contract = "listen",
        cancellable = false
    )]
    fn debug_renderer_draw();
}
