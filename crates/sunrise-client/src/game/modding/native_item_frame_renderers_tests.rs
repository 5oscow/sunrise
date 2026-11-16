//! `register_entity_renderers` for item frames and glow item frames (#1296): a renderer
//! registered for `minecraft:item_frame` or `minecraft:glow_item_frame` draws in place of the
//! frame model and the item or map on its face, reading the frame's own state; with no renderer
//! the family's own geometry is unchanged; a failing renderer leaves that geometry until a
//! resource reload resolves it again.
use super::Game;
use super::native_entity_renderers::EntityRendererTable;
use mod_api::{
    BlockEntityDrawContext, Direction, EntityRenderState, EntityRenderer, Hooks,
    RegisterEntityRenderersEvent, RegistrationContext,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU32, Ordering},
};
use sunrise_client::entities::ItemFrame;
use sunrise_client::mesher::Vertex;

/// One stone quad a block wide, `height` blocks above the entity.
fn raised_quad(draw: &mut dyn BlockEntityDrawContext, height: f32) {
    let (low, high) = (height, height + 1.0);
    let corners = [
        [0.0, high, 0.0],
        [0.0, low, 0.0],
        [1.0, low, 0.0],
        [1.0, high, 0.0],
    ];
    draw.quad("block/stone", corners, [0.0, 0.0, 16.0, 16.0], 0xFF_FFFF);
}

/// What the frame renderer last read, and whether it was asked.
#[derive(Default)]
struct Heard {
    state: Option<EntityRenderState>,
}

/// Draws a quad 100 blocks up and records the frame it read.
struct Frame(Arc<Mutex<Heard>>);
impl EntityRenderer for Frame {
    fn submit(
        &self,
        entity: &EntityRenderState,
        draw: &mut dyn BlockEntityDrawContext,
    ) -> Result<(), String> {
        self.0.lock().unwrap().state = Some(*entity);
        raised_quad(draw, 100.0);
        Ok(())
    }
}

/// Refuses its first call with an `Err`, then draws a quad 400 blocks up; `calls` counts the
/// calls.
#[derive(Default)]
struct Flaky {
    calls: AtomicU32,
}
impl Flaky {
    fn calls(&self) -> u32 {
        self.calls.load(Ordering::Relaxed)
    }
}
impl EntityRenderer for Flaky {
    fn submit(
        &self,
        _: &EntityRenderState,
        draw: &mut dyn BlockEntityDrawContext,
    ) -> Result<(), String> {
        let first = self.calls.fetch_add(1, Ordering::Relaxed) == 0;
        raised_quad(draw, 400.0);
        if first {
            return Err("refused its first frame".into());
        }
        Ok(())
    }
}

/// The camera, south of a south-facing frame and looking at its front.
const CAMERA: glam::DVec3 = glam::DVec3::new(0.5, 64.5, 4.5);
/// The hanging entity's position: the centre of a frame at `[0, 64, 0]` facing south.
const CENTRE: [f64; 3] = [0.5, 64.5, 0.03125];
const FACING: u8 = 3;

/// How high above the entity a vertex is, in blocks.
fn above(v: &Vertex) -> f32 {
    v.pos[1] - (CENTRE[1] - CAMERA.y) as f32
}

/// The vertex's fields, for a byte-for-byte comparison (`Vertex` is not `PartialEq`).
fn verts(out: &[Vertex]) -> Vec<([f32; 3], [f32; 2], u32, u32, u32, [i8; 4], [f32; 2])> {
    out.iter()
        .map(|v| (v.pos, v.uv, v.layer, v.color, v.light, v.normal, v.glint_uv))
        .collect()
}

fn hanging(id: u32, glow: bool) -> ItemFrame {
    ItemFrame {
        attachment_lifetime: Default::default(),
        join: Default::default(),
        id,
        pos: [0, 64, 0],
        facing: FACING,
        glow,
        item: None,
        rotation: 0,
        drop_chance: 1.0,
        fixed: false,
        invisible: false,
        check_interval: 0,
        custom_name: None,
        random: sunrise_client::random_source::entity(),
    }
}

/// The vertices one frame of item frames draws for a single entity.
fn frame_frame(game: &mut Game<'_>, id: u32, glow: bool) -> Vec<Vertex> {
    game.item_frames.clear();
    game.item_frames.push(hanging(id, glow));
    let mut out = Vec::new();
    game.frame_geometry(CAMERA, &mut out);
    out
}

#[test]
fn an_item_frame_registration_resolves_to_the_frame_slots() {
    struct Mark;
    impl EntityRenderer for Mark {
        fn submit(
            &self,
            _: &EntityRenderState,
            _: &mut dyn BlockEntityDrawContext,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    let registered = std::collections::BTreeMap::from([
        (
            "minecraft:item_frame".to_owned(),
            Arc::new(Mark) as Arc<dyn EntityRenderer>,
        ),
        (
            "minecraft:glow_item_frame".to_owned(),
            Arc::new(Mark) as Arc<dyn EntityRenderer>,
        ),
    ]);
    let (table, unknown) =
        EntityRendererTable::resolve(&registered, &[], &Default::default(), |id| {
            [
                "minecraft:item_frame",
                "minecraft:glow_item_frame",
                "minecraft:oak_boat",
            ]
            .contains(&id)
        });
    assert!(table.item_frame(false).is_some());
    assert!(table.item_frame(true).is_some());
    assert_eq!(unknown, Vec::<String>::new());
}

/// Runs `check` on a client game with prepared resources and the scene's frame renderers,
/// resolved as the first load does. In a child process running only `test`: the registrations are
/// kept once per process.
fn in_scene(test: &str, check: impl FnOnce(&mut Game<'_>, &Arc<Mutex<Heard>>, &Flaky)) {
    if std::env::var_os("SUNRISE_ITEM_FRAME_RENDERERS_CHILD").is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("game::native_item_frame_renderers_tests::{test}"),
                "--include-ignored",
                "--nocapture",
            ])
            .env("SUNRISE_ITEM_FRAME_RENDERERS_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let heard = Arc::new(Mutex::new(Heard::default()));
    let flaky = Arc::new(Flaky::default());
    struct CallScene(Arc<Mutex<Heard>>);
    #[mod_api::__subscriptions]
    impl Hooks for CallScene {
        fn register_entity_renderers(
            &self,
            _: &RegistrationContext,
            event: &mut RegisterEntityRenderersEvent<'_>,
        ) {
            event.renderers.register(
                "minecraft:item_frame".into(),
                Arc::new(Frame(self.0.clone())),
            );
            event.renderers.register(
                "minecraft:glow_item_frame".into(),
                Arc::new(Frame(self.0.clone())),
            );
        }
    }
    struct RefuseScene(Arc<Flaky>);
    #[mod_api::__subscriptions]
    impl Hooks for RefuseScene {
        fn register_entity_renderers(
            &self,
            _: &RegistrationContext,
            event: &mut RegisterEntityRenderersEvent<'_>,
        ) {
            event
                .renderers
                .register("minecraft:item_frame".into(), self.0.clone());
        }
    }
    if test.starts_with("a_failed") {
        let mods = vec![mod_api::ModEntry::new(
            "scene",
            Box::new(RefuseScene(flaky.clone())),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            super::mods::register_entity_renderers(super::execution_role::ExecutionRole::Client);
            super::native_entity_refs::tests::with_client_game(|game| {
                game.resolve_entity_renderers();
                check(game, &heard, &flaky);
            });
        });
        return;
    }
    let mods = vec![mod_api::ModEntry::new(
        "scene",
        Box::new(CallScene(heard.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || {
        super::mods::register_entity_renderers(super::execution_role::ExecutionRole::Client);
        super::native_entity_refs::tests::with_client_game(|game| {
            game.resolve_entity_renderers();
            check(game, &heard, &flaky);
        });
    });
}

fn assert_frame_state(state: EntityRenderState, id: u32, glow: bool) {
    assert_eq!(state.id, id);
    assert_eq!(state.partial, 0.0);
    assert_eq!(state.age, 0.0);
    assert_eq!(state.yaw, 0.0);
    assert_eq!(state.pitch, 0.0);
    assert_eq!(state.living, None);
    let frame = state.item_frame.expect("item frame state");
    assert_eq!(frame.facing, Direction::South);
    assert_eq!(frame.glow, glow);
    assert!(!frame.invisible);
    assert_eq!(frame.rotation, 0);
    assert!(!frame.has_item);
    assert!(!frame.has_map);
}

#[test]
#[ignore = "daily"]
fn a_registered_item_frame_draws_its_renderer_in_place_of_the_model() {
    in_scene(
        "a_registered_item_frame_draws_its_renderer_in_place_of_the_model",
        |game, heard, _| {
            let drawn = frame_frame(game, 91, false);
            assert_eq!(drawn.len(), 4, "one quad and no frame: {drawn:?}");
            assert!(drawn.iter().all(|v| (100.0..=101.0).contains(&above(v))));
            let state = heard.lock().unwrap().state.expect("the renderer was asked");
            assert_frame_state(state, 91, false);

            heard.lock().unwrap().state = None;
            let drawn = frame_frame(game, 94, true);
            assert_eq!(drawn.len(), 4, "glow: one quad and no frame: {drawn:?}");
            assert!(drawn.iter().all(|v| (100.0..=101.0).contains(&above(v))));
            let state = heard
                .lock()
                .unwrap()
                .state
                .expect("the glow renderer was asked");
            assert_frame_state(state, 94, true);
        },
    );
}

/// With no renderer registered, an item frame's geometry is byte-identical to the family's own
/// route (the model near the entity, no raised quad), and two frames of the same entity match.
#[test]
#[ignore = "daily"]
fn an_unregistered_item_frame_is_byte_identical_to_the_vanilla_route() {
    if std::env::var_os("SUNRISE_ITEM_FRAME_RENDERERS_PLAIN").is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "game::native_item_frame_renderers_tests::an_unregistered_item_frame_is_byte_identical_to_the_vanilla_route",
                "--include-ignored",
                "--nocapture",
            ])
            .env("SUNRISE_ITEM_FRAME_RENDERERS_PLAIN", "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    super::native_entity_refs::tests::with_client_game(|game| {
        game.resolve_entity_renderers();
        let first = frame_frame(game, 92, false);
        assert!(!first.is_empty(), "its vanilla frame model");
        assert!(
            first.iter().all(|v| above(v) < 2.0),
            "near the entity: {first:?}"
        );
        let second = frame_frame(game, 92, false);
        assert_eq!(
            verts(&first),
            verts(&second),
            "two frames of the same item frame match byte for byte"
        );
        let glow = frame_frame(game, 95, true);
        assert!(!glow.is_empty(), "its vanilla glow frame model");
        assert!(
            glow.iter().all(|v| above(v) < 2.0),
            "glow near the entity: {glow:?}"
        );
    });
}

/// A failing renderer falls back to the frame model and is asked no more; only a reload's fresh
/// table asks it again.
#[test]
#[ignore = "daily"]
fn a_failed_item_frame_renderer_draws_the_model_until_a_resource_reload() {
    in_scene(
        "a_failed_item_frame_renderer_draws_the_model_until_a_resource_reload",
        |game, _, flaky| {
            for frame_name in ["the failing frame", "the next frame"] {
                let drawn = frame_frame(game, 93, false);
                assert!(!drawn.is_empty(), "{frame_name}: its own model");
                assert!(
                    drawn.iter().all(|v| above(v) < 2.0),
                    "{frame_name}: no quad kept: {drawn:?}"
                );
                assert_eq!(flaky.calls(), 1, "{frame_name}: asked once, then no more");
            }
            super::model_baking_completed_tests::reload(game);
            let drawn = frame_frame(game, 93, false);
            assert_eq!(flaky.calls(), 2, "the reload resolved it again");
            assert_eq!(drawn.len(), 4, "one quad and no frame: {drawn:?}");
            assert!(drawn.iter().all(|v| (400.0..=401.0).contains(&above(v))));
        },
    );
}
