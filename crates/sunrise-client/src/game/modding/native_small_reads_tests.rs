//! A client's `LevelRef::brightness`, `EntityRef::set_sprinting` and the server's
//! `tick_rate_is_frozen`.

use super::native_entity_refs::tests::with_game;
use super::native_guest_turn_tests::with_guests;
use super::{Game, execution_role::ExecutionRole};
use mod_api::LightLayer;

/// A client reads the light of the level it shows, only while logged in.
#[test]
#[ignore = "daily"]
fn a_client_reads_the_light_of_its_level() {
    use std::sync::Arc;
    use sunrise_client::chunk::{Chunk, Light};
    use sunrise_protocol::connection::Pipe;
    with_game(|game| {
        game.options.execution_role = ExecutionRole::Client;
        game.options.dedicated = None;
        let (client, _server) = Pipe::pair();
        game.connect_local(client, "BrightnessTest").unwrap();
        game.remote.as_mut().unwrap().logged_in = true;
        let mut chunk = Chunk::empty(0, 0);
        Arc::make_mut(&mut chunk.sections[0]).light = Light::Uniform(0xC5);
        game.world.chunks.insert((0, 0), Arc::new(chunk));
        let level = game.level("minecraft:overworld").unwrap();
        let lit = [3, -60, 5];
        assert_eq!(
            game.level_brightness(level, LightLayer::Block, lit),
            Some(5)
        );
        assert_eq!(game.level_brightness(level, LightLayer::Sky, lit), Some(12));
        assert_eq!(
            game.level_brightness(level, LightLayer::Sky, [16, -60, 0]),
            None
        );
        assert_eq!(
            game.level_can_see_sky(level, lit),
            None,
            "that read stays on the server"
        );
        game.remote.as_mut().unwrap().logged_in = false;
        assert_eq!(game.level_brightness(level, LightLayer::Block, lit), None);
    });
}

/// A player's sprinting flag is set and cleared, and a stale handle sets nothing.
#[test]
#[ignore = "daily"]
fn a_players_sprinting_is_set_and_cleared() {
    with_guests(|game: &mut Game<'_>| {
        let player = game.player(13).unwrap();
        let entity = player.entity();
        assert_eq!(game.entity_is_sprinting(entity), Some(false));
        assert!(game.entity_set_sprinting(entity, true));
        assert_eq!(game.entity_is_sprinting(entity), Some(true));
        assert!(game.entity_set_sprinting(entity, false));
        assert_eq!(game.entity_is_sprinting(entity), Some(false));
        let stale = mod_api::EntityRef::new(
            mod_api::WorldEpoch::new(entity.epoch().get() + 1),
            entity.id(),
        );
        assert!(!game.entity_set_sprinting(stale, true));
    });
}

/// The server's frozen state follows `/tick freeze`; a client has none.
#[test]
#[ignore = "daily"]
fn the_tick_rate_is_frozen_on_a_server_only() {
    with_game(|game| {
        game.options.dedicated = Some(Vec::new());
        game.clock.set_frozen(true);
        assert_eq!(game.tick_rate_is_frozen(), Some(true));
        game.clock.set_frozen(false);
        assert_eq!(game.tick_rate_is_frozen(), Some(false));
        game.options.execution_role = ExecutionRole::Client;
        game.options.dedicated = None;
        assert_eq!(game.tick_rate_is_frozen(), None);
    });
}
