use super::*;
use crate::game::native_entity_refs::tests::with_game;
use mod_api::{
    BlockEntityPart, BlockPart, CanPlaceItemEvent, Hooks, ITEMS, ModEntry, Mods, Registrar,
    RegistrationContext, STORAGE,
};
use sunrise_client::{
    chunk::Chunk,
    containers::{ModBlockEntity, ModContainerInsertRule},
};
use sunrise_core::position::ChunkPos;

const TARGET: Pos = (8, 64, 8);
const ABOVE: Pos = (8, 65, 8);

struct Filter;

#[mod_api::sunrise_part]
impl BlockPart for Filter {}

#[mod_api::sunrise_part]
impl BlockEntityPart for Filter {
    fn can_place_item(&self, event: &mut CanPlaceItemEvent<'_>) -> Option<bool> {
        match event.entity.data.first() {
            Some(1) => return Some(false),
            Some(2) => return None,
            _ => {}
        }
        match event.slot {
            0 => Some(false),
            1 => Some(true),
            2 => Some(event.entity.items[1].is_none() && event.stack.count() == 1),
            _ => None,
        }
    }
}

#[mod_api::__subscriptions]
impl Hooks for Filter {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        let filtered = r.declare_block_entity_type("filtered", Vec::new());
        r.block_entity_part(&filtered, Self).unwrap();
        r.declare_block_entity_type("plain", Vec::new());
    }
}

fn with_filter(test: impl FnOnce(&[ModEntry])) {
    let mods: Mods = vec![ModEntry::new("admission", Box::new(Filter))].into();
    mod_platform::testing::with_mods(mods.clone(), || test(&mods));
}

fn fixture(test: impl FnOnce(&mut Game<'_>)) {
    with_filter(|_| {
        with_game(|game| {
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            game.block_ticks
                .add_empty_chunk(ChunkPos::new(0, 0))
                .unwrap();
            let barrel = game
                .pack
                .assets
                .registry
                .default_state("minecraft:barrel")
                .unwrap();
            game.world
                .set(TARGET.0, TARGET.1, TARGET.2, barrel)
                .unwrap();
            game.world.block_entities.set_mod_containers(
                [(
                    barrel.0,
                    Arc::new(ModContainer {
                        slots: 3,
                        faces: std::array::from_fn(|_| vec![0, 1, 2]),
                        inputs: vec![0, 1, 2],
                        insert_rules: vec![
                            ModContainerInsertRule::Any,
                            ModContainerInsertRule::OnlyWhenEmpty,
                        ],
                        ..Default::default()
                    }),
                )]
                .into_iter()
                .collect(),
            );
            let mut entity = ModBlockEntity::new("admission:filtered".into(), Default::default());
            entity.items = vec![None; 3];
            game.world
                .block_entities
                .insert(TARGET, BlockEntity::Mod(Box::new(entity)));
            test(game);
        });
    });
}

fn source(game: &mut Game<'_>, block: &str, slots: usize) -> BlockStateId {
    let state = game
        .pack
        .assets
        .registry
        .resolve(&sunrise_world::chunk::BlockState {
            name: block.to_owned(),
            properties: [("facing".to_owned(), "down".to_owned())]
                .into_iter()
                .collect(),
        })
        .unwrap();
    game.world.set(ABOVE.0, ABOVE.1, ABOVE.2, state).unwrap();
    let mut items = vec![None; slots];
    items[0] = Some(ItemStack::new("minecraft:stone", 2));
    game.world
        .block_entities
        .insert(ABOVE, BlockEntity::Chest(items));
    state
}

fn target<'a>(game: &'a Game<'_>) -> &'a ModBlockEntity {
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&TARGET) else {
        panic!("expected mod block entity");
    };
    entity
}

#[test]
#[ignore = "daily"]
fn hopper_skips_a_refused_slot_and_preserves_the_stack_if_all_refuse() {
    fixture(|game| {
        source(game, "minecraft:hopper", HOPPER_SLOTS);
        game.tick_hopper(ABOVE);
        assert_eq!(
            target(game).items,
            [None, Some(ItemStack::new("minecraft:stone", 1)), None]
        );
        let BlockEntity::Mod(entity) = game.world.block_entities.get_mut(&TARGET).unwrap() else {
            unreachable!();
        };
        entity.data = vec![1];
        let before = (entity.items.clone(), entity.revision());
        let source_before = game
            .world
            .block_entities
            .get(&ABOVE)
            .unwrap()
            .slots()
            .to_vec();
        game.hoppers.get_mut(&ABOVE).unwrap().cooldown = 0;
        game.tick_hopper(ABOVE);
        assert_eq!(
            game.world.block_entities.get(&ABOVE).unwrap().slots(),
            source_before
        );
        assert_eq!(target(game).items, before.0);
        assert_eq!(target(game).revision(), before.1);
    });
}

#[test]
#[ignore = "daily"]
fn dropper_skips_a_refused_slot() {
    fixture(|game| {
        let state = source(game, "minecraft:dropper", 9);
        game.dispense_from(BlockPos::new(ABOVE.0, ABOVE.1, ABOVE.2), state)
            .unwrap();
        assert_eq!(
            target(game).items,
            [None, Some(ItemStack::new("minecraft:stone", 1)), None]
        );
        assert_eq!(
            game.world.block_entities.get(&ABOVE).unwrap().slots()[0],
            Some(ItemStack::new("minecraft:stone", 1))
        );
        assert!(game.dropped.is_empty());
    });
}

#[test]
#[ignore = "daily"]
fn default_items_insert_checks_admission_and_overrides_the_declared_rule() {
    fixture(|game| {
        let level = game.level("minecraft:overworld").unwrap();
        let at = [TARGET.0, TARGET.1, TARGET.2];
        let handler = level
            .capability(game, &ITEMS, at, Some(Direction::Up))
            .unwrap();
        let stack = ItemStack::new("minecraft:stone", 1);
        use super::super::api_values::TryIntoApi;
        let resource = ItemResource::of(&stack.try_into_api().unwrap());
        assert_eq!(
            mod_api::run_transaction(game, |host, tx| {
                let first = handler.insert_any(host, tx, &resource, 2);
                // Slot 1's declared OnlyWhenEmpty would refuse this subsequent insert.
                let second = handler.insert(host, tx, 1, &resource, 1);
                Ok::<_, ()>((first, second))
            }),
            Ok((2, 1))
        );
        assert_eq!(
            target(game).items,
            [None, Some(ItemStack::new("minecraft:stone", 3)), None]
        );
    });
}

#[test]
#[ignore = "daily"]
fn storage_rechecks_the_entity_after_an_accepted_write_in_the_same_attempt() {
    fixture(|game| {
        let level = game.level("minecraft:overworld").unwrap();
        let at = [TARGET.0, TARGET.1, TARGET.2];
        let handler = level
            .capability(game, &STORAGE, at, Some(Direction::Up))
            .unwrap();
        use super::super::api_values::TryIntoApi;
        let resource =
            ItemResource::of(&ItemStack::new("minecraft:stone", 1).try_into_api().unwrap());
        assert_eq!(
            mod_api::run_transaction(game, |host, tx| {
                Ok::<_, ()>(handler.insert(host, tx, &resource, 65))
            }),
            Ok(64)
        );
        // Slot 2 would take the remainder if the query still saw slot 1 empty.
        assert_eq!(
            target(game).items,
            [None, Some(ItemStack::new("minecraft:stone", 64)), None]
        );
    });
}

#[test]
#[ignore = "daily"]
fn loot_replace_block_uses_the_container_rule_for_each_slot() {
    fixture(|game| {
        game.build_registries();
        let result = |game: &mut Game<'_>, slot| {
            game.as_console(|game| game.run_command(&format!(
            r#"loot replace block 8 64 8 container.{slot} 1 loot {{"pools":[{{"rolls":1,"entries":[{{"type":"minecraft:item","name":"minecraft:stone"}}]}}]}}"#
        )))
        };
        result(game, 0);
        assert!(target(game).items.iter().all(Option::is_none));
        assert!(result(game, 1));
        assert_eq!(
            target(game).items,
            [None, Some(ItemStack::new("minecraft:stone", 1)), None]
        );
    });
}

#[test]
#[ignore = "daily"]
fn no_answer_falls_back_to_the_declared_insert_rule() {
    fixture(|game| {
        let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&TARGET) else {
            unreachable!();
        };
        entity.data = vec![2];
        let c = game.containers();
        let view = c.view(TARGET, false).unwrap();
        let offered = ItemStack::new("minecraft:stone", 1);
        assert!(c.can_place(&view, 1, &offered, &mut PlacementAttempt::default()));
        *game.slot_mut_at(&view, 1).unwrap() = Some(offered.clone());
        assert!(
            !game
                .containers()
                .can_place(&view, 1, &offered, &mut PlacementAttempt::default())
        );
    });
}

#[test]
#[cfg(not(any(feature = "system-allocator", feature = "alloc-census")))]
fn native_admission_allocates_only_when_preparing_subscribed_snapshots() {
    use std::hint::black_box;
    use sunrise_threads::allocation_count::measure;

    with_filter(|mods| {
        crate::game::api_values::install_parts(mods).unwrap();
        mod_api::__private::resolve_items(["minecraft:stone"]).unwrap();
        let mut stack = ItemStack::new("minecraft:stone", 2);
        stack
            .data_mut()
            .set_other("minecraft:max_stack_size", Some(Tag::Int(16)));
        let make_entity = |id: &str| {
            let mut entity = ModBlockEntity::new(id.into(), Default::default());
            entity.items = vec![None; 3];
            entity
        };
        // Even when there is a subscriber for another type, the ordinary container
        // doesn't convert the component-bearing offer or the entity's saved fields.
        let plain = make_entity("admission:plain");
        let mut attempt = PlacementAttempt::default();
        let (plain_answers, plain_allocs) =
            measure(|| [0, 1].map(|slot| black_box(attempt.ask((0, 64, 0), &plain, slot, &stack))));
        assert_eq!(plain_answers, [None, None]);
        assert_eq!(plain_allocs, 0);

        let mut filtered = make_entity("admission:filtered");
        let mut attempt = PlacementAttempt::default();
        let (first, setup_allocs) =
            measure(|| black_box(attempt.ask(TARGET, &filtered, 0, &stack)));
        assert_eq!(first, Some(false));
        assert!(
            setup_allocs > 0,
            "allocation counter must observe snapshot creation"
        );
        stack.count = 1;
        let (next, reuse_allocs) = measure(|| black_box(attempt.ask(TARGET, &filtered, 2, &stack)));
        assert_eq!(next, Some(true));
        assert_eq!(reuse_allocs, 0);

        *filtered.item_slot_mut(1).unwrap() = Some(ItemStack::new("minecraft:stone", 1));
        attempt.wrote_entity();
        let (refreshed, refresh_allocs) =
            measure(|| black_box(attempt.ask(TARGET, &filtered, 2, &stack)));
        assert_eq!(refreshed, Some(false));
        assert!(refresh_allocs > 0);
        println!(
            "allocations: unsubscribed pair={plain_allocs}, subscribed setup={setup_allocs}, next slot={reuse_allocs}, changed entity={refresh_allocs}"
        );
    });
}
