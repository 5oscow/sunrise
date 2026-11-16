//! Where a portal leads, asked through a level handle (`LevelRef::portal_destination`,
//! `LevelRef::find_or_create_portal_exit`). Both read the level in place and the parked
//! levels only: a level that is not open, or a chunk the search reads that is not loaded,
//! is `ChunksNotLoaded` (vanilla loads them), and nothing is read from the save. The search is
//! the one dimension travel runs, over a side of it (`PortalSide`).
use super::Game;
use crate::game::{
    Dim,
    end::END_SPAWN,
    nether_portal::{Plan, PortalSide},
};
use mod_api::{LevelRef, PortalError, PortalExit, PortalShape};
use sunrise_client::containers::BlockEntity;
use sunrise_core::{geometry::Vec3, position::BlockPos};
use sunrise_world::level::portal_travel::FoundRectangle;

/// What a probe of a portal block found.
enum Probe {
    /// An End portal or gateway: the answer.
    Fixed(Dim, Vec3),
    /// A nether portal: the plan, and the exit portal when one stands.
    Nether(Plan, Option<FoundRectangle>),
}

impl Game<'_> {
    fn portal_error(error: mod_api::LevelReadError) -> PortalError {
        match error {
            mod_api::LevelReadError::Stale => PortalError::LevelMissing,
            _ => PortalError::Unavailable,
        }
    }

    /// Every chunk the search of `radius` around `at` reads is loaded in `side`.
    fn portal_chunks_loaded(side: &PortalSide<'_>, at: BlockPos, radius: i32) -> bool {
        PortalSide::search_chunks(at.x >> 4, at.z >> 4, radius)
            .all(|chunk| side.world.chunks.contains_key(&chunk))
    }

    /// The portal block at `pos` of `level` and what it leads to, found without changing
    /// anything.
    fn probe_portal(
        &self,
        level: LevelRef,
        pos: [i32; 3],
        size: [f64; 2],
    ) -> Result<Probe, PortalError> {
        self.native_level_world_result(level)
            .map_err(Self::portal_error)?;
        let source = self
            .portal_side(Dim::from_id(level.dimension_id()))
            .ok_or(PortalError::LevelMissing)?;
        let [x, y, z] = pos;
        if !source.world.is_loaded_at(x, y, z) {
            return Err(PortalError::ChunksNotLoaded);
        }
        let state = source.world.block(x, y, z);
        let name = &*self.res.facts().get(state).name;
        if self.res.behaviours().portal_axis(state).is_some() {
            let entry = BlockPos::new(x, y, z);
            let at = Vec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5);
            let plan =
                self.portal_plan_from(&source, entry, at, size[0] as f32, size[1] as f32, true);
            if !self.may_enter(plan.dim) {
                return Err(PortalError::EnteringNotAllowed);
            }
            let target = self
                .portal_side(plan.dim)
                .filter(|target| {
                    Self::portal_chunks_loaded(
                        target,
                        plan.approximate,
                        PortalSide::search_radius(plan.dim == Dim::Nether),
                    )
                })
                .ok_or(PortalError::ChunksNotLoaded)?;
            let found = self.find_portal_in(&target, None, &plan).map(|(_, r)| r);
            return Ok(Probe::Nether(plan, found));
        }
        match name {
            "minecraft:end_portal" => self.probe_end_portal(&source),
            "minecraft:end_gateway" => self.probe_end_gateway(&source, pos),
            _ => Err(PortalError::NotAPortal),
        }
    }

    /// `EndPortalBlock.getPortalDestination` for an entity other than a player, without the
    /// platform: the End's spawn point, or out of the End the Overworld's shared spawn.
    fn probe_end_portal(&self, source: &PortalSide<'_>) -> Result<Probe, PortalError> {
        if source.dim != Dim::End {
            let [x, y, z] = END_SPAWN;
            self.portal_side(Dim::End)
                .ok_or(PortalError::ChunksNotLoaded)?;
            return Ok(Probe::Fixed(
                Dim::End,
                Vec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5),
            ));
        }
        let [x, _, z] = self.level.world_spawn.unwrap_or_else(|| {
            self.spawn.map_or([0, 64, 0], |s| {
                [s.x.floor() as i32, s.y.floor() as i32, s.z.floor() as i32]
            })
        });
        let overworld = self
            .portal_side(Dim::Overworld)
            .filter(|side| side.world.is_loaded_at(x, 64, z))
            .ok_or(PortalError::ChunksNotLoaded)?;
        let y = self.motion_top_in(overworld.world, Dim::Overworld, x, z);
        Ok(Probe::Fixed(
            Dim::Overworld,
            Vec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5),
        ))
    }

    /// `TheEndGatewayBlockEntity.getPortalPosition` of a gateway that already has its exit.
    fn probe_end_gateway(
        &self,
        source: &PortalSide<'_>,
        pos: [i32; 3],
    ) -> Result<Probe, PortalError> {
        let Some(BlockEntity::EndGateway(gateway)) =
            source.world.block_entities.get(&(pos[0], pos[1], pos[2]))
        else {
            return Err(PortalError::NotAPortal);
        };
        let exit = gateway.exit_portal.ok_or(PortalError::NoExit)?;
        let target = if gateway.exact_teleport {
            exit
        } else {
            let (cx, cz) = (exit[0] >> 4, exit[2] >> 4);
            let loaded = (cz - 1..=cz + 1)
                .all(|z| (cx - 1..=cx + 1).all(|x| source.world.chunks.contains_key(&(x, z))));
            if !loaded {
                return Err(PortalError::ChunksNotLoaded);
            }
            let p = self.tallest_block_in(source.world, [exit[0], exit[1] + 2, exit[2]], 5, false);
            [p[0], p[1] + 1, p[2]]
        };
        Ok(Probe::Fixed(
            source.dim,
            sunrise_client::end_gateway::bottom_center(target),
        ))
    }

    /// The answer for the destination `exit` of `plan` in the loaded level of `plan.dim`.
    fn nether_exit(&self, level: LevelRef, plan: &Plan, exit: FoundRectangle) -> PortalExit {
        let side = self.in_place_or_parked(plan.dim);
        let (at, yaw_turn) = self.arrival_in(&side, plan, exit);
        let corner = exit.min_corner;
        let x_axis = self
            .res
            .behaviours()
            .portal_axis(side.world.block(corner.x, corner.y, corner.z))
            .unwrap_or(true);
        let shape = PortalShape::from(mod_api::__private::built::PortalShape {
            x_axis,
            bottom_left: [corner.x, corner.y, corner.z],
            width: exit.axis1_size,
            height: exit.axis2_size,
        });
        PortalExit::new(
            self.level_ref(level.epoch(), plan.dim.key()),
            [at.x, at.y, at.z],
            yaw_turn,
            Some(shape),
        )
    }

    fn in_place_or_parked(&self, dim: Dim) -> PortalSide<'_> {
        self.portal_side(dim)
            .expect("the side was loaded when the plan was made")
    }

    fn fixed_exit(&self, level: LevelRef, dim: Dim, at: Vec3) -> PortalExit {
        PortalExit::new(
            self.level_ref(level.epoch(), dim.key()),
            [at.x, at.y, at.z],
            0.0,
            None,
        )
    }

    /// Where an entity of `size` arrives through the portal at `portal_pos`, changing nothing
    /// (`getPortalDestination`). The direction the entity faces does not move it: vanilla
    /// places the exit from the two portals' axes alone.
    pub(super) fn level_portal_destination(
        &self,
        level: LevelRef,
        portal_pos: [i32; 3],
        _facing: mod_api::Direction,
        size: [f64; 2],
    ) -> Result<PortalExit, PortalError> {
        match self.probe_portal(level, portal_pos, size)? {
            Probe::Fixed(dim, at) => Ok(self.fixed_exit(level, dim, at)),
            Probe::Nether(plan, Some(exit)) => Ok(self.nether_exit(level, &plan, exit)),
            Probe::Nether(_, None) => Err(PortalError::NoExit),
        }
    }

    /// [`Self::level_portal_destination`], building the exit portal when none stands
    /// (`PortalForcer.createPortal`) and telling `portal_create` as travel does.
    pub(super) fn level_find_or_create_portal_exit(
        &mut self,
        level: LevelRef,
        portal_pos: [i32; 3],
        facing: mod_api::Direction,
        size: [f64; 2],
    ) -> Result<PortalExit, PortalError> {
        if !self.transactions.is_empty() {
            return Err(PortalError::InTransaction);
        }
        let plan = match self.probe_portal(level, portal_pos, size)? {
            Probe::Nether(plan, None) => plan,
            _ => return self.level_portal_destination(level, portal_pos, facing, size),
        };
        // Building works on the level in place: the destination goes in place for it, and the
        // level that was in place comes back.
        let source = self.dim;
        self.enter_level(plan.dim)
            .map_err(|_| PortalError::LevelMissing)?;
        let built = self.create_exit_portal(&plan);
        let answer = built.map(|exit| self.nether_exit(level, &plan, exit));
        if let Err(e) = self.enter_level(source) {
            log::error!("could not reopen {}: {e}", source.id());
        }
        answer.ok_or(PortalError::NoExit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::native_entity_refs::tests::with_game;
    use mod_api::Direction;
    use sunrise_client::chunk::Chunk;

    const SIZE: [f64; 2] = [0.98, 0.98];
    const ENTRY: [i32; 3] = [8, 70, 8];

    fn nether_portal_blocks(game: &Game<'_>) -> usize {
        let b = game.res.behaviours();
        let side = game.portal_side(Dim::Nether).expect("the Nether is open");
        side.world
            .chunks
            .values()
            .map(|c| c.blocks_where(|s| b.portal_axis(s).is_some()).len())
            .sum()
    }

    #[test]
    #[ignore = "daily"]
    fn a_portal_is_asked_where_it_leads_and_find_or_create_builds_its_exit() {
        with_game(|game| {
            game.build_registries();
            game.set_allow_commands(true);
            game.options.dedicated = Some(Vec::new());
            game.native_lifecycle.world_open_ready().unwrap();
            game.world
                .chunks
                .insert((0, 0), std::sync::Arc::new(Chunk::empty(0, 0)));
            let portal = game
                .pack
                .assets
                .registry
                .default_state("minecraft:nether_portal")
                .unwrap();
            game.world
                .set(ENTRY[0], ENTRY[1], ENTRY[2], portal)
                .unwrap();
            let overworld = game.level("minecraft:overworld").unwrap();
            let ask = |game: &Game<'_>| {
                game.level_portal_destination(overworld, ENTRY, Direction::North, SIZE)
            };
            // The Nether is not open.
            assert_eq!(ask(game), Err(PortalError::ChunksNotLoaded));
            // Open with none of the chunks the search reads.
            game.enter_level(Dim::Nether).unwrap();
            game.enter_level(Dim::Overworld).unwrap();
            assert_eq!(ask(game), Err(PortalError::ChunksNotLoaded));
            // Its chunks loaded, nothing stands there: the read finds none and builds none.
            game.enter_level(Dim::Nether).unwrap();
            for z in -3..=3 {
                for x in -3..=3 {
                    game.streaming
                        .insert_remote(&mut game.world, Chunk::empty(x, z));
                }
            }
            game.enter_level(Dim::Overworld).unwrap();
            assert_eq!(ask(game), Err(PortalError::NoExit));
            assert_eq!(nether_portal_blocks(game), 0);
            // A block that is no portal.
            assert_eq!(
                game.level_portal_destination(overworld, [9, 70, 9], Direction::North, SIZE),
                Err(PortalError::NotAPortal)
            );
            // Not inside a transaction.
            let seal = game.transaction_open();
            assert_eq!(
                game.level_find_or_create_portal_exit(overworld, ENTRY, Direction::North, SIZE),
                Err(PortalError::InTransaction)
            );
            game.transaction_close(&seal, false);
            // Find or create builds one, and the read then finds it.
            let built = game
                .level_find_or_create_portal_exit(overworld, ENTRY, Direction::North, SIZE)
                .expect("the exit is built");
            assert_eq!(game.dim, Dim::Overworld);
            assert!(nether_portal_blocks(game) > 0);
            assert_eq!(built.level, game.level("minecraft:the_nether").unwrap());
            assert!(built.exit_portal.is_some());
            assert_eq!(ask(game), Ok(built));
        });
    }
}
