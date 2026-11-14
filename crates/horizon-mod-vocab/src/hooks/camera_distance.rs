//! The `camera_distance` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0114

pub trait Hooks {
    /// Final scaled third-person camera distance, before visual-shape collision clipping.
    /// `entity` includes its living scale/distance and optional mount scale/distance.
    /// Only finite requests in 0..=1024 blocks replace the vanilla distance.
    #[hook(
        signature = "pub(in crate::game) fn native_camera_distance( &self, partial: f32, mirrored: bool, entity: &mod_api::client::camera::CameraEntity, distance: &mut f32, )",
        target = "horizon_client::game::native_hooks::base::Game<'_>::native_camera_distance",
        side = "client",
        contract = "listen",
        neoforge = "CalculateDetachedCameraDistanceEvent"
    )]
    fn camera_distance();
}
