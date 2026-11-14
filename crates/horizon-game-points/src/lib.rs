//! Canonical records and the complete catalog of this game's injection points.
//! This engine-owned data is shared by actual sites and native/wasm SDK listeners; platform
//! registration and typed SDK constructors do not belong in the engine dependency graph.
#![deny(missing_docs)]

extern crate mod_vocab as mod_api;

include!(concat!(env!("OUT_DIR"), "/game_records.rs"));
include!(concat!(env!("OUT_DIR"), "/game_catalog.rs"));

mod registry;
pub use registry::install;

/// The game's side of point dispatch, not a mod registration API.
#[doc(hidden)]
pub mod __private {
    pub use crate::registry::{POINTS, TABLE};
    pub use mod_api::__private::points::{ApiKind, PointHost, cold};

    /// Read one activity flag with no catalog scan or allocation.
    #[inline]
    pub fn active(place: crate::PointId) -> bool {
        TABLE.active(place as usize)
    }

    /// Dispatch through the one canonical native/wasm table.
    pub fn fire_api(place: crate::PointId, call: &mut dyn FnMut(ApiKind)) {
        TABLE.fire_api(place as usize, call);
    }
}
