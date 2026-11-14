//! The game-owned canonical table, shared by native and wasm registrations.
use mod_api::__private::points::Table;
use std::sync::atomic::AtomicBool;

/// Install this game's actual catalog and the lower sites' checked global index map.
pub fn install() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        // SAFETY: LOWER_PLACES is compile-time checked against both vocabularies' variants,
        // and the lower sites and SDK import the same canonical lower record definitions.
        unsafe { horizon_engine_slots::install_points(&TABLE, &crate::LOWER_PLACES) }
            .expect("installing the game's point provider");
        horizon_engine_slots::install_catalog(&CATALOG)
            .expect("installing the game's point catalog");
    });
}

/// The one activity array for the game's complete point catalog.
pub static POINTS: [AtomicBool; crate::POINT_COUNT] =
    [const { AtomicBool::new(false) }; crate::POINT_COUNT];

/// The one registration table, including the original unstaged-target refusals.
pub static TABLE: Table = if crate::STAGED {
    Table::new(&POINTS)
} else {
    Table::without_sites(&POINTS, crate::POINT_TARGETS)
};

static CATALOG: horizon_engine_slots::Catalog = horizon_engine_slots::Catalog {
    table: &TABLE,
    layout: crate::POINT_LAYOUT,
    places: crate::CATALOG_PLACES,
    refusals: crate::API_REFUSALS,
};
