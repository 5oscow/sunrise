//! Sunrise Replay Studio editor.

pub mod camera;
pub mod path_render;
pub mod spline;
pub mod timeline_ui;
pub mod topbar;

pub use camera::StudioFreecam;
pub use path_render::render_camera_path;
pub use spline::{CameraKeyframe, CameraTrack, EvaluatedPose};
pub use timeline_ui::{TimelineAction, TimelineUi};
pub use topbar::{TopBarAction, TopBarUi};
