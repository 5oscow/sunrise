//! The living travel gate (#2414 PR 2): the door the `living_travel` hook
//! fires through, at the two boundaries the maintainer ruling names - the
//! player's `Player.travel` branch selection (`MotionWorld::player_travel`)
//! and the mob's `LivingEntity.travel` entry (`Mob::travel`). A travel that
//! runs there asks this gate first; with no listener, or on a client's own
//! prediction, the resolved input and velocity stand and the tick cost is
//! one branch.
use horizon_world::native_load_context::LoopContext;
use mod_api::TravelMode;

/// The `living_travel` hook's target (#2414): the generated dispatch the
/// catalog's generator requires of a target, with the loop's own reader as
/// the context. Its declaration carries `isolate`, so the same generated
/// walk is the one the travel gate dispatches through - no alias, no second
/// route.
pub fn native_living_travel(
    context: &LoopContext,
    entity: u32,
    kind: &str,
    mode: TravelMode,
    input: &mut [f64; 3],
    velocity: &mut [f64; 3],
    canceled: &mut bool,
) {
    let reader = horizon_client_render::api_context::LoopReader(context);
    mod_api::dispatch_living_travel(
        &reader.context(),
        entity,
        kind,
        mode,
        input,
        velocity,
        canceled,
    );
}
