//! The engine's side of the input source (docs/layers.md, "Controllers, splitscreen, replays and
//! UI pages"): what gameplay tells a device beyond the keyboard and mouse. Gameplay names these
//! types, never a controller's; the controller code answers. The trait is
//! [`sunrise_hooks::input`]'s.
pub use sunrise_hooks::input::Feedback;
