//! The agent's turns toward a point read the pick the game takes after the turn: `attack ID`
//! and `look-at entity ID` wait for the next tick or frame pick (client_pick.rs) instead of
//! picking themselves, so a `client_pick` listener runs only in those calls.
use super::*;
use crate::game::native_client_session::tests::logged_client;
use crate::game::native_entity_refs::tests::with_client_game;
use mod_api::{ClientHost, ClientPickEvent, CrosshairTarget, Hooks};
use std::sync::{Arc, Mutex, mpsc};
use sunrise_client::{chunk::Chunk, motion::PlayerMotion};
use sunrise_core::geometry::Vec3;
use sunrise_protocol::connection::Connection;
use sunrise_protocol::packets::play::{self, Clientbound as C};

/// A subscribed `client_pick` listener: counts its calls, and can replace the result.
#[derive(Default)]
struct Listener {
    calls: usize,
    result: Option<Option<CrosshairTarget>>,
}
struct Picks(Arc<Mutex<Listener>>);
#[mod_api::__subscriptions]
impl Hooks for Picks {
    fn client_pick(&self, _: &mut dyn ClientHost, event: &mut ClientPickEvent<'_>) {
        let mut listener = self.0.lock().unwrap();
        listener.calls += 1;
        if let Some(result) = listener.result {
            *event.result = result;
        }
    }
}

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
/// (entity 31) 1.5 blocks east of it, behind its back. The listener's count starts at zero.
fn with_pig_behind(run: impl FnOnce(&mut Game<'_>, &Mutex<Listener>)) {
    let listener = Arc::new(Mutex::new(Listener::default()));
    let mods = vec![mod_api::ModEntry::new(
        "agent_picks",
        Box::new(Picks(listener.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || {
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
            listener.lock().unwrap().calls = 0;
            run(game, &listener);
        })
    });
}

fn command(game: &mut Game<'_>, line: &str) -> (Result<Handled>, mpsc::Receiver<String>) {
    let (reply, replies) = mpsc::channel();
    let words: Vec<&str> = line.split_whitespace().collect();
    (game.agent_command(&words, line, &reply, &NoLoop), replies)
}

fn calls(listener: &Mutex<Listener>) -> usize {
    listener.lock().unwrap().calls
}

/// A drawn frame: its pick, then the control poll's agent step.
fn frame(game: &mut Game<'_>) {
    game.refresh_client_pick(1.0);
    game.frames += 1;
    game.advance_agent();
}

/// A game tick's pick before its inputs, then the control poll's agent step.
fn tick(game: &mut Game<'_>) {
    game.ticks += 1;
    game.refresh_client_pick(1.0);
    game.advance_agent();
}

/// The attack clicks `KeyMapping.click` has queued and nothing has consumed yet.
fn attack_clicks(game: &Game<'_>) -> usize {
    let mut attack = game.toggles.0[2].clone();
    std::iter::from_fn(|| attack.consume_click().then_some(())).count()
}

fn reply(replies: &mpsc::Receiver<String>) -> Value {
    serde_json::from_str(&replies.try_recv().expect("a reply")).unwrap()
}

#[test]
#[ignore = "daily"]
fn look_at_entity_replies_with_the_entity_it_turned_to() {
    with_pig_behind(|game, listener| {
        let (handled, replies) = command(game, "look-at entity 31");
        assert!(matches!(handled, Ok(Handled::Deferred)));
        // East of the eye: yaw -90, set at once; the pick waits for the frame.
        assert!((game.yaw + 90.0).abs() < 1e-3, "{}", game.yaw);
        assert_eq!(calls(listener), 0, "the command itself picks nothing");
        assert_eq!(game.crosshair_target(), None);
        assert!(replies.try_recv().is_err());
        frame(game);
        assert_eq!(calls(listener), 1, "the frame's own pick");
        let reply = reply(&replies);
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
fn look_at_over_previews_the_angle_without_picking() {
    with_pig_behind(|game, listener| {
        let (handled, _replies) = command(game, "look-at entity 31 --over 1");
        assert!(matches!(handled, Ok(Handled::Deferred)));
        assert_eq!(calls(listener), 0, "the preview fires no listener");
        assert_eq!(game.crosshair_target(), None, "nor publishes a target");
        assert_eq!(
            (game.yaw, game.pitch),
            (90.0, 0.0),
            "the view turns by mouse"
        );
    });
}

#[test]
#[ignore = "daily"]
fn attack_by_id_hits_the_entity_it_turned_to() {
    with_pig_behind(|game, listener| {
        let (attacked, replies) = command(game, "attack 31");
        assert!(
            matches!(attacked, Ok(Handled::Deferred)),
            "attack 31 was refused: {:?}",
            attacked.err()
        );
        assert_eq!(game.agent.task.as_ref().map(|t| t.name), Some("attack"));
        assert_eq!(calls(listener), 0, "the command itself picks nothing");
        assert_eq!(attack_clicks(game), 0, "no click before the tick's pick");
        let health = game.mobs.iter().find(|m| m.id == 31).unwrap().health;
        frame(game);
        assert_eq!(
            attack_clicks(game),
            0,
            "a frame is not the pick the click reads"
        );
        tick(game);
        assert_eq!(calls(listener), 2, "the frame's and the tick's own picks");
        assert_eq!(attack_clicks(game), 1);
        assert_eq!(game.mob_hit().map(|(i, _, _)| game.mobs[i].id), Some(31));
        for _ in 0..4 {
            assert!(replies.try_recv().is_err());
            tick(game);
        }
        let reply = reply(&replies);
        assert_eq!(reply["target"], json!(31), "{reply}");
        assert_eq!(reply["hit"], json!(true), "{reply}");
        assert_eq!(reply["health_before"], json!(health), "{reply}");
    });
}

#[test]
#[ignore = "daily"]
fn attack_by_id_ends_without_a_click_on_death_or_without_ticks() {
    with_pig_behind(|game, _| {
        let (_, replies) = command(game, "attack 31");
        game.screen = Screen::Dead;
        game.advance_agent();
        assert_eq!(reply(&replies)["error"], json!("the player died"));
        game.screen = Screen::Playing;
        let (_, replies) = command(game, "attack 31");
        game.agent.task.as_mut().unwrap().deadline = Instant::now();
        game.advance_agent();
        assert_eq!(
            reply(&replies)["error"],
            json!("game ticks are not advancing (paused?)")
        );
        assert_eq!(attack_clicks(game), 0);
    });
}

#[test]
#[ignore = "daily"]
fn attack_by_id_follows_the_listener_decision() {
    with_pig_behind(|game, listener| {
        listener.lock().unwrap().result = Some(None);
        let (attacked, replies) = command(game, "attack 31");
        assert!(matches!(attacked, Ok(Handled::Deferred)));
        tick(game);
        assert_eq!(calls(listener), 1);
        let reply = reply(&replies);
        assert_eq!(
            reply["error"],
            json!("that entity is out of reach or hidden; get closer")
        );
        assert!(game.agent.task.is_none());
        assert_eq!(attack_clicks(game), 0, "nothing was clicked");
    });
}
