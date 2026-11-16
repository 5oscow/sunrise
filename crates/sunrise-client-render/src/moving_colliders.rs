//! The moving colliders solid to every mover of the level being ticked, as the game placed them
//! when that level's tick began (`Game::placed_moving_colliders`).
//!
//! Every physical move resolves through `SceneCollisions`, so this is the one place a collider
//! enters entity collision, whatever the mover (the player, a mob, an item, a vehicle). The
//! game runs on one thread per world (the server thread, the window), so the placed colliders
//! are that thread's own, and a [`Scope`] sets them for one level's tick only. Outside a scope,
//! and in a level with none, a query answers at once with nothing.
//!
//! The level's colliders are indexed by their world bounds in a coarse grid, so a mover far
//! from every collider pays one lookup however many the level holds.
use std::{cell::Cell, cell::RefCell, collections::HashMap};
use sunrise_core::geometry::{Aabb, Axis, Vec3, oriented::OrientedCollider};
use sunrise_world::entity_obstruction::EntityIdentity;

/// Side of an index cell, in blocks.
const CELL: f64 = 16.0;
/// A collider covering more cells than this is not stored per cell; every query tests it.
const BIG_CELLS: i64 = 64;

/// One collider entity, placed where it stood at the start of the pass.
pub struct Placed {
    pub collider: OrientedCollider,
    /// The collider's own entity and every entity in its vehicle stack: the root vehicle and
    /// each passenger at any depth, those the collider's entity rides with included. The mover
    /// whose identity is among them does not collide with it (`isPassengerOfSameVehicle`).
    pub exempt: Vec<u32>,
}

/// The level's colliders with their world bounds and the grid over them.
#[derive(Default)]
struct Index {
    placed: Vec<(Aabb, Placed)>,
    cells: HashMap<[i32; 3], Vec<u32>>,
    big: Vec<u32>,
    /// The box holding every collider.
    all: Option<Aabb>,
}

fn cells_of(b: Aabb) -> ([i64; 3], [i64; 3]) {
    let f = |v: f64| (v / CELL).floor() as i64;
    (
        Axis::ALL.map(|a| f(b.min(a))),
        Axis::ALL.map(|a| f(b.max(a))),
    )
}

impl Index {
    fn new(placed: Vec<Placed>) -> Self {
        let mut index = Self::default();
        for p in placed {
            let Some(bounds) = p.collider.world_bounds() else {
                continue;
            };
            let at = index.placed.len() as u32;
            index.all = Some(index.all.map_or(bounds, |all| all.union_bounds(bounds)));
            let (lo, hi) = cells_of(bounds);
            let count = (0..3).map(|i| hi[i] - lo[i] + 1).product::<i64>();
            if count > BIG_CELLS {
                index.big.push(at);
            } else {
                for x in lo[0]..=hi[0] {
                    for y in lo[1]..=hi[1] {
                        for z in lo[2]..=hi[2] {
                            index
                                .cells
                                .entry([x as i32, y as i32, z as i32])
                                .or_default()
                                .push(at);
                        }
                    }
                }
            }
            index.placed.push((bounds, p));
        }
        index
    }
}

thread_local! {
    static PLACED: RefCell<Index> = RefCell::new(Index::default());
    /// The entity whose move is being resolved, for the moves that do not say who they are.
    static MOVER: Cell<u64> = const { Cell::new(0) };
    static SCRATCH: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

/// The colliders placed for the level being ticked, for as long as the guard lives; the
/// colliders of the scope it replaced come back when it drops, so levels nest and nothing
/// outlives its tick. The level's tick holds one (like `block_contact::in_level`), and so
/// does a client's tick of the level it shows.
pub struct Scope {
    before: Index,
}

impl Scope {
    pub fn enter(placed: Vec<Placed>) -> Self {
        let index = Index::new(placed);
        Self {
            before: PLACED.with(|cell| std::mem::replace(&mut *cell.borrow_mut(), index)),
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

/// Runs `f` as the move of entity `id`: the collision entries that are not told which entity
/// moves take it from here, so the entity is never stopped by a collider of its own vehicle
/// stack. The move gate wraps every body move in this.
pub fn as_mover<R>(id: u32, f: impl FnOnce() -> R) -> R {
    let before = MOVER.with(|m| m.replace(u64::from(id)));
    let result = f();
    MOVER.with(|m| m.set(before));
    result
}

/// Appends to `out` the colliders meeting `bounds` that `entity` collides with. An identity of
/// 0 is the move [`as_mover`] names; with none named (a ray, a projectile's path) it is exempt
/// from nothing and meets every collider.
pub fn near(entity: EntityIdentity, bounds: Aabb, out: &mut Vec<OrientedCollider>) {
    PLACED.with(|cell| {
        let index = cell.borrow();
        let Some(all) = index.all else { return };
        // EntityGetter.getEntityCollisions expands its query by 1e-7.
        let area = bounds.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
        if !all.intersects(area) {
            return;
        }
        let mover = if entity.0 == 0 {
            MOVER.with(Cell::get)
        } else {
            entity.0
        };
        let mut found = SCRATCH.with(|s| std::mem::take(&mut *s.borrow_mut()));
        found.clear();
        let (lo, hi) = cells_of(area);
        let count = (0..3).map(|i| hi[i] - lo[i] + 1).product::<i64>();
        if count > index.cells.len() as i64 {
            found.extend(0..index.placed.len() as u32);
        } else {
            for x in lo[0]..=hi[0] {
                for y in lo[1]..=hi[1] {
                    for z in lo[2]..=hi[2] {
                        if let Some(list) = index.cells.get(&[x as i32, y as i32, z as i32]) {
                            found.extend_from_slice(list);
                        }
                    }
                }
            }
            found.extend_from_slice(&index.big);
        }
        found.sort_unstable();
        found.dedup();
        for &at in &found {
            let (world, p) = &index.placed[at as usize];
            if world.intersects(area)
                && (mover == 0 || !p.exempt.iter().any(|id| u64::from(*id) == mover))
            {
                out.push(p.collider.clone());
            }
        }
        SCRATCH.with(|s| *s.borrow_mut() = found);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use sunrise_core::geometry::oriented::{ColliderBoxes, Frame};

    fn block_at(x: f64, exempt: &[u32]) -> Placed {
        let boxes = ColliderBoxes::new(vec![Aabb::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 1.0),
        )]);
        Placed {
            collider: OrientedCollider::new(
                Frame::translation(Vec3::new(x, 0.0, 0.0)),
                Arc::new(boxes),
            ),
            exempt: exempt.to_vec(),
        }
    }

    fn found(entity: u64, around: f64) -> usize {
        let mut out = Vec::new();
        let area = Aabb::new(
            Vec3::new(around - 0.5, 0.0, 0.0),
            Vec3::new(around + 1.5, 1.0, 1.0),
        );
        near(EntityIdentity(entity), area, &mut out);
        out.len()
    }

    #[test]
    fn a_query_meets_the_colliders_near_it_and_not_those_exempt_or_far() {
        in_level(
            vec![
                block_at(0.0, &[7, 8]),
                block_at(100.0, &[]),
                block_at(2_000.0, &[]),
            ],
            || {
                assert_eq!(found(5, 0.0), 1, "an outsider meets the near one");
                assert_eq!(found(7, 0.0), 0, "the collider's own stack does not");
                assert_eq!(found(8, 0.0), 0);
                assert_eq!(found(7, 100.0), 1, "another collider is not exempt for it");
                assert_eq!(found(5, 50.0), 0, "nothing in reach");
                assert_eq!(found(0, 0.0), 1, "no mover named: exempt from nothing");
                as_mover(7, || {
                    assert_eq!(found(0, 0.0), 0, "the named mover is exempt")
                });
                assert_eq!(found(0, 0.0), 1, "the name ends with the move");
            },
        );
        assert_eq!(found(5, 0.0), 0, "outside a scope there is none");
    }

    #[test]
    fn scopes_nest_and_restore() {
        in_level(vec![block_at(0.0, &[])], || {
            in_level(Vec::new(), || assert_eq!(found(5, 0.0), 0));
            assert_eq!(found(5, 0.0), 1);
        });
    }
}
