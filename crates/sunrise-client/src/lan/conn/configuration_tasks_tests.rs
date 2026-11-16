//! The public task hooks over the real memory-connection transport.
use super::super::tests::login_profile;
use super::*;
use crate::lan::configuration_tasks::Session;
use mod_api::{
    ConfigurationConnection, ConfigurationTask, ConfigurationTaskError, Hooks, Host,
    IPayloadContext, IPayloadHandler, Identifier, ModEntry, PayloadBytes, PayloadId, Registrar,
    ServerHost,
};
use std::sync::Mutex;

const CHANNEL: &str = "task_fixture:value";
fn kind(name: &str) -> Identifier {
    Identifier::parse(&format!("task_fixture:{name}")).unwrap()
}
#[derive(Clone, Copy)]
enum Case {
    Reply,
    WrongType,
    Failure,
    Vanilla,
    UnsupportedPeer,
    Tick,
}
struct Fixture {
    case: Case,
    connection: Mutex<Option<(ConfigurationConnection, Arc<Session>)>>,
    starts: Mutex<Vec<Identifier>>,
    ack: std::sync::mpsc::Sender<()>,
}
struct Mod(Arc<Fixture>);
struct Reply(Arc<Fixture>);
impl IPayloadHandler for Reply {
    fn handle(
        &self,
        bytes: PayloadBytes,
        _: &mut dyn IPayloadContext,
    ) -> Result<(), mod_api::PayloadHandleError> {
        assert_eq!(bytes.0, [7]);
        let (connection, session) = self.0.connection.lock().unwrap().clone().unwrap();
        connection
            .finish(&mut TaskHost(session), &kind("first"))
            .unwrap();
        self.0.ack.send(()).unwrap();
        Ok(())
    }
}
#[sunrise_mod_api_macros::__subscriptions]
impl Hooks for Mod {
    fn register(&self, registrar: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        registrar
            .payload_handlers()
            .registrar("1")
            .optional()
            .executes_on(mod_api::HandlerThread::Network)
            .configuration_bidirectional(
                PayloadId(CHANNEL.into()),
                Arc::new(Reply(self.0.clone())),
                Some(Arc::new(Reply(self.0.clone()))),
            );
    }
    fn register_configuration_tasks(
        &self,
        _: &mut dyn ServerHost,
        event: &mut mod_api::RegisterConfigurationTasksEvent<'_>,
    ) {
        assert_eq!(event.name, "Alex");
        event.tasks.push(ConfigurationTask::new(kind("first")));
        event.tasks.push(ConfigurationTask::new(kind("second")));
    }
    fn configuration_task_start(
        &self,
        host: &mut dyn ServerHost,
        event: &mut mod_api::ConfigurationTaskStartEvent<'_>,
    ) {
        self.0.starts.lock().unwrap().push(event.task_type.clone());
        if event.task_type == &kind("second") {
            event.connection.finish(host, event.task_type).unwrap();
            return;
        }
        match self.0.case {
            Case::Reply => event
                .connection
                .send_payload(host, &PayloadId(CHANNEL.into()), PayloadBytes(vec![7]))
                .unwrap(),
            Case::WrongType => assert_eq!(
                event.connection.finish(host, &kind("wrong")),
                Err(ConfigurationTaskError::WrongType)
            ),
            Case::Failure => event.connection.fail(host, "fixture failure"),
            Case::Vanilla => {
                assert!(
                    !event
                        .connection
                        .has_channel(host, &PayloadId(CHANNEL.into()))
                );
                event.connection.finish(host, event.task_type).unwrap();
            }
            Case::Tick => {}
            Case::UnsupportedPeer => assert!(
                event
                    .connection
                    .send_payload(host, &PayloadId(CHANNEL.into()), PayloadBytes(vec![7]),)
                    .is_err()
            ),
        }
    }
    fn configuration_task_tick(
        &self,
        _: &mut dyn ServerHost,
        event: &mut mod_api::ConfigurationTaskTickEvent<'_>,
    ) {
        if matches!(self.0.case, Case::Tick) {
            event.finished = true;
        }
    }
}
struct TaskHost(Arc<Session>);
macro_rules! default_areas { ($($area:ident),* $(,)?) => { $(impl mod_api::prelude::$area for TaskHost {})* }; }
default_areas!(
    BlockEntitiesHost,
    BlockStatesHost,
    ClientViewHost,
    ConfigHost,
    EntitiesHost,
    FluidsHost,
    ItemEntitiesHost,
    ItemsHost,
    KeysHost,
    LevelsHost,
    LivingHost,
    MathHost,
    MobsHost,
    ModStateHost,
    PayloadTokensHost,
    PlayersHost,
    RecipePropertiesHost,
    RecipesHost,
    ResourcesHost,
    TagsHost,
    TextHost,
    AdvancementsServerHost,
    BlockEntitiesServerHost,
    BlockStatesServerHost,
    BossBarsServerHost,
    ConfigServerHost,
    DisplaysServerHost,
    EntitiesServerHost,
    ItemsServerHost,
    MenusServerHost,
    MerchantsServerHost,
    MinecartsServerHost,
    MobsServerHost,
    ModStateServerHost,
    PayloadsServerHost,
    PlayersServerHost,
    ProjectilesServerHost,
    RecipesServerHost,
    ScoreboardServerHost,
    ServicesServerHost,
    WorldEditServerHost,
);
impl Host for TaskHost {}
impl ServerHost for TaskHost {}
impl mod_api::prelude::ConfigurationTasksServerHost for TaskHost {
    fn configuration_has_channel(&self, _: ConfigurationConnection, channel: &PayloadId) -> bool {
        self.0.has_channel(channel)
    }
    fn configuration_finish(
        &mut self,
        _: ConfigurationConnection,
        task_type: &Identifier,
    ) -> Result<(), ConfigurationTaskError> {
        self.0.finish(task_type)
    }
    fn configuration_fail(&mut self, _: ConfigurationConnection, reason: &str) {
        self.0.fail(reason);
    }
    fn configuration_send_payload(
        &mut self,
        _: ConfigurationConnection,
        id: &PayloadId,
        bytes: PayloadBytes,
    ) -> Result<(), ConfigurationTaskError> {
        let result = (|| {
            let endpoint = self.0.endpoint();
            let epoch = endpoint.transport().epoch().unwrap();
            endpoint.prepare(&epoch, id, bytes)?.publish()
        })();
        result.map_err(|error| {
            self.0
                .fail(format!("Configuration payload refused: {error:?}"));
            ConfigurationTaskError::Payload(error)
        })
    }
}
fn turn(connection: ConfigurationConnection, session: &Arc<Session>) {
    let mut host = TaskHost(session.clone());
    while let Some(task) = session.turn() {
        if task.start {
            mod_api::dispatch_configuration_task_start(&mut host, connection, &task.task_type);
        } else {
            let mut finished = false;
            mod_api::dispatch_configuration_task_tick(
                &mut host,
                connection,
                &task.task_type,
                &mut finished,
            );
            if finished {
                session.finish(&task.task_type).unwrap();
            }
        }
        if !task.start || !session.finished() {
            break;
        }
    }
}
fn next(c: &mut Connection) -> cfg::Clientbound {
    let (id, bytes) = c.recv().unwrap();
    cfg::Clientbound::decode(id, &bytes).unwrap()
}
#[test]
fn tasks_gate_admission_and_complete_through_the_registered_transport() {
    mod_platform::__private::initialize().unwrap();
    for case in [
        Case::Reply,
        Case::WrongType,
        Case::Failure,
        Case::Vanilla,
        Case::UnsupportedPeer,
        Case::Tick,
    ] {
        let (ack, received) = channel();
        let fixture = Arc::new(Fixture {
            ack,
            case,
            connection: Mutex::new(None),
            starts: Mutex::new(Vec::new()),
        });
        mod_platform::testing::with_mods(
            vec![ModEntry::new(
                "task_fixture",
                Box::new(Mod(fixture.clone())),
            )]
            .into(),
            || {
                mod_platform::__private::part_tests::install_parts_fixture(
                    mod_api::__private::mods(),
                )
                .unwrap();
                let config = crate::lan::NetConfig {
                    listen: false,
                    ..crate::lan::NetConfig::loopback(0, String::new(), 8)
                };
                let lan = crate::lan::Lan::start_with_configuration_tasks(
                    Arc::new(SyncData::empty()),
                    config,
                    true,
                )
                .unwrap();
                let mut c = Connection::local(lan.connect_local().unwrap());
                c.reader
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                login_profile(&mut c, "Alex");
                c.send(&login::Serverbound::LoginAcknowledged(Empty).encode())
                    .unwrap();
                loop {
                    match next(&mut c) {
                        cfg::Clientbound::CustomPayload(payload)
                            if payload.channel
                                == sunrise_protocol::payload_negotiation::CHANNEL
                                && !matches!(case, Case::Vanilla | Case::UnsupportedPeer) =>
                        {
                            use sunrise_protocol::payload_negotiation::{CHANNEL, Control};
                            if Control::decode(&payload.data).unwrap() == Control::Query {
                                c.send(
                                    &cfg::Serverbound::CustomPayload(cfg::CustomPayload {
                                        channel: CHANNEL.into(),
                                        data: Control::Offer(
                                            lan.shared.payload_local.manifest.clone(),
                                        )
                                        .encode()
                                        .unwrap(),
                                    })
                                    .encode(),
                                )
                                .unwrap();
                            }
                        }
                        cfg::Clientbound::SelectKnownPacks(packs) => {
                            c.send(&cfg::Serverbound::SelectKnownPacks(packs).encode())
                                .unwrap();
                            break;
                        }
                        _ => {}
                    }
                }
                let request = lan
                    .configuration_requests
                    .as_ref()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
                let connection = request.connection;
                let session = request.session;
                *fixture.connection.lock().unwrap() = Some((connection, session.clone()));
                let mut tasks = Vec::new();
                mod_api::dispatch_register_configuration_tasks(
                    &mut TaskHost(session.clone()),
                    connection,
                    request.player,
                    &request.name,
                    &mut tasks,
                );
                session.register(tasks);
                turn(connection, &session);
                if matches!(case, Case::Reply) {
                    loop {
                        match next(&mut c) {
                            cfg::Clientbound::CustomPayload(p) if p.channel == CHANNEL => {
                                assert_eq!(p.data, [7]);
                                break;
                            }
                            cfg::Clientbound::FinishConfiguration(_) => {
                                panic!("admitted before task reply")
                            }
                            _ => {}
                        }
                    }
                    assert!(session.result().is_none());
                    c.send(
                        &cfg::Serverbound::CustomPayload(cfg::CustomPayload {
                            channel: CHANNEL.into(),
                            data: vec![7],
                        })
                        .encode(),
                    )
                    .unwrap();
                    received.recv_timeout(Duration::from_secs(5)).unwrap();
                    assert!(session.finished());
                    turn(connection, &session);
                } else if matches!(case, Case::Tick) {
                    assert!(session.result().is_none());
                    turn(connection, &session);
                    turn(connection, &session);
                }
                loop {
                    match next(&mut c) {
                        cfg::Clientbound::Disconnect(reason) => {
                            assert!(matches!(
                                case,
                                Case::WrongType | Case::Failure | Case::UnsupportedPeer
                            ));
                            let reason =
                                sunrise_protocol::text::plain(&reason.reason, &Default::default());
                            assert!(reason.contains("Configuration error"), "{reason}");
                            break;
                        }
                        cfg::Clientbound::FinishConfiguration(_) => {
                            assert!(!matches!(
                                case,
                                Case::WrongType | Case::Failure | Case::UnsupportedPeer
                            ));
                            assert_eq!(
                                *fixture.starts.lock().unwrap(),
                                vec![kind("first"), kind("second")]
                            );
                            c.send(&cfg::Serverbound::FinishConfiguration(Empty).encode())
                                .unwrap();
                            let receipt = lan
                                .events
                                .recv_timeout_owned(Duration::from_secs(5))
                                .unwrap();
                            assert!(matches!(receipt.into_parts().0, NetEvent::Joined(_)));
                            break;
                        }
                        _ => {}
                    }
                }
            },
        );
    }
}
