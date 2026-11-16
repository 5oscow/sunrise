//! `LevelRef::play_sound` leaving one player out: the player handle is translated to the
//! connection inside the game.

use super::native_guest_turn_tests::with_guests;
use mod_api::SoundSource;

/// A sound queued with `except` names that player's connection; without it, no one.
#[test]
#[ignore = "daily"]
fn a_level_sound_leaves_out_the_player_asked() {
    with_guests(|game| {
        game.options.dedicated = Some(Vec::new());
        let player = game.player(13).unwrap();
        let level = game.level(game.dim.id()).unwrap();
        let at = [0.0, 64.0, 0.0];
        let sound = "minecraft:block.bell.use";
        let play = |game: &mut super::Game<'_>, except| {
            game.level_play_sound(level, sound, at, SoundSource::Blocks, 1.0, 1.0, except)
        };
        let queued = |game: &super::Game<'_>| game.creatures.lan_sounds.len();
        let before = queued(game);
        assert!(play(game, Some(player)));
        assert_eq!(game.creatures.lan_sounds.last().unwrap().except, Some(13));
        assert!(play(game, None));
        assert_eq!(game.creatures.lan_sounds.last().unwrap().except, None);
        assert_eq!(queued(game), before + 2);
        let gone = mod_api::__private::game::player_ref(
            mod_api::EntityRef::new(
                mod_api::WorldEpoch::new(player.entity().epoch().get() + 1),
                player.entity().id(),
            ),
            player.uuid(),
        );
        assert!(!play(game, Some(gone)), "a stale player queues nothing");
        assert_eq!(queued(game), before + 2);
    });
}

/// Both directions of a container transfer send the sound to the other guest, not its actor.
#[test]
#[ignore = "daily"]
fn fluid_sounds_leave_out_the_actor_but_reach_other_players() {
    with_guests(|game| {
        game.options.dedicated = Some(Vec::new());
        let player = game.player(13).unwrap();
        let level = game.level(game.dim.id()).unwrap();
        let water = mod_api::__private::fluid_key("minecraft:water").unwrap();
        let receivers: Vec<_> = [12, 13]
            .into_iter()
            .map(|conn| {
                let (tx, rx) = std::sync::mpsc::channel();
                game.lan
                    .as_mut()
                    .unwrap()
                    .hub
                    .update_guest(conn, |guest| {
                        guest.sender =
                            sunrise_client::lan::PlayerSender::new(tx, std::sync::Arc::default());
                    })
                    .unwrap();
                (conn, rx)
            })
            .collect();
        for (pickup, helper) in [(true, false), (false, false), (true, true), (false, true)] {
            if helper {
                let fluid = mod_api::FluidStack::new(water, 1000);
                mod_api::fluid_util::trigger_sound_and_game_event(
                    game,
                    &mod_api::FluidResource::of(&fluid),
                    level,
                    [3.5, 64.5, 5.5],
                    Some(player),
                    pickup,
                );
            } else {
                let seal = game.transaction_open();
                assert!(game.level_play_fluid_effects(
                    level,
                    [3, 64, 5],
                    water,
                    pickup,
                    Some(player),
                    seal.transaction(),
                ));
                game.transaction_close(&seal, true);
            }
            game.flush_native_sounds();
            for (conn, rx) in &receivers {
                if *conn == 13 {
                    assert!(rx.try_recv().is_err(), "the actor already played the sound");
                } else {
                    assert!(matches!(
                        rx.try_recv(),
                        Ok(sunrise_client::lan::Out::Packet(_))
                    ));
                    assert!(rx.try_recv().is_err(), "one sound for the other player");
                }
            }
        }
    });
}
