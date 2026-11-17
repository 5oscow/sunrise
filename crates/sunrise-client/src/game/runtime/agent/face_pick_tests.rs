//! The agent's turns toward a point pick again: `attack ID` and `look-at entity ID` read the
//! stored crosshair target (client_pick.rs) in the same command that turned the view.
use super::*;
use crate::game::native_client_session::tests::logged_client;
use crate::game::native_entity_refs::tests::with_client_game;
use std::sync::{Arc, mpsc};
use sunrise_client::{chunk::Chunk, motion::PlayerMotion};
use sunrise_core::geometry::Vec3;
use sunrise_protocol::connection::Connection;
use sunrise_protocol::packets::play::{self, Clientbound as C};

struct NoLoop;
impl EventLoopCtl for NoLoop {
    fn exit(&self) {}
    fn set_control_flow(&self, _: winit::event_loop::ControlFlow) {}
}

/// The server's side of the handshake up to play, as `client_pick`'s tests drive it.
fn finish_login(server: &mut Connection) {
    use sunrise_protocol::packets::{Empty, configuration as cfg, login};
    server.recv().unwrap(); // Intention.
    server.recv().unwrap(); // Hello.
    server
        .send(
            &login::Clientbound::LoginFinished(login::LoginFinished {
                profile: login::GameProfile {
                    uuid: 42,
                    name: "AgentPickTest".into(),
                    properties: vec![],
                },
                session: 1,
            })
            .encode(),
        )
        .unwrap();
    server.recv().unwrap(); // Login acknowledged.
    server.recv().unwrap(); // Brand.
    server.recv().unwrap(); // Client information.
    server
        .send(&cfg::Clientbound::FinishConfiguration(Empty).encode())
        .unwrap();
    server.recv().unwrap(); // Finish configuration.
}

/// A guest at (0.5, 1, 0.5) looking west (yaw 90), its stored pick taken that way, with a pig
/// (entity 31) 1.5 blocks east of it, behind its back.
fn with_pig_behind(run: impl FnOnce(&mut Game<'_>)) {
    mod_platform::testing::with_mods(Vec::new().into(), || {
        with_client_game(|game| {
            let mut server = Connection::local(logged_client(game));
            server
                .reader
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            finish_login(&mut server);
            game.native_lifecycle
                .begin_open(mod_api::Authority::RemoteClient);
            game.title = None;
            game.player = Some(PlayerMotion::new(game.motion, Vec3::new(0.5, 1.0, 0.5)).unwrap());
            game.previous_position = Vec3::new(0.5, 1.0, 0.5);
            game.yaw = 90.0;
            game.pitch = 0.0;
            game.entity_reach = 3.0;
            game.block_reach = 4.5;
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            let pig = game
                .remote
                .as_ref()
                .unwrap()
                .builtin
                .get("minecraft:entity_type")
                .unwrap()
                .id("minecraft:pig")
                .unwrap();
            game.handle_packet(C::AddEntity(play::AddEntity {
                id: 31,
                uuid: 31,
                kind: pig,
                position: [2.0, 1.0, 0.5],
                velocity: [0.0; 3],
                pitch: 0,
                yaw: 0,
                head_yaw: 0,
                data: 0,
            }))
            .unwrap();
            game.refresh_client_pick(1.0);
            assert!(game.mob_hit().is_none(), "the pig starts behind the view");
            run(game);
        })
    });
}

fn command(game: &mut Game<'_>, line: &str) -> Result<Handled> {
    let (reply, _replies) = mpsc::channel();
    let words: Vec<&str> = line.split_whitespace().collect();
    game.agent_command(&words, line, &reply, &NoLoop)
}

#[test]
#[ignore = "daily"]
fn look_at_entity_replies_with_the_entity_it_turned_to() {
    with_pig_behind(|game| {
        let Ok(Handled::Reply(reply)) = command(game, "look-at entity 31") else {
            panic!("look-at entity 31 did not reply at once");
        };
        // East of the eye: yaw -90.
        assert!(
            (reply["yaw"].as_f64().unwrap() + 90.0).abs() < 1e-3,
            "{reply}"
        );
        assert_eq!(reply["target"]["kind"], json!("entity"));
        assert_eq!(reply["target"]["entity"]["id"], json!(31));
    });
}

#[test]
#[ignore = "daily"]
fn attack_by_id_hits_the_entity_it_turned_to() {
    with_pig_behind(|game| {
        let attacked = command(game, "attack 31");
        assert!(
            matches!(attacked, Ok(Handled::Deferred)),
            "attack 31 was refused: {:?}",
            attacked.err()
        );
        assert_eq!(game.agent.task.as_ref().map(|t| t.name), Some("attack"));
        assert_eq!(game.mob_hit().map(|(i, _, _)| game.mobs[i].id), Some(31));
    });
}
