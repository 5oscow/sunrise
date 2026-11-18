//! #2875: what a mod's write sends into a player's surroundings (a stack thrown from their eyes,
//! a sound at them) lands in the level the player is in (`Player.level()`), whichever level's
//! hook makes it.
use super::Dim;
use super::native_guest_turn_tests::with_guests;
use mod_api::{HostTransaction, ItemPlace, ServerHost};
use sunrise_client::{items::ItemStack, lan::playerdata};

/// Where the diamonds lying in `dim`'s level are.
fn diamonds_in(game: &mut super::Game<'_>, dim: Dim) -> Vec<[f64; 3]> {
    let back = game.dim;
    game.enter_level(dim).unwrap();
    let found = game
        .dropped
        .iter()
        .filter(|e| &*e.stack.item == "minecraft:diamond")
        .map(|e| [e.position.x, e.position.y, e.position.z])
        .collect();
    game.enter_level(back).unwrap();
    found
}

/// Guests 12 (the world's owner) and 13 in the Nether, at `SPOTS`, with full inventories;
/// the Overworld in place, as for an Overworld hook.
fn in_the_nether(game: &mut super::Game<'_>) -> mod_api::ItemStack {
    game.level.player_uuid = Some(playerdata::uuid_ints(12));
    // A level is open while a player is in it.
    game.enter_level(Dim::Nether).unwrap();
    game.enter_level(Dim::Overworld).unwrap();
    for (conn, at) in SPOTS {
        game.lan
            .as_mut()
            .unwrap()
            .hub
            .update_guest(conn, |g| {
                g.dimension = "minecraft:the_nether".to_owned();
                g.position = at;
                g.inventory
                    .fill(Some(ItemStack::new("minecraft:stone", 64)));
            })
            .unwrap();
    }
    let host: &dyn mod_api::Host = game;
    host.item_from_snbt(r#"{id:"minecraft:diamond",count:1}"#)
        .unwrap()
}

const SPOTS: [(u32, [f64; 3]); 2] = [(12, [40.5, 70.0, -12.5]), (13, [-30.5, 80.0, 22.5])];

/// The last diamond thrown in the Nether lies at `at`, the `thrown`th there, and none in the
/// Overworld, which is still the level in place.
fn thrown_at(game: &mut super::Game<'_>, at: [f64; 3], thrown: usize, case: &str) {
    assert_eq!(
        game.dim,
        Dim::Overworld,
        "{case}: the level in place is back"
    );
    assert!(diamonds_in(game, Dim::Overworld).is_empty(), "{case}");
    let nether = diamonds_in(game, Dim::Nether);
    assert_eq!(nether.len(), thrown, "{case}");
    let last = nether.last().unwrap();
    let near = (0..3).all(|axis| (last[axis] - at[axis]).abs() < 2.0);
    assert!(near, "{case}: thrown at {last:?}, the player is at {at:?}");
}

/// A stack given from an Overworld hook to a player in the Nether whose inventory is full is
/// thrown where that player stands in the Nether, never at their coordinates in the
/// Overworld: for the world's owner and for another guest, given directly, inside a
/// transaction (whose overflow waits for the commit), and put into a hotbar slot through
/// `ItemPlace::PlayerSlot` in a committed transaction.
#[test]
#[ignore = "daily"]
fn a_full_inventory_in_another_level_overflows_where_the_player_is() {
    with_guests(|game| {
        let diamond = in_the_nether(game);
        let mut thrown = 0;
        for (conn, at) in SPOTS {
            let player = game.player(u128::from(conn)).unwrap();
            let resource = mod_api::ItemResource::of(&diamond);
            for way in ["directly", "in a transaction", "into a slot"] {
                let case = format!("guest {conn}, {way}");
                let host: &mut dyn ServerHost = game;
                match way {
                    "directly" => player.give_stack(host, diamond.clone()).expect(&case),
                    "in a transaction" => host
                        .transaction(|host, _| {
                            player.give_stack(host, diamond.clone()).unwrap();
                            Ok::<_, ()>(())
                        })
                        .unwrap(),
                    _ => host
                        .transaction(|host, tx| {
                            let slot = ItemPlace::PlayerSlot {
                                player: u128::from(conn),
                                slot: 0,
                            };
                            assert_eq!(slot.insert(host, tx, &resource, 1), 1);
                            Ok::<_, ()>(())
                        })
                        .unwrap(),
                }
                thrown += 1;
                thrown_at(game, at, thrown, &case);
            }
        }
    });
}

/// A stack a mod throws from the eyes of a player in another level lands at them in their
/// level. Inside a transaction, whose level stays in place, the throw can't reach theirs: the
/// stack comes back untouched and nothing lands anywhere.
#[test]
#[ignore = "daily"]
fn a_throw_for_a_player_in_another_level_lands_there_or_comes_back() {
    with_guests(|game| {
        let diamond = in_the_nether(game);
        let (conn, at) = SPOTS[1];
        let player = game.player(u128::from(conn)).unwrap();
        let host: &mut dyn ServerHost = game;
        let back = host
            .transaction(|host, _| Ok::<_, ()>(player.drop_stack(host, diamond.clone(), false)))
            .unwrap();
        assert_eq!(back.unwrap(), Some(diamond.clone()));
        assert!(diamonds_in(game, Dim::Overworld).is_empty());
        assert!(diamonds_in(game, Dim::Nether).is_empty());
        let host: &mut dyn ServerHost = game;
        assert_eq!(player.drop_stack(host, diamond, false).unwrap(), None);
        thrown_at(game, at, 1, "outside a transaction");
    });
}

/// A panic in a turn run in a player's own level still puts the level in place back and ends
/// the guest's turn before it propagates.
#[test]
#[ignore = "daily"]
fn a_panic_in_a_turn_in_another_level_puts_the_level_back() {
    with_guests(|game| {
        in_the_nether(game);
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            game.as_guest_in_their_level(13, Dim::Nether, |game| {
                assert_eq!(game.dim, Dim::Nether);
                panic!("a listener's panic");
            })
        }));
        assert!(unwound.is_err());
        assert_eq!(game.dim, Dim::Overworld);
        assert!(game.lan_guest_swaps.is_empty());
        assert!(game.as_guest_in_their_level(13, Dim::Nether, |game| {
            assert_eq!(game.dim, Dim::Nether);
        }));
        assert_eq!(game.dim, Dim::Overworld);
    });
}
