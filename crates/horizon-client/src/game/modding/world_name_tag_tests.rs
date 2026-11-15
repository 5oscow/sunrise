//! `WorldDrawContext::name_tag` (#4171): a mod's label hides behind a wall unless it is
//! see-through, shrinks with distance as a name tag does, and is refused whole for text too deep
//! or a position that is not finite.
use super::host::draw::GameWorldDrawContext;
use super::native_entity_refs::tests::with_client_game;
use glam::{DVec3, Mat4, Vec3};
use horizon_client::{mesher::Vertex, world_geometry::WorldGeometry};
use mod_api::{DrawError, NameTagOptions, Text, WorldDrawContext};

/// Four blocks in front of the label's anchor (0.5 above (0.5, 70, 0.5)), facing south (+Z).
const CAMERA: [f64; 3] = [0.5, 70.5, -3.5];

/// The view the tests draw with: the camera looking south, reversed depth as the game's.
fn view_proj() -> Mat4 {
    let projection = Mat4::perspective_rh(70f32.to_radians(), 1.0, 512.0, 0.05);
    projection * Mat4::look_to_rh(Vec3::ZERO, Vec3::Z, Vec3::Y)
}

/// One view's labels at `at`, the game's camera facing south: what it draws with the name
/// tags, then the see-through copies.
fn labels(
    game: &mut super::Game<'_>,
    at: [f64; 3],
    text: &Text,
    options: NameTagOptions,
) -> (Result<(), DrawError>, [Vec<Vertex>; 2]) {
    game.yaw = 0.0;
    game.pitch = 0.0;
    game.posed_rotation = None;
    game.camera_type = horizon_client::camera::CameraType::FirstPerson;
    let mut draw = WorldGeometry::default();
    draw.reset(CAMERA, Mat4::IDENTITY, [0.5, 0.5]);
    let result = GameWorldDrawContext::new(game, &mut draw).name_tag(at, text, options);
    (result, draw.labels().map(<[Vertex]>::to_vec))
}

/// An opaque grey wall two blocks in front of the camera, in the white texel.
fn wall(game: &super::Game<'_>) -> Vec<Vertex> {
    let white = game.gui.atlas.sprites["horizon/white"];
    let uv = [(white.u0 + white.u1) * 0.5, (white.v0 + white.v1) * 0.5];
    // Counterclockwise as seen from the camera.
    [[4.0, -4.0], [-4.0, -4.0], [-4.0, 4.0], [4.0, 4.0]]
        .map(|[x, y]| Vertex {
            pos: [x, y, 2.0],
            uv,
            layer: 0,
            color: 0xFF80_8080,
            light: horizon_client::block_entity_render::vertex_light(0xF0),
            normal: [0; 4],
            glint_uv: [0.0; 2],
        })
        .to_vec()
}

fn pixels(game: &super::Game<'_>, world: &[Vertex], see_through: &[Vertex]) -> Vec<u8> {
    horizon_client::gfx::entity_pixel_test::render_see_through(
        world,
        see_through,
        [game.gui.atlas.width, game.gui.atlas.height],
        &game.gui.atlas.pixels(),
        view_proj(),
    )
    .expect("software Vulkan label pixels")
}

#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn a_label_hides_behind_a_wall_unless_it_is_see_through() {
    with_client_game(|game| {
        let text = Text::literal("Label");
        let at = [0.5, 70.0, 0.5];
        let (ok, [world, through]) =
            labels(game, at, &text, NameTagOptions::new(false, true, 0xF0));
        assert_eq!(ok, Ok(()));
        assert!(!world.is_empty() && through.is_empty());
        let empty = pixels(game, &[], &[]);
        assert_ne!(pixels(game, &world, &[]), empty, "visible without a wall");
        let wall = wall(game);
        let only_wall = pixels(game, &wall, &[]);
        let behind = [wall.clone(), world].concat();
        assert_eq!(
            pixels(game, &behind, &[]),
            only_wall,
            "hidden behind the wall"
        );

        let (ok, [world, through]) = labels(game, at, &text, NameTagOptions::new(true, true, 0xF0));
        assert_eq!(ok, Ok(()));
        assert!(!world.is_empty() && !through.is_empty());
        let behind = [wall, world].concat();
        let shown = pixels(game, &behind, &through);
        assert_ne!(shown, only_wall, "the dim copy over the wall");
    });
}

#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn labels_four_times_as_far_are_a_quarter_the_size_on_screen() {
    with_client_game(|game| {
        let text = Text::literal("Size");
        let width = |game: &mut super::Game<'_>, z: f64| {
            // The anchor is 0.5 below the label; the label `z` blocks ahead of the camera.
            let at = [CAMERA[0], CAMERA[1] - 0.5, CAMERA[2] + z];
            let (ok, [world, _]) = labels(game, at, &text, NameTagOptions::new(false, false, 0xF0));
            assert_eq!(ok, Ok(()));
            let xs = world
                .iter()
                .map(|v| view_proj().project_point3(Vec3::from(v.pos)).x);
            let (lo, hi) = xs.fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
            hi - lo
        };
        let (near, far) = (width(game, 4.0), width(game, 16.0));
        assert!(near > 0.0);
        assert!((near / far - 4.0).abs() < 1e-3, "{near} / {far}");
    });
}

#[test]
#[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
fn a_label_too_deep_or_not_finite_is_refused_and_draws_nothing() {
    with_client_game(|game| {
        let mut too_deep = Text::literal("leaf");
        for _ in 1..513 {
            too_deep = Text::literal("parent").append(too_deep);
        }
        let options = NameTagOptions::new(true, true, 0xF0);
        let (refused, drawn) = labels(game, [0.5, 70.0, 0.5], &too_deep, options);
        assert_eq!(refused, Err(DrawError::TextTooDeep));
        assert!(drawn.iter().all(Vec::is_empty));
        for at in [[f64::NAN, 70.0, 0.5], [0.5, f64::INFINITY, 0.5]] {
            let (refused, drawn) = labels(game, at, &Text::literal("x"), options);
            assert_eq!(refused, Err(DrawError::NonFinite));
            assert!(drawn.iter().all(Vec::is_empty));
        }
        // Drawn half a block above its anchor: in the plane four blocks ahead, facing the camera.
        let (_, [world, _]) = labels(game, [0.5, 70.0, 0.5], &Text::literal("x"), options);
        let centre = world
            .iter()
            .fold(DVec3::ZERO, |sum, v| sum + Vec3::from(v.pos).as_dvec3())
            / world.len() as f64;
        assert!((centre.z - 4.0).abs() < 0.01, "{centre}");
    });
}
