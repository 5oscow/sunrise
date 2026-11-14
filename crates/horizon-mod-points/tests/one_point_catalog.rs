//! Site and SDK records are one canonical Rust type; numeric catalogs are checked at compile time.
const _: fn(horizon_game_points::TickArgs) -> mod_points::TickArgs = |args| args;

#[test]
fn sdk_and_site_catalogs_have_the_same_identity() {
    assert_eq!(mod_points::POINT_LAYOUT, horizon_game_points::POINT_LAYOUT);
    assert_eq!(mod_points::POINT_COUNT, horizon_game_points::POINT_COUNT);
    assert_eq!(
        mod_points::PointId::Tick as usize,
        horizon_game_points::PointId::Tick as usize
    );
}
