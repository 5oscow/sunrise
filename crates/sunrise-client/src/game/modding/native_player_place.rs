//! A block a mod writes in a player's turn: `level_set_block` with a placement's flags, then `setPlacedBy`.
use super::{Game, native_server::PlayerRead};
use mod_api::{BlockUpdateFlags, LevelRef, PlayerRef, PlayerWorldRefusal as Refusal};
use sunrise_core::position::BlockPos;
use sunrise_world::registry::BlockStateId;

/// A placement's write flags: neighbours and clients told, shapes updated unless known.
fn placement_flags(update_neighbour_shapes: bool) -> BlockUpdateFlags {
    let flags = BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS;
    if update_neighbour_shapes {
        flags
    } else {
        flags | BlockUpdateFlags::KNOWN_SHAPE
    }
}

impl Game<'_> {
    /// The one block write (`level_set_block`, its block entity included) with a placement's
    /// flags, then `Block.setPlacedBy` in the player's turn; nothing else of `BlockItem.place`.
    pub(super) fn player_place_block(
        &mut self,
        player: PlayerRef,
        level: LevelRef,
        pos: [i32; 3],
        state: BlockStateId,
        update_neighbour_shapes: bool,
    ) -> Result<(), Refusal> {
        if !self.is_logical_server() {
            return Err(Refusal::Client);
        }
        let dead = match self.player_handle_read(player).ok_or(Refusal::Stale)? {
            PlayerRead::Acting { .. } => self
                .player
                .as_ref()
                .is_none_or(|p| p.health.health() <= 0.0),
            PlayerRead::Guest(g) => g.dead,
        };
        if dead {
            return Err(Refusal::Dead);
        }
        if self.pack.assets.registry.state(state).is_none() {
            return Err(Refusal::UnknownState);
        }
        if self.native_level_world(level).is_none() {
            return Err(Refusal::StaleLevel);
        }
        if level.dimension() != self.dim.key() {
            return Err(Refusal::OtherLevel);
        }
        let [x, y, z] = pos;
        if !self.world.shape.contains_y(y)
            || !(-30_000_000..30_000_000).contains(&x)
            || !(-30_000_000..30_000_000).contains(&z)
        {
            return Err(Refusal::OutOfWorld { pos });
        }
        if !self.world.loaded(x, z) {
            return Err(Refusal::NotLoaded { pos });
        }
        if self.world.block(x, y, z) == state {
            return Err(Refusal::Unchanged);
        }
        let flags = placement_flags(update_neighbour_shapes);
        // The write is `level_set_block`'s, on the level as it stands (a guest's turn is not
        // the authoritative level); only `setPlacedBy` runs in the player's turn.
        match self.level_set_block(level, pos, state, flags) {
            Ok(true) => {}
            Ok(false) => return Err(Refusal::Unchanged),
            Err(_) => return Err(Refusal::Stale),
        }
        let ran = self.native_as_player_handle(player, |game| game.set_placed_by(pos, state));
        if ran { Ok(()) } else { Err(Refusal::Stale) }
    }
    /// A chosen state written as a placement writes it, then `Block.setPlacedBy`, with no
    /// player: the write and the extra halves are those of `player_place_block`. `Ok(false)`
    /// when the write was refused or changed nothing.
    pub(super) fn level_place_block(
        &mut self,
        level: LevelRef,
        pos: [i32; 3],
        state: BlockStateId,
        update_neighbour_shapes: bool,
    ) -> Result<bool, mod_api::LevelWriteError> {
        let flags = placement_flags(update_neighbour_shapes);
        if !self.level_set_block(level, pos, state, flags)? {
            return Ok(false);
        }
        self.set_placed_by(pos, state);
        Ok(true)
    }
    /// `Block.setPlacedBy` of the block that now stands at `pos`, when it is `state`'s.
    fn set_placed_by(&mut self, pos: [i32; 3], state: BlockStateId) {
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        self.with_level(|b, level, ticks| b.set_placed_by_state(level, ticks, at, state));
        let _ = self.process_level_events();
    }
}
