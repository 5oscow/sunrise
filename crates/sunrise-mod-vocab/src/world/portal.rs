//! What asking a portal where it leads answers, and why it refuses.
use crate::{LevelRef, PortalShape};

/// Where an entity entering a portal arrives, from `LevelRef::portal_destination` and
/// `LevelRef::find_or_create_portal_exit`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
#[derive(sunrise_mod_api_macros::Wire)]
pub struct PortalExit {
    /// The level it arrives in.
    pub level: LevelRef,
    /// The bottom centre of its box there.
    pub pos: [f64; 3],
    /// The degrees to add to its yaw: 0, or 90 when the two nether portals' axes differ.
    pub yaw_turn: f32,
    /// The nether portal it arrives in (its bottom-left inside block, width, height and axis);
    /// None for an End portal and an End gateway.
    pub exit_portal: Option<PortalShape>,
}

impl PortalExit {
    /// An answer.
    pub fn new(
        level: LevelRef,
        pos: [f64; 3],
        yaw_turn: f32,
        exit_portal: Option<PortalShape>,
    ) -> Self {
        Self {
            level,
            pos,
            yaw_turn,
            exit_portal,
        }
    }
}

/// Why a portal destination is not answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(sunrise_mod_api_macros::Wire)]
pub enum PortalError {
    /// The block is no nether portal, End portal or End gateway.
    NotAPortal,
    /// The handle is stale, or the destination level does not exist.
    LevelMissing,
    /// The `allow_entering_nether_using_portals` game rule forbids the trip into the Nether.
    EnteringNotAllowed,
    /// No exit portal was found, and for `find_or_create_portal_exit` none could be built.
    NoExit,
    /// Inside a transaction: a built portal cannot be rolled back.
    InTransaction,
    /// The destination level is not open, or a chunk the search or the build reads is not loaded.
    ChunksNotLoaded,
    /// This side or build cannot answer.
    Unavailable,
}

impl crate::NoBackend for PortalError {
    fn no_backend() -> Self {
        Self::Unavailable
    }
}
