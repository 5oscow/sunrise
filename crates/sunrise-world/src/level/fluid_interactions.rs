//! What a liquid block turns into beside another fluid or block (NeoForge's
//! `FluidInteractionRegistry`): vanilla's two lava rules and the entries mods declare, as one
//! resolved table per source fluid. Installed on [`Behaviours`] for the open world, resolved
//! once when the world opens and again on each data reload, so no name or tag is looked up
//! while a liquid updates.
use super::{Behaviours, data::BlockSet, states::BlockId};
use crate::{fluid::FluidKind, registry::BlockStateId};
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

/// A block, or every block of a tag.
#[derive(Clone, Debug)]
pub enum BlockMatch {
    Block(BlockId),
    Tag(Arc<BlockSet>),
}
impl BlockMatch {
    fn matches(&self, block: BlockId) -> bool {
        match self {
            Self::Block(known) => *known == block,
            Self::Tag(set) => set.contains(block),
        }
    }
}

/// What has to be beside the liquid block.
#[derive(Clone, Debug)]
pub enum Neighbour {
    /// A fluid, or every fluid of a tag, as the cell's fluid state says: a waterlogged block
    /// holds water. Source and flowing form are one fluid.
    Fluids(Arc<[FluidKind]>),
    Blocks(BlockMatch),
}

/// One interaction of a source fluid. A `None` result is no interaction for that state of the
/// source cell.
#[derive(Clone, Debug)]
pub struct FluidInteraction {
    pub neighbour: Neighbour,
    /// What the block under the source cell has to be.
    pub below: Option<BlockMatch>,
    /// What a source cell becomes.
    pub source: Option<BlockStateId>,
    /// What a flowing cell becomes.
    pub flowing: Option<BlockStateId>,
}

/// A cell beside the liquid block: its fluid state's fluid and its block.
#[derive(Clone, Copy, Debug)]
pub struct Cell {
    pub fluid: FluidKind,
    pub block: BlockId,
}

impl FluidInteraction {
    /// The block this interaction makes of the liquid cell, as its state says.
    fn result(&self, source_cell: bool) -> Option<BlockStateId> {
        if source_cell {
            self.source
        } else {
            self.flowing
        }
    }
    fn applies(&self, cell: Cell, below: BlockId) -> bool {
        let neighbour = match &self.neighbour {
            Neighbour::Fluids(fluids) => fluids.iter().any(|fluid| fluid.same_family(cell.fluid)),
            Neighbour::Blocks(blocks) => blocks.matches(cell.block),
        };
        neighbour && self.below.as_ref().is_none_or(|b| b.matches(below))
    }
}

/// What the liquid cell becomes: the result of the first interaction that applies and has one
/// for the cell's state, by neighbour (`neighbours`: above, north, south, west, east, never
/// below), then by interaction, as `FluidInteractionRegistry.canInteract` walks them.
pub fn first_match(
    interactions: &[FluidInteraction],
    neighbours: [Cell; 5],
    below: BlockId,
    source_cell: bool,
) -> Option<BlockStateId> {
    neighbours.into_iter().find_map(|cell| {
        interactions
            .iter()
            .filter(|it| it.applies(cell, below))
            .find_map(|it| it.result(source_cell))
    })
}

/// The interactions of every source fluid, in the order they were declared.
#[derive(Clone, Debug, Default)]
pub struct FluidInteractions {
    by_source: Vec<(FluidKind, Arc<[FluidInteraction]>)>,
}
impl FluidInteractions {
    /// Group `entries` by source fluid, keeping their order within each.
    pub fn new(entries: impl IntoIterator<Item = (FluidKind, FluidInteraction)>) -> Self {
        let mut grouped: Vec<(FluidKind, Vec<FluidInteraction>)> = Vec::new();
        for (source, interaction) in entries {
            match grouped.iter_mut().find(|(k, _)| k.same_family(source)) {
                Some((_, list)) => list.push(interaction),
                None => grouped.push((source, vec![interaction])),
            }
        }
        Self {
            by_source: grouped
                .into_iter()
                .map(|(source, list)| (source, list.into()))
                .collect(),
        }
    }
    /// The interactions of `kind`'s fluid; none for a fluid that has none.
    pub fn of(&self, kind: FluidKind) -> Option<&Arc<[FluidInteraction]>> {
        self.by_source
            .iter()
            .find(|(source, _)| source.same_family(kind))
            .map(|(_, list)| list)
    }
}

impl Behaviours {
    /// `LiquidBlock.shouldSpreadLiquid`'s two rules as the table's first entries. `water` is
    /// the fluids of the `minecraft:water` tag, which a data pack may edit.
    pub fn vanilla_fluid_interactions(
        &self,
        water: &[FluidKind],
    ) -> Vec<(FluidKind, FluidInteraction)> {
        let n = &self.named;
        let state = |block| Some(self.default_of(block));
        vec![
            (
                FluidKind::Lava,
                FluidInteraction {
                    neighbour: Neighbour::Fluids(water.into()),
                    below: None,
                    source: state(n.obsidian),
                    flowing: state(n.cobblestone),
                },
            ),
            (
                FluidKind::Lava,
                FluidInteraction {
                    neighbour: Neighbour::Blocks(BlockMatch::Block(n.blue_ice)),
                    below: Some(BlockMatch::Block(n.soul_soil)),
                    source: state(n.basalt),
                    flowing: state(n.basalt),
                },
            ),
        ]
    }
    /// The fluids of the `minecraft:water` fluid tag the prepared data gives.
    pub fn water_fluids(&self) -> Vec<FluidKind> {
        FluidKind::ALL
            .into_iter()
            .filter(|kind| kind.mask() & self.fluid_tags.water != 0)
            .collect()
    }
    /// The table of the prepared data alone, which a world without data packs or mods keeps.
    pub(super) fn install_vanilla_fluid_interactions(&self) {
        self.set_fluid_interactions(FluidInteractions::new(
            self.vanilla_fluid_interactions(&self.water_fluids()),
        ));
    }
    /// Install the open world's table, replacing the last load's.
    pub fn set_fluid_interactions(&self, table: FluidInteractions) {
        let slot = &self.fluid_interactions;
        let has = |filter: fn(FluidKind) -> bool| {
            table.by_source.iter().any(|(source, _)| filter(*source))
        };
        slot.vanilla.store(
            table
                .by_source
                .iter()
                .fold(0, |mask, (source, _)| mask | source.mask()),
            Ordering::Release,
        );
        slot.mods.store(
            has(|source| source.mod_fluid().is_some()),
            Ordering::Release,
        );
        *slot
            .table
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Arc::new(table);
    }
    /// The interactions of `kind`'s fluid, taken once per update: none for a fluid without any,
    /// which answers from two atomic loads, with no lock.
    pub(crate) fn fluid_interactions(&self, kind: FluidKind) -> Option<Arc<[FluidInteraction]>> {
        let slot = &self.fluid_interactions;
        let may_have = match kind.mod_fluid() {
            Some(_) => slot.mods.load(Ordering::Acquire),
            None => kind.mask() & slot.vanilla.load(Ordering::Acquire) != 0,
        };
        if !may_have {
            return None;
        }
        slot.table
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .of(kind)
            .cloned()
    }
}

/// The open world's table on [`Behaviours`]: replaced whole by each data load. Which fluids
/// have entries is also kept as atomics (vanilla kinds by mask, mod fluids as one flag), so a
/// fluid without any is told apart without taking the lock.
#[derive(Default)]
pub(super) struct Slot {
    table: RwLock<Arc<FluidInteractions>>,
    vanilla: AtomicU8,
    mods: AtomicBool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const AIR: BlockId = BlockId(0);
    const OBSIDIAN: BlockStateId = BlockStateId(10);
    const COBBLESTONE: BlockStateId = BlockStateId(11);
    const BASALT: BlockStateId = BlockStateId(12);
    const MOD_STONE: BlockStateId = BlockStateId(20);
    const BLUE_ICE: BlockId = BlockId(5);
    const SOUL_SOIL: BlockId = BlockId(6);
    const SLAB: BlockId = BlockId(7);
    const STONE: BlockId = BlockId(8);

    fn cell(fluid: FluidKind, block: BlockId) -> Cell {
        Cell { fluid, block }
    }
    fn around(cells: &[(usize, Cell)]) -> [Cell; 5] {
        let mut all = [cell(FluidKind::Empty, AIR); 5];
        for (at, c) in cells {
            all[*at] = *c;
        }
        all
    }
    fn vanilla() -> Vec<FluidInteraction> {
        vec![
            FluidInteraction {
                neighbour: Neighbour::Fluids([FluidKind::Water, FluidKind::FlowingWater].into()),
                below: None,
                source: Some(OBSIDIAN),
                flowing: Some(COBBLESTONE),
            },
            FluidInteraction {
                neighbour: Neighbour::Blocks(BlockMatch::Block(BLUE_ICE)),
                below: Some(BlockMatch::Block(SOUL_SOIL)),
                source: Some(BASALT),
                flowing: Some(BASALT),
            },
        ]
    }
    fn made(
        rules: &[FluidInteraction],
        cells: [Cell; 5],
        below: BlockId,
        source: bool,
    ) -> Option<u32> {
        first_match(rules, cells, below, source).map(|state| state.0)
    }

    #[test]
    fn water_makes_obsidian_of_a_source_and_cobblestone_of_flowing_lava() {
        let rules = vanilla();
        let water = around(&[(3, cell(FluidKind::FlowingWater, AIR))]);
        assert_eq!(made(&rules, water, STONE, true), Some(OBSIDIAN.0));
        assert_eq!(made(&rules, water, STONE, false), Some(COBBLESTONE.0));
    }

    #[test]
    fn a_waterlogged_neighbour_is_water() {
        let waterlogged = around(&[(1, cell(FluidKind::Water, SLAB))]);
        assert_eq!(made(&vanilla(), waterlogged, STONE, true), Some(OBSIDIAN.0));
    }

    #[test]
    fn blue_ice_beside_lava_over_soul_soil_makes_basalt_and_nothing_else_does() {
        let rules = vanilla();
        let ice = around(&[(4, cell(FluidKind::Empty, BLUE_ICE))]);
        assert_eq!(made(&rules, ice, SOUL_SOIL, true), Some(BASALT.0));
        assert_eq!(made(&rules, ice, SOUL_SOIL, false), Some(BASALT.0));
        assert_eq!(
            made(&rules, ice, STONE, true),
            None,
            "needs soul soil below"
        );
        let stone = around(&[(0, cell(FluidKind::Empty, STONE))]);
        assert_eq!(made(&rules, stone, SOUL_SOIL, true), None);
        // The cell below is never a neighbour: only the five others are passed.
        assert_eq!(made(&rules, around(&[]), SOUL_SOIL, true), None);
    }

    #[test]
    fn the_first_neighbour_then_the_first_entry_wins() {
        let rules = vanilla();
        // Water to the east and blue ice above, over soul soil: above comes first.
        let both = around(&[
            (0, cell(FluidKind::Empty, BLUE_ICE)),
            (4, cell(FluidKind::Water, AIR)),
        ]);
        assert_eq!(made(&rules, both, SOUL_SOIL, true), Some(BASALT.0));
        // The same neighbour matching two entries gives the first entry's result.
        let ice_in_water = around(&[(2, cell(FluidKind::Water, BLUE_ICE))]);
        assert_eq!(
            made(&rules, ice_in_water, SOUL_SOIL, true),
            Some(OBSIDIAN.0)
        );
    }

    #[test]
    fn a_mod_entry_matches_a_block_tag_and_follows_vanillas() {
        let mut stones = BlockSet::with_capacity(16);
        stones.insert(STONE);
        let mut rules = vanilla();
        rules.push(FluidInteraction {
            neighbour: Neighbour::Blocks(BlockMatch::Tag(Arc::new(stones))),
            below: None,
            source: Some(MOD_STONE),
            flowing: None,
        });
        let beside = around(&[(1, cell(FluidKind::Empty, STONE))]);
        assert_eq!(made(&rules, beside, AIR, true), Some(MOD_STONE.0));
        assert_eq!(
            made(&rules, beside, AIR, false),
            None,
            "a None result is no interaction"
        );
        // ... and does not stop a later entry from answering.
        rules.push(FluidInteraction {
            neighbour: Neighbour::Blocks(BlockMatch::Block(STONE)),
            below: None,
            source: None,
            flowing: Some(COBBLESTONE),
        });
        assert_eq!(made(&rules, beside, AIR, false), Some(COBBLESTONE.0));
        let water = around(&[(1, cell(FluidKind::Water, STONE))]);
        assert_eq!(made(&rules, water, AIR, true), Some(OBSIDIAN.0));
    }

    #[test]
    fn the_table_is_per_source_fluid_and_an_unlisted_fluid_has_none() {
        let entries = vanilla()
            .into_iter()
            .map(|it| (FluidKind::Lava, it))
            .chain([(
                FluidKind::Water,
                FluidInteraction {
                    neighbour: Neighbour::Blocks(BlockMatch::Block(STONE)),
                    below: None,
                    source: Some(MOD_STONE),
                    flowing: Some(MOD_STONE),
                },
            )]);
        let table = FluidInteractions::new(entries);
        assert_eq!(table.of(FluidKind::FlowingLava).map(|l| l.len()), Some(2));
        assert_eq!(table.of(FluidKind::Lava).map(|l| l.len()), Some(2));
        assert_eq!(table.of(FluidKind::FlowingWater).map(|l| l.len()), Some(1));
        assert!(table.of(FluidKind::Empty).is_none());
        assert!(FluidInteractions::default().of(FluidKind::Lava).is_none());
    }
}
