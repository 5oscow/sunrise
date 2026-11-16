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
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    sync::{Arc, OnceLock},
};
use sunrise_core::geometry::{Aabb, Axis, Vec3, oriented::OrientedCollider};
use sunrise_world::entity_obstruction::EntityIdentity;

/// Side of an index cell, in blocks.
const CELL: f64 = 16.0;
/// A collider covering more cells than this is not stored per cell; every query tests it.
const BIG_CELLS: i64 = 64;

/// One collider entity, placed where it stood at the start of the pass.
pub struct Placed {
    /// The collider's entity.
    pub id: u32,
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

/// A level's placed colliders, indexed once and shared: the game keeps one per level and
/// builds a new one only when a collider, its place or its vehicle stack changed.
#[derive(Clone)]
pub struct Prepared(Arc<Index>);

impl Prepared {
    pub fn new(placed: Vec<Placed>) -> Self {
        Self(Arc::new(Index::new(placed)))
    }

    /// How many colliders the level holds.
    pub fn len(&self) -> usize {
        self.0.placed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.placed.is_empty()
    }

    /// The entities exempt from the `at`th collider.
    pub fn exempt(&self, at: usize) -> Option<&[u32]> {
        self.0.placed.get(at).map(|(_, p)| p.exempt.as_slice())
    }

    /// The colliders, in the order they were placed.
    pub fn placed(&self) -> impl Iterator<Item = &Placed> {
        self.0.placed.iter().map(|(_, p)| p)
    }

    /// No collider at all, without building anything.
    pub fn empty() -> Self {
        static EMPTY: OnceLock<Arc<Index>> = OnceLock::new();
        Self(EMPTY.get_or_init(Arc::default).clone())
    }
}

thread_local! {
    static PLACED: RefCell<Arc<Index>> = RefCell::new(Prepared::empty().0);
    /// The entity whose move is being resolved, for the moves that do not say who they are.
    static MOVER: Cell<u64> = const { Cell::new(0) };
    static SCRATCH: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

/// The colliders placed for the level being ticked, for as long as the guard lives; the
/// colliders of the scope it replaced come back when it drops, so levels nest and nothing
/// outlives its tick. The level's tick holds one (like `block_contact::in_level`), and so
/// does a client's tick of the level it shows.
pub struct Scope {
    before: Arc<Index>,
}

impl Scope {
    pub fn enter(placed: Prepared) -> Self {
        Self {
            before: PLACED.with(|cell| std::mem::replace(&mut *cell.borrow_mut(), placed.0)),
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let before = std::mem::replace(&mut self.before, Prepared::empty().0);
        PLACED.with(|cell| *cell.borrow_mut() = before);
    }
}

/// Runs `f` with `placed` as the colliders of the level it ticks.
pub fn in_level<R>(placed: Prepared, f: impl FnOnce() -> R) -> R {
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

/// Whether a collider box `entity` collides with overlaps `bounds` (the thin box at a body's
/// eyes, for the in-wall damage a block's suffocation gives).
pub fn overlaps(entity: EntityIdentity, bounds: Aabb) -> bool {
    let mut found = Vec::new();
    near(entity, bounds, &mut found);
    found.iter().any(|collider| collider.intersects(bounds))
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
            id: 1,
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
            Prepared::new(vec![
                block_at(0.0, &[7, 8]),
                block_at(100.0, &[]),
                block_at(2_000.0, &[]),
            ]),
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

    /// A thin box at a body's eyes inside a collider box is in it; outside, or in its own
    /// stack's, it is not.
    #[test]
    fn an_eye_inside_a_box_is_in_the_wall_unless_it_is_the_stacks_own() {
        let eye = Aabb::new(Vec3::new(0.5, 0.5, 0.5), Vec3::new(0.7, 0.500_001, 0.7));
        let outside = Aabb::new(Vec3::new(5.0, 0.5, 0.5), Vec3::new(5.2, 0.500_001, 0.7));
        in_level(Prepared::new(vec![block_at(0.0, &[7])]), || {
            assert!(overlaps(EntityIdentity(5), eye));
            assert!(!overlaps(EntityIdentity(5), outside));
            assert!(!overlaps(EntityIdentity(7), eye));
        });
    }

    #[test]
    fn scopes_nest_and_restore() {
        in_level(Prepared::new(vec![block_at(0.0, &[])]), || {
            in_level(Prepared::empty(), || assert_eq!(found(5, 0.0), 0));
            assert_eq!(found(5, 0.0), 1);
        });
    }
}
