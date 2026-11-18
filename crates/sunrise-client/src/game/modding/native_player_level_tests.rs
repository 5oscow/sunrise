//! #2875: a mod's write to a player in another level than the hook's reaches their inventory at
//! once, and nothing it would set down lands in the world: not in the hook's level, where the
//! player isn't, and not in theirs, which isn't in place.
use super::Dim;
use super::native_guest_turn_tests::with_guests;
use mod_api::{HostTransaction, ItemPlace, ServerHost};
use sunrise_client::{items::ItemStack, lan::playerdata};

const NETHER: &str = "minecraft:the_nether";

/// How many item entities lie in each of the Overworld and the Nether.
fn dropped(game: &mut super::Game<'_>) -> [usize; 2] {
    let here = game.dim;
    let overworld = game.dropped.len();
    game.enter_level(Dim::Nether).unwrap();
    let nether = game.dropped.len();
    game.enter_level(here).unwrap();
    [overworld, nether]
}

/// Guests 12 (the world's owner) and 13 in the Nether with full inventories but for slot 8;
/// the Overworld in place, as for an Overworld hook. A diamond to give.
fn in_the_nether(game: &mut super::Game<'_>) -> mod_api::ItemStack {
    game.level.player_uuid = Some(playerdata::uuid_ints(12));
    // A level is open while a player is in it.
    game.enter_level(Dim::Nether).unwrap();
    game.enter_level(Dim::Overworld).unwrap();
    for conn in [12, 13] {
        game.lan
            .as_mut()
            .unwrap()
            .hub
            .update_guest(conn, |g| {
                g.dimension = NETHER.to_owned();
                g.inventory
                    .fill(Some(ItemStack::new("minecraft:stone", 64)));
                g.inventory[8] = None;
            })
            .unwrap();
    }
    let host: &dyn mod_api::Host = game;
    host.item_from_snbt(r#"{id:"minecraft:diamond",count:1}"#)
        .unwrap()
}

fn diamonds(game: &super::Game<'_>, conn: u32) -> u32 {
    let guest = game.lan.as_ref().unwrap().hub.guest(conn).unwrap();
    let held = guest.inventory.iter().flatten();
    held.filter(|s| &*s.item == "minecraft:diamond")
        .map(|s| s.count)
        .sum()
}

/// A stack given from an Overworld hook to a player in the Nether goes into their inventory
/// at once, directly and inside a committed transaction, for the owner and another guest. What
/// doesn't fit comes back to the mod, and an item-place insert takes only what fits; nothing
/// drops in either level.
#[test]
#[ignore = "daily"]
fn a_give_to_a_player_in_another_level_fills_their_inventory_and_returns_the_rest() {
    with_guests(|game| {
        let diamond = in_the_nether(game);
        let resource = mod_api::ItemResource::of(&diamond);
        let before = dropped(game);
        for conn in [12, 13] {
            let player = game.player(u128::from(conn)).unwrap();
            let case = format!("guest {conn}");
            // Slot 8 takes a diamond and then 63 more.
            let host: &mut dyn ServerHost = game;
            assert_eq!(player.give_stack(host, diamond.clone()), Ok(None), "{case}");
            let given = host
                .transaction(|host, _| Ok::<_, ()>(player.give_stack(host, diamond.clone())))
                .unwrap();
            assert_eq!(given, Ok(None), "{case}");
            assert_eq!(diamonds(game, conn), 2, "{case}");
            game.lan
                .as_mut()
                .unwrap()
                .hub
                .update_guest(conn, |g| {
                    g.inventory[8] = Some(ItemStack::new("minecraft:stone", 64))
                })
                .unwrap();
            let host: &mut dyn ServerHost = game;
            assert_eq!(
                player.give_stack(host, diamond.clone()),
                Ok(Some(diamond.clone())),
                "{case}: full"
            );
            let taken = host
                .transaction(|host, tx| {
                    let slot = ItemPlace::PlayerSlot {
                        player: u128::from(conn),
                        slot: 0,
                    };
                    Ok::<_, ()>(slot.insert(host, tx, &resource, 1))
                })
                .unwrap();
            assert_eq!(taken, 0, "{case}: a full slot takes nothing");
            assert_eq!(diamonds(game, conn), 0, "{case}");
            assert_eq!(dropped(game), before, "{case}: nothing dropped anywhere");
            assert_eq!(game.dim, Dim::Overworld);
        }
    });
}

/// A guest borrowed into an Overworld turn while they are in the Nether (as a harvest query
/// borrows the requested player) gets the same: inside their own turn a full inventory's
/// overflow comes back and nothing drops.
#[test]
#[ignore = "daily"]
fn a_player_borrowed_into_another_levels_turn_drops_nothing_there() {
    with_guests(|game| {
        let diamond = in_the_nether(game);
        game.lan
            .as_mut()
            .unwrap()
            .hub
            .update_guest(13, |g| {
                g.inventory[8] = Some(ItemStack::new("minecraft:stone", 64))
            })
            .unwrap();
        let player = game.player(13).unwrap();
        let before = dropped(game);
        let (given, thrown) = game
            .as_guest(13, |game| {
                let host: &mut dyn ServerHost = game;
                let given = player.give_stack(host, diamond.clone());
                let thrown = player.drop_stack(host, diamond.clone(), true);
                (given, thrown)
            })
            .unwrap();
        assert_eq!(given, Ok(Some(diamond.clone())));
        assert_eq!(thrown, Ok(Some(diamond)));
        assert_eq!(dropped(game), before);
    });
}

/// A throw or a break for a player in another level changes nothing: the stack comes back, and
/// the same coordinates keep their block in both levels.
#[test]
#[ignore = "daily"]
fn a_throw_or_break_for_a_player_in_another_level_changes_nothing() {
    with_guests(|game| {
        let diamond = in_the_nether(game);
        let pos = [6, 64, 4];
        let stone = game
            .pack
            .assets
            .registry
            .default_state("minecraft:stone")
            .unwrap();
        let netherrack = game
            .pack
            .assets
            .registry
            .default_state("minecraft:netherrack")
            .unwrap();
        for (dim, state) in [(Dim::Nether, netherrack), (Dim::Overworld, stone)] {
            game.enter_level(dim).unwrap();
            game.world.chunks.insert(
                (0, 0),
                std::sync::Arc::new(sunrise_client::chunk::Chunk::empty(0, 0)),
            );
            game.world.set(pos[0], pos[1], pos[2], state).unwrap();
        }
        let player = game.player(13).unwrap();
        let before = dropped(game);
        let host: &mut dyn ServerHost = game;
        assert_eq!(
            player.drop_stack(host, diamond.clone(), true),
            Ok(Some(diamond))
        );
        assert!(!player.destroy_block(host, pos, false));
        assert_eq!(dropped(game), before);
        assert_eq!(game.world.block(pos[0], pos[1], pos[2]), stone);
        game.enter_level(Dim::Nether).unwrap();
        assert_eq!(game.world.block(pos[0], pos[1], pos[2]), netherrack);
    });
}

/// A give to a guest in another level who has a menu open keeps the menu: the inventory write
/// isn't tied to a level.
#[test]
#[ignore = "daily"]
fn a_give_to_a_player_in_another_level_keeps_their_open_menu() {
    use sunrise_core::nbt::Tag;
    with_guests(|game| {
        let diamond = in_the_nether(game);
        let window = game
            .lan
            .as_mut()
            .unwrap()
            .hub
            .open_native_menu(13, 5, Tag::String("menu".into()), vec![None; 45])
            .unwrap();
        let player = game.player(13).unwrap();
        let host: &mut dyn ServerHost = game;
        assert_eq!(player.give_stack(host, diamond), Ok(None));
        assert_eq!(diamonds(game, 13), 1);
        let guest = game.lan.as_ref().unwrap().hub.guest(13).unwrap();
        assert_eq!(guest.menu.as_ref().map(|m| m.window), Some(window));
    });
}
