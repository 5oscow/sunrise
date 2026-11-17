//! Real authoritative menu transfers: a take grants XP at the boundary, can refill a
//! result before repetition, and rolls back both transfer and reward on failure.
use crate::game::native_guest_turn_tests::with_guests;
use mod_api::{
    Hooks, ItemPlace, ItemResource, MenuPart, MenuSlotTakeEvent, ModEntry, Mods, Registrar,
};
use std::{cell::RefCell, sync::Arc};
use sunrise_client::{
    chunk::Chunk,
    containers::{BlockEntity, ModBlockEntity, ModBlockEntityFields, ModContainer, ModMenuType},
    items::ItemStack,
    lan::menu::Slots,
    menu::{Input, Kind, Menu},
};
use sunrise_protocol::packets::play::ContainerClick;

const MENU: &str = "minecraft:generic_9x3";
const POS: (i32, i32, i32) = (1, 64, 1);
const RESULT: usize = 2;

#[derive(Clone, Copy, Default)]
enum Script {
    #[default]
    Reward,
    Refill,
    Fail,
    Stale,
    Handled,
    Deny,
    SwapLimit,
}
thread_local! {
    static SCRIPT: RefCell<Script> = const { RefCell::new(Script::Reward) };
    static TAKES: RefCell<Vec<(usize, u32, u32)>> = const { RefCell::new(Vec::new()) };
}
struct Part;
#[mod_api::sunrise_part]
impl MenuPart for Part {
    fn menu_slot_rules(&self, _: &dyn mod_api::Host, event: &mut mod_api::MenuSlotRulesEvent<'_>) {
        SCRIPT.with(|script| match *script.borrow() {
            Script::Deny => event.may_pickup = false,
            Script::SwapLimit if event.slot == 0 => event.max_stack = 1,
            _ => {}
        });
    }

    fn menu_slot_take(
        &self,
        host: &mut dyn mod_api::ServerHost,
        event: &mut MenuSlotTakeEvent<'_>,
    ) {
        assert_eq!(event.context.player, Some(12));
        assert_eq!(event.context.pos, Some([1, 64, 1]));
        assert_eq!(event.container, mod_api::MenuContainer::Main);
        let player = host.players().acting_player().expect("the clicking player");
        assert!(player.give_experience_points(host, event.stack.count()));
        let first = TAKES.with(|takes| {
            let mut takes = takes.borrow_mut();
            takes.push((event.slot, event.stack.count(), event.quick_craft_amount));
            takes.len() == 1
        });
        match SCRIPT.with(|script| *script.borrow()) {
            Script::Reward | Script::Handled | Script::Deny | Script::SwapLimit => {}
            Script::Refill if first => {
                let level = host.level("minecraft:overworld").unwrap();
                let place = ItemPlace::BlockEntity {
                    level,
                    pos: [1, 64, 1],
                    side: None,
                    slot: RESULT,
                };
                let tx = host.transaction_current().expect("inside the click");
                assert_eq!(
                    place.insert(host, &tx, &ItemResource::of(event.stack), 3),
                    3
                );
            }
            Script::Fail => panic!("take listener failure"),
            Script::Stale => {
                let tx = host.transaction_current().unwrap();
                let place = ItemPlace::PlayerSlot {
                    player: 12,
                    slot: 20,
                };
                assert_eq!(
                    place.insert(host, &tx, &ItemResource::of(event.stack), 1),
                    1
                );
            }
            Script::Refill => {}
        }
    }
}
struct QuickMove;
#[mod_api::sunrise_part]
impl mod_api::BlockPart for QuickMove {}
#[mod_api::sunrise_part]
impl mod_api::BlockEntityPart for QuickMove {
    fn menu_quick_move(
        &self,
        _host: &mut dyn mod_api::ServerHost,
        event: &mut mod_api::MenuQuickMoveEvent<'_>,
    ) -> mod_api::Flow {
        if !SCRIPT.with(|script| matches!(*script.borrow(), Script::Handled)) {
            return mod_api::Flow::Continue;
        }
        let mut moved = event.stack.clone();
        moved.set_count(1).unwrap();
        event.entity.items[0] = Some(moved);
        event.stack.set_count(1).unwrap();
        mod_api::Flow::Cancel
    }
}
struct TestMod;
#[mod_api::__subscriptions]
impl Hooks for TestMod {
    fn register(&self, registrar: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        registrar.menu_part(MENU, Part);
        let kind = registrar.block_entity_type("machine").blocks(&[]).build();
        registrar.block_entity_part(&kind, QuickMove).unwrap();
    }
}

fn with_machine(
    script: Script,
    test: impl FnOnce(&mut crate::game::Game<'_>, &'static ModMenuType),
) {
    SCRIPT.with(|s| *s.borrow_mut() = script);
    TAKES.with(|s| s.borrow_mut().clear());
    let mods: Mods = vec![ModEntry::new("take_test", Box::new(TestMod))].into();
    crate::game::api_values::install_parts(&mods).unwrap();
    mod_platform::testing::with_mods(mods, || {
        with_guests(|game| {
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            let state = game
                .pack
                .assets
                .registry
                .default_state("minecraft:stone")
                .unwrap();
            game.world.set(POS.0, POS.1, POS.2, state).unwrap();
            let container = Arc::new(ModContainer {
                slots: 27,
                ..Default::default()
            });
            let state = game.world.block(POS.0, POS.1, POS.2).0;
            game.world
                .block_entities
                .set_mod_containers([(state, container)].into_iter().collect());
            let mut items = vec![None; 27];
            items[RESULT] = Some(ItemStack::new("minecraft:stone", 2));
            let fields = ModBlockEntityFields {
                items: items.clone(),
                ..Default::default()
            };
            game.world.block_entities.insert(
                POS,
                BlockEntity::Mod(Box::new(ModBlockEntity::new(
                    "take_test:machine".into(),
                    fields,
                ))),
            );
            let kind = Box::leak(Box::new(ModMenuType {
                id: MENU,
                slots: 27,
                data_slots: 0,
                results: vec![RESULT],
                crafting: None,
                links: None,
            }));
            let host = game.lan.as_mut().unwrap();
            host.hub.open_container(
                12,
                MENU,
                sunrise_core::nbt::Tag::String("machine".into()),
                POS,
                Slots::Block(POS),
                items,
                None,
            );
            host.hub.guests.update(0, |guest| {
                guest.menu.as_mut().unwrap().mod_type = Some(kind);
                guest.position = [1.5, 64.0, 2.5];
            });
            test(game, kind);
        })
    });
}

fn click(game: &mut crate::game::Game<'_>, slot: usize, button: i8, input: i32) {
    let open = game.lan.as_ref().unwrap().hub.guests[0]
        .menu
        .as_ref()
        .unwrap();
    let packet = ContainerClick {
        container: open.window,
        state: open.state,
        slot: slot as i16,
        button,
        input,
        changed: Vec::new(),
        carried: None,
    };
    assert!(game.lan_machine_click(12, &packet));
}
fn output(game: &crate::game::Game<'_>) -> u32 {
    game.world.block_entities.get(&POS).unwrap().slots()[RESULT]
        .as_ref()
        .map_or(0, |stack| stack.count)
}
fn takes() -> Vec<(usize, u32, u32)> {
    TAKES.with(|s| s.borrow().clone())
}

#[test]
#[ignore = "daily"]
fn mod_slot_takes_resume_before_repetition_and_refuse_failed_or_stale_rewards() {
    // Primary/half pickup, merge into the cursor, number-key swap, throw and pick-all.
    for (button, input, cursor, expected) in [
        (0, 0, 0, 2),
        (1, 0, 0, 1),
        (0, 0, 1, 2),
        (0, 2, 0, 2),
        (0, 4, 0, 1),
        (1, 4, 0, 2),
        (0, 6, 1, 2),
    ] {
        with_machine(Script::Reward, |game, _| {
            if cursor > 0 {
                game.lan.as_mut().unwrap().hub.guests.update(0, |g| {
                    g.carried = Some(ItemStack::new("minecraft:stone", cursor))
                });
            }
            let slot = if input == 6 { 0 } else { RESULT };
            click(game, slot, button, input);
            assert_eq!(
                takes(),
                [(RESULT, expected, 0)],
                "input {input}, button {button}"
            );
            assert_eq!(output(game), 2 - expected);
            assert_eq!(
                game.lan.as_ref().unwrap().hub.guests[0].xp_total,
                expected as i32
            );
        });
    }
    with_machine(Script::Refill, |game, _| {
        click(game, RESULT, 0, 1);
        assert_eq!(takes(), [(RESULT, 2, 2), (RESULT, 3, 3)]);
        assert_eq!(output(game), 0);
        let guest = &game.lan.as_ref().unwrap().hub.guests[0];
        assert_eq!(
            guest
                .inventory
                .iter()
                .flatten()
                .map(|stack| stack.count)
                .sum::<u32>(),
            5
        );
        assert_eq!(guest.xp_total, 5);
    });
    with_machine(Script::Handled, |game, _| {
        let open = game.lan.as_ref().unwrap().hub.guests[0]
            .menu
            .as_ref()
            .unwrap();
        let packet = ContainerClick {
            container: open.window,
            state: open.state,
            slot: RESULT as i16,
            button: 0,
            input: 1,
            changed: Vec::new(),
            carried: None,
        };
        assert!(game.lan_mod_quick_move(12, &packet));
        assert_eq!(takes(), [(RESULT, 1, 1)]);
        assert_eq!(output(game), 1);
        let guest = &game.lan.as_ref().unwrap().hub.guests[0];
        assert!(guest.inventory.iter().all(Option::is_none));
        assert_eq!(
            game.world.block_entities.get(&POS).unwrap().slots()[0]
                .as_ref()
                .unwrap()
                .count,
            1
        );
        assert_eq!(guest.xp_total, 1);
    });
    for script in [Script::Fail, Script::Stale] {
        with_machine(script, |game, _| {
            click(game, RESULT, 0, 0);
            assert_eq!(output(game), 2);
            let guest = &game.lan.as_ref().unwrap().hub.guests[0];
            assert_eq!(guest.xp_total, 0);
            assert!(guest.carried.is_none());
            assert!(guest.inventory.iter().all(Option::is_none));
        });
    }
    with_machine(Script::Reward, |game, _| {
        game.lan.as_mut().unwrap().hub.guests.update(0, |guest| {
            guest
                .inventory
                .fill(Some(ItemStack::new("minecraft:dirt", 64)));
            guest.inventory[35] = Some(ItemStack::new("minecraft:stone", 63));
        });
        click(game, RESULT, 0, 1);
        assert_eq!(takes(), [(RESULT, 1, 1)]);
        assert_eq!(output(game), 1);
        assert_eq!(game.lan.as_ref().unwrap().hub.guests[0].xp_total, 1);
    });
    with_machine(Script::Reward, |game, _| {
        game.lan
            .as_mut()
            .unwrap()
            .hub
            .guests
            .update(0, |guest| guest.game_mode = 1);
        click(game, RESULT, 0, 3);
        assert_eq!(output(game), 2);
        assert!(takes().is_empty());
        assert_eq!(
            game.lan.as_ref().unwrap().hub.guests[0]
                .carried
                .as_ref()
                .unwrap()
                .count,
            64
        );
    });
    with_machine(Script::Reward, |game, _| {
        if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&POS) {
            entity.items[0] = Some(ItemStack::new("minecraft:stone", 2));
        }
        game.lan.as_mut().unwrap().hub.guests.update(0, |guest| {
            guest.carried = Some(ItemStack::new("minecraft:dirt", 1));
        });
        click(game, 0, 0, 0);
        assert!(takes().is_empty());
        assert_eq!(
            game.lan.as_ref().unwrap().hub.guests[0]
                .carried
                .as_ref()
                .unwrap()
                .count,
            2
        );
    });
    with_machine(Script::Deny, |game, _| {
        click(game, RESULT, 0, 0);
        assert_eq!(output(game), 2);
        assert!(takes().is_empty());
    });
    with_machine(Script::Reward, |game, _| {
        game.lan.as_mut().unwrap().hub.guests.update(0, |guest| {
            guest
                .inventory
                .fill(Some(ItemStack::new("minecraft:dirt", 64)));
        });
        click(game, RESULT, 0, 1);
        assert_eq!(output(game), 2);
        assert!(takes().is_empty());
    });
    for (script, source_count, expected) in [(Script::Reward, 1, 2), (Script::SwapLimit, 2, 2)] {
        with_machine(script, |game, _| {
            if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&POS) {
                entity.items[0] = Some(ItemStack::new("minecraft:stone", 2));
            }
            game.lan.as_mut().unwrap().hub.guests.update(0, |guest| {
                guest.inventory[0] = Some(ItemStack::new("minecraft:dirt", source_count));
            });
            click(game, 0, 0, 2);
            assert_eq!(takes(), [(0, expected, 0)]);
            assert_eq!(
                game.lan.as_ref().unwrap().hub.guests[0].xp_total,
                expected as i32
            );
        });
    }
    // A client's prediction remains data-only, even with the same registered menu part.
    with_machine(Script::Reward, |game, kind| {
        let mut prediction = Menu::new(Kind::Mod(kind));
        prediction.set_registry_id(Some(MENU));
        prediction.load_slot(RESULT, Some(ItemStack::new("minecraft:stone", 2)));
        prediction
            .click(&game.menu_env(), RESULT as i32, 0, Input::Pickup)
            .unwrap();
        assert_eq!(prediction.carried.as_ref().unwrap().count, 2);
        assert!(takes().is_empty());
    });
}
