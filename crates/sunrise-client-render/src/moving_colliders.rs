//! The moving colliders solid to every mover of the level being ticked, as the game placed them
//! when that level's tick began (`Game::placed_moving_colliders`).
//!
//! Every physical move resolves through `SceneCollisions`, so this is the one place a collider
//! enters entity collision, whatever the mover (the player, a mob, an item, a vehicle). The
//! game runs on one thread per world (the server thread, the window), so the placed colliders
//! are that thread's own, and a [`Scope`] sets them for one level's tick only. Outside a scope,
//! and in a level with none, a query answers at once with nothing.
use std::cell::RefCell;
use sunrise_core::geometry::{Aabb, Vec3, oriented::OrientedCollider};
use sunrise_world::entity_obstruction::EntityIdentity;

/// One collider entity, placed where it stood at the start of the pass.
pub struct Placed {
    pub collider: OrientedCollider,
    /// The collider's own entity and every entity riding it, directly or not: the mover
    /// whose identity is among them does not collide with it (`isPassengerOfSameVehicle`).
    pub exempt: Vec<u32>,
}

thread_local! {
    static PLACED: RefCell<Vec<Placed>> = const { RefCell::new(Vec::new()) };
}

/// The colliders placed for the level being ticked, for as long as the guard lives; the
/// colliders of the scope it replaced come back when it drops, so levels nest and nothing
/// outlives its tick. The level's tick holds one (like `block_contact::in_level`), and so
/// does a client's tick of the level it shows.
pub struct Scope {
    before: Vec<Placed>,
}

impl Scope {
    pub fn enter(placed: Vec<Placed>) -> Self {
        Self {
            before: PLACED.with(|cell| std::mem::replace(&mut *cell.borrow_mut(), placed)),
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        PLACED.with(|cell| *cell.borrow_mut() = std::mem::take(&mut self.before));
    }
}

/// Runs `f` with `placed` as the colliders of the level it ticks.
pub fn in_level<R>(placed: Vec<Placed>, f: impl FnOnce() -> R) -> R {
    let _scope = Scope::enter(placed);
    f()
}

/// The colliders meeting `bounds` that `entity` collides with. An identity of 0 is a mover
/// that does not say who it is, and is exempt from none.
pub fn near(entity: EntityIdentity, bounds: Aabb) -> Vec<OrientedCollider> {
    PLACED.with(|cell| {
        let placed = cell.borrow();
        if placed.is_empty() {
            return Vec::new();
        }
        // EntityGetter.getEntityCollisions expands its query by 1e-7.
        let area = bounds.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
        placed
            .iter()
            .filter(|p| entity.0 == 0 || !p.exempt.iter().any(|id| u64::from(*id) == entity.0))
            .filter(|p| {
                p.collider
                    .world_bounds()
                    .is_some_and(|b| b.intersects(area))
            })
            .map(|p| p.collider.clone())
            .collect()
    })
}
