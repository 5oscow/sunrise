//! #2875: a mod's write to a player runs in the level the player is in (`Player.level()`),
//! whichever level's hook makes it.
use super::Dim;
use super::native_guest_turn_tests::with_guests;
use mod_api::{HostTransaction, ServerHost};
use sunrise_client::lan::playerdata;

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

/// A stack given from an Overworld hook to a player in the Nether whose inventory is full is
/// thrown where that player stands in the Nether, never at their coordinates in the
/// Overworld: for the world's owner and for another guest, given directly and inside a
/// transaction (whose overflow waits for the commit).
#[test]
#[ignore = "daily"]
fn a_full_inventory_in_another_level_overflows_where_the_player_is() {
    with_guests(|game| {
        game.level.player_uuid = Some(playerdata::uuid_ints(12));
        // A level is open while a player is in it.
        game.enter_level(Dim::Nether).unwrap();
        game.enter_level(Dim::Overworld).unwrap();
        let diamond = {
            let host: &dyn mod_api::Host = game;
            host.item_from_snbt(r#"{id:"minecraft:diamond",count:1}"#)
                .unwrap()
        };
        let spots = [(12, [40.5, 70.0, -12.5]), (13, [-30.5, 80.0, 22.5])];
        for (conn, at) in spots {
            game.lan
                .as_mut()
                .unwrap()
                .hub
                .update_guest(conn, |g| {
                    g.dimension = "minecraft:the_nether".to_owned();
                    g.position = at;
                    g.inventory.fill(Some(sunrise_client::items::ItemStack::new(
                        "minecraft:stone",
                        64,
                    )));
                })
                .unwrap();
        }
        let mut thrown = 0;
        for (conn, at) in spots {
            let player = game.player(u128::from(conn)).unwrap();
            for in_transaction in [false, true] {
                let case = format!("guest {conn}, in a transaction: {in_transaction}");
                let host: &mut dyn ServerHost = game;
                if in_transaction {
                    host.transaction(|host, _| {
                        player.give_stack(host, diamond.clone()).unwrap();
                        Ok::<_, ()>(())
                    })
                    .unwrap();
                } else {
                    player.give_stack(host, diamond.clone()).expect(&case);
                }
                thrown += 1;
                assert!(diamonds_in(game, Dim::Overworld).is_empty(), "{case}");
                let nether = diamonds_in(game, Dim::Nether);
                assert_eq!(nether.len(), thrown, "{case}");
                let last = nether.last().unwrap();
                let near = (0..3).all(|axis| (last[axis] - at[axis]).abs() < 2.0);
                assert!(near, "{case}: thrown at {last:?}, the player is at {at:?}");
            }
        }
    });
}
