//! The `entity_stance` area: how an entity holds itself: spectating, crouching, sprinting, its pose. Host method declarations; access defaults and game forwarders are generated.
#![allow(unused_variables)]
use crate::*;
/// The handles' reads of the `entity_stance` area, implemented by the game and reached through [`Backend`].
// hidden: a handle's backend, reached through `Backend`; mods call the handle, never this trait
#[doc(hidden)]
#[sunrise_mod_api_macros::host_area]
pub trait EntityStanceBackend {
    /// Side: server.
    /// `Entity.isSpectator`: true for a player in spectator mode, false for
    /// every other entity.
    #[access(WorldOnly)]
    fn entity_spectator(&self, entity: EntityRef) -> Option<bool>;
    /// Side: both.
    /// `isSecondaryUseActive`: the local client's live semantic sneak state (not the
    /// physical key binding or last tick's crouching pose); the server's shift-key flag.
    /// False for non-player entities, None for a stale or unreachable handle.
    #[access(WorldOnly)]
    fn entity_is_shift_key_down(&self, entity: EntityRef) -> Option<bool>;
    /// Side: server.
    /// Whether the entity crouches (`Entity.isCrouching`: the crouching
    /// pose).
    #[access(WorldOnly)]
    fn entity_is_crouching(&self, entity: EntityRef) -> Option<bool>;
    /// Side: server.
    /// Whether the entity sprints (`Entity.isSprinting`): a player, or a
    /// cat or ocelot dashing after its target (their shared sprinting flag);
    /// false for the other kinds.
    #[access(WorldOnly)]
    fn entity_is_sprinting(&self, entity: EntityRef) -> Option<bool>;
    /// Side: server.
    /// The entity's pose (`Entity.getPose`) as `Pose` names it: `standing`,
    /// `crouching`, `swimming`, `sleeping`, a warden's `roaring`... Kinds
    /// with no pose stand.
    #[access(WorldOnly)]
    fn entity_pose(&self, entity: EntityRef) -> Option<&'static str>;
}
