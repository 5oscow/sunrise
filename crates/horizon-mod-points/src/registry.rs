//! SDK registration into the game's one canonical point table.
pub use horizon_game_points::__private::{POINTS, TABLE};
#[cfg(not(target_arch = "wasm32"))]
pub use mod_api::__private::points::register;
pub use mod_api::__private::points::{RegisterError, stop};

/// Seal the existing native/wasm registry after installing the actual game catalog.
pub fn seal() {
    horizon_game_points::install();
    mod_api::__private::points::seal();
}

impl crate::PointId {
    /// This SDK place in the game's one table; generated const checks pin its numeric identity.
    pub const fn place(self) -> mod_api::__private::points::Place {
        mod_api::__private::points::Place::new(&TABLE, self as usize)
    }
}
