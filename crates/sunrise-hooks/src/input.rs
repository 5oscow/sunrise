//! The input source (docs/layers.md, "Controllers, splitscreen, replays and UI pages"): what a
//! device beyond the keyboard and mouse (a controller) tells the game and is told by it. A crate
//! above the engine offers a source through [`crate::Hooks::input_source`]; each `Game` (each
//! splitscreen player) holds its own. With none offered the engine runs vanilla's keyboard and
//! mouse path and asks nothing more than whether it has a source.
//!
//! The engine implements [`InputHost`], the whole surface a source gets. It names only values of
//! its own and plain types, never a `Game`.

/// Something that happened to the player that a device may let them feel (a controller's
/// vibration). The device decides how, or whether.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Feedback {
    /// The player broke a block.
    BlockBroken,
    /// An explosion this far away (blocks) with this power.
    Explosion { distance: f64, radius: f32 },
    /// A totem of undying saved the player.
    Totem,
}

/// What the engine offers a source, as it holds or releases the player's view for one call.
pub trait InputHost {
    /// Whether a menu or a screen has the input rather than the world.
    fn in_menu(&self) -> bool;
    /// Whether the game's window has the focus.
    fn window_focused(&self) -> bool;
    /// Whether the window has a player entity to turn and move.
    fn has_player(&self) -> bool;
    /// Whether the player looks through a spyglass (the look slows).
    fn scoping(&self) -> bool;
    /// Whether the cinematic camera is on (the turn eases in and out).
    fn smooth_camera(&self) -> bool;
    /// Turns the view as the mouse does, by `yaw` and `pitch` in mouse units (`Entity.turn`).
    fn turn(&mut self, yaw: f64, pitch: f64);
}

/// One device's side of the game, for one `Game`. The engine calls it at the places vanilla's
/// keyboard and mouse act; every method has the do-nothing answer a missing device gives.
pub trait InputSource {
    /// Once a loop iteration, before the frame's input is acted on.
    fn frame(&mut self, host: &mut dyn InputHost);

    /// The analog move vector (`[left, forward]`, -1..1) while the device moves the player.
    fn move_vector(&self, host: &dyn InputHost) -> Option<[f32; 2]>;

    /// Whether the device holds the key mapping named `mapping` (`key.use`).
    fn holds(&self, mapping: &str) -> bool;

    /// Turns the view by what the device gives this frame, through [`InputHost::turn`].
    fn look(&mut self, host: &mut dyn InputHost, seconds: f32);

    /// The keyboard or mouse acted in play (a key or button pressed): it has control again.
    fn keyboard_used(&mut self);

    /// Whether the pointer event being handled is one the device made, not the mouse.
    fn made_pointer_event(&self) -> bool;

    /// A new player entity took over (spawn, respawn, a server joined, a dimension change).
    fn player_replaced(&mut self, host: &mut dyn InputHost);

    /// Whether the drop about to happen is the device's whole-stack drop. Asking answers once.
    fn takes_stack_drop(&mut self) -> bool;

    /// The HUD starts a frame: what the device drew on the last one is gone.
    fn hud_begin(&mut self);

    /// Something happened to the player that the device may let them feel.
    fn feedback(&mut self, event: Feedback);
}
