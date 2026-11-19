//! A pack's Polytone expression uniforms on its lightmap shader (`shader_modifiers`) through a
//! game's frame: the atlas gives the shader the block its pack names, and the frame the game
//! builds evaluates the expression into the renderer's lightmap pipeline. A game with no pack
//! modifiers evaluates nothing.
use super::Game;
use std::sync::Arc;
use sunrise_assets::resource_source::{Resource, ResourceSource};

const SCREENQUAD: &str = "assets/minecraft/shaders/core/screenquad.vsh";
const LIGHTMAP: &str = "assets/minecraft/shaders/core/lightmap.fsh";
const MODIFIER: &str = "assets/minecraft/polytone/shader_modifiers/core/lightmap.json";

/// A pack whose lightmap writes its `Tint` expression uniform in red, with `modifier` as its
/// `shader_modifiers/core/lightmap.json` when given.
fn pack(modifier: Option<&str>) -> ResourceSource {
    let screenquad = "#version 330\nlayout(location = 0) out vec2 texCoord;\nvoid main() {\n    \
                      vec2 uv = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);\n    \
                      gl_Position = vec4(uv * 2.0 - 1.0, 0.0, 1.0);\n    texCoord = uv;\n}\n";
    let lightmap = "#version 330\nlayout(std140) uniform LightmapInfo {\n    float SkyFactor;\n} \
                    info;\nlayout(std140) uniform Tint {\n    float value;\n};\n\
                    layout(location = 0) in vec2 texCoord;\nlayout(location = 0) out vec4 fragColor;\n\
                    void main() {\n    fragColor = vec4(value, info.SkyFactor, 0.0, 1.0);\n}\n";
    let mut files = vec![(SCREENQUAD, screenquad), (LIGHTMAP, lightmap)];
    files.extend(modifier.map(|m| (MODIFIER, m)));
    ResourceSource::indexed(
        files
            .into_iter()
            .map(|(path, text)| (path.to_owned(), Resource::bytes(text.as_bytes().to_vec())))
            .collect(),
    )
}

/// The game's offscreen renderer, on a hidden window.
fn renderer(game: &mut Game<'_>) {
    let mut builder = winit::event_loop::EventLoop::builder();
    #[cfg(target_os = "linux")]
    {
        use winit::platform::x11::EventLoopBuilderExtX11;
        builder.with_x11().with_any_thread(true);
    }
    let event_loop = builder.build().expect("an event loop");
    #[allow(deprecated)]
    let window = Arc::new(
        event_loop
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(320, 240)),
            )
            .unwrap(),
    );
    game.options.offscreen = true;
    game.renderer = Some(game.new_renderer(window).unwrap());
}

/// The values the renderer's lightmap pipeline holds after one frame the game builds with the
/// packs' `modifier` read as its resources do.
fn after_a_frame(game: &mut Game<'_>, modifier: Option<&str>) -> Vec<(String, f32)> {
    let pack = pack(modifier);
    let gui = crate::game::UiResources::load(&pack, std::path::Path::new("")).unwrap();
    let program = gui.atlas.lightmap_shader.clone();
    let renderer = game.renderer.as_mut().unwrap();
    renderer.set_lightmap_shader(program.as_deref());
    game.pack.shader_modifiers =
        sunrise_client::polytone_post::ShaderModifiers::read(&pack, Default::default())
            .map(Arc::new);
    game.title = None;
    game.render_with(|renderer, frame| renderer.render(frame))
        .unwrap();
    game.renderer
        .as_mut()
        .unwrap()
        .lightmap_expressions()
        .to_vec()
}

#[test]
#[ignore = "daily"]
fn a_frame_gives_the_lightmap_its_expression_uniforms() {
    // A drawn frame needs the client's resources (its prepared textures).
    super::native_entity_refs::tests::with_client_game(|game| {
        renderer(game);
        let values = after_a_frame(game, Some(r#"{"Tint": "0.25 * 2"}"#));
        assert_eq!(values, [("Tint".to_owned(), 0.5)]);
        assert!(game.lightmap_reads_expressions());
        // Fullbright keeps the light texture white: nothing to evaluate.
        game.renderer.as_mut().unwrap().fullbright = true;
        assert!(!game.lightmap_reads_expressions());
        game.renderer.as_mut().unwrap().fullbright = false;
        // The block no pack names: the shader isn't the pack's, and nothing is evaluated.
        assert!(after_a_frame(game, None).is_empty());
        assert!(!game.lightmap_reads_expressions());
    });
}
