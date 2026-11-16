//! `register_entity_renderers` for paintings (#1296): a renderer registered for
//! `minecraft:painting` draws in place of the painting's quads, reading the facing and size; with no
//! renderer the family's own geometry is unchanged; a failing renderer leaves those quads until a
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
use sunrise_client::entities::Painting;
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

/// What the painting renderer last read, and whether it was asked.
#[derive(Default)]
struct Heard {
    state: Option<EntityRenderState>,
}

/// Draws a quad 100 blocks up and records the frame it read.
struct Paint(Arc<Mutex<Heard>>);
impl EntityRenderer for Paint {
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

/// The camera, west of a west-facing painting and looking at its front.
const CAMERA: glam::DVec3 = glam::DVec3::new(-4.0, 64.5, 0.5);
/// The hanging entity's position: the centre of a 1x1 painting at `[0, 64, 0]` facing west.
const CENTRE: [f64; 3] = [0.96875, 64.5, 0.5];
const FACING: u8 = 1;
const WIDTH: i32 = 1;
const HEIGHT: i32 = 1;

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

fn hanging(id: u32) -> Painting {
    Painting {
        attachment_lifetime: Default::default(),
        join: Default::default(),
        id,
        pos: [0, 64, 0],
        facing: FACING,
        variant: Arc::from("minecraft:kebab"),
        width: WIDTH,
        height: HEIGHT,
        check_interval: 0,
    }
}

/// The vertices one frame of paintings draws for a single entity.
fn painting_frame(game: &mut Game<'_>, id: u32) -> Vec<Vertex> {
    game.paintings.clear();
    game.paintings.push(hanging(id));
    let mut out = Vec::new();
    game.painting_geometry(CAMERA, &mut out);
    out
}

#[test]
fn a_painting_registration_resolves_to_the_painting_slot() {
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
    let registered = std::collections::BTreeMap::from([(
        "minecraft:painting".to_owned(),
        Arc::new(Mark) as Arc<dyn EntityRenderer>,
    )]);
    let (table, unknown) =
        EntityRendererTable::resolve(&registered, &[], &Default::default(), |id| {
            ["minecraft:painting", "minecraft:oak_boat"].contains(&id)
        });
    assert!(table.painting().is_some());
    assert_eq!(unknown, Vec::<String>::new());
}

/// Runs `check` on a client game with prepared resources and the scene's painting renderer,
/// resolved as the first load does. In a child process running only `test`: the registrations are
/// kept once per process.
fn in_scene(test: &str, check: impl FnOnce(&mut Game<'_>, &Arc<Mutex<Heard>>, &Flaky)) {
    if std::env::var_os("SUNRISE_PAINTING_RENDERERS_CHILD").is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("game::native_painting_renderers_tests::{test}"),
                "--include-ignored",
                "--nocapture",
            ])
            .env("SUNRISE_PAINTING_RENDERERS_CHILD", "1")
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
            event
                .renderers
                .register("minecraft:painting".into(), Arc::new(Paint(self.0.clone())));
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
                .register("minecraft:painting".into(), self.0.clone());
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

#[test]
#[ignore = "daily"]
fn a_registered_painting_draws_its_renderer_in_place_of_the_quads() {
    in_scene(
        "a_registered_painting_draws_its_renderer_in_place_of_the_quads",
        |game, heard, _| {
            let drawn = painting_frame(game, 91);
            assert_eq!(drawn.len(), 4, "one quad and no painting: {drawn:?}");
            assert!(drawn.iter().all(|v| (100.0..=101.0).contains(&above(v))));
            let state = heard.lock().unwrap().state.expect("the renderer was asked");
            assert_eq!(state.id, 91);
            assert_eq!(state.partial, 0.0);
            assert_eq!(state.age, 0.0);
            assert_eq!(state.yaw, 90.0);
            assert_eq!(state.pitch, 0.0);
            assert_eq!(state.living, None);
            let painting = state.painting.expect("painting state");
            assert_eq!(painting.facing, Direction::West);
            assert_eq!((painting.width, painting.height), (WIDTH, HEIGHT));
        },
    );
}

/// With no renderer registered, a painting's geometry is byte-identical to the family's own
/// route (the quads near the entity, no raised quad), and two frames of the same entity match.
#[test]
#[ignore = "daily"]
fn an_unregistered_painting_is_byte_identical_to_the_vanilla_route() {
    if std::env::var_os("SUNRISE_PAINTING_RENDERERS_PLAIN").is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "game::native_painting_renderers_tests::an_unregistered_painting_is_byte_identical_to_the_vanilla_route",
                "--include-ignored",
                "--nocapture",
            ])
            .env("SUNRISE_PAINTING_RENDERERS_PLAIN", "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    super::native_entity_refs::tests::with_client_game(|game| {
        game.resolve_entity_renderers();
        let first = painting_frame(game, 92);
        assert!(!first.is_empty(), "its vanilla quads");
        assert!(
            first.iter().all(|v| above(v) < 2.0),
            "near the entity: {first:?}"
        );
        let second = painting_frame(game, 92);
        assert_eq!(
            verts(&first),
            verts(&second),
            "two frames of the same painting match byte for byte"
        );
    });
}

/// A failing renderer falls back to the quads and is asked no more; only a reload's fresh table
/// asks it again.
#[test]
#[ignore = "daily"]
fn a_failed_painting_renderer_draws_the_quads_until_a_resource_reload() {
    in_scene(
        "a_failed_painting_renderer_draws_the_quads_until_a_resource_reload",
        |game, _, flaky| {
            for frame_name in ["the failing frame", "the next frame"] {
                let drawn = painting_frame(game, 93);
                assert!(!drawn.is_empty(), "{frame_name}: its own quads");
                assert!(
                    drawn.iter().all(|v| above(v) < 2.0),
                    "{frame_name}: no quad kept: {drawn:?}"
                );
                assert_eq!(flaky.calls(), 1, "{frame_name}: asked once, then no more");
            }
            super::model_baking_completed_tests::reload(game);
            let drawn = painting_frame(game, 93);
            assert_eq!(flaky.calls(), 2, "the reload resolved it again");
            assert_eq!(drawn.len(), 4, "one quad and no painting: {drawn:?}");
            assert!(drawn.iter().all(|v| (400.0..=401.0).contains(&above(v))));
        },
    );
}
