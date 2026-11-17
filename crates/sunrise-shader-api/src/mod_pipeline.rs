//! Mod render pipelines' shader, composed and checked without a GPU. A mod's fragment file
//! (`assets/<namespace>/shaders/pipeline/<path>.wgsl`) defines `fn pipeline_fragment(input:
//! PipelineInput) -> vec4<f32>`; [`compose_with`] puts the renderer's prelude before it and its
//! entry points after it, and parses and validates the whole shader. The game builds its
//! pipelines from the result and `cargo sunrise-mod check` runs the same code, so a file is
//! refused with the same message in both.
//!
//! Colour follows the mod render colour rule (docs/design/mod-api/mod-render-colour.md):
//! the mod's function reads and returns linear light, and the generated entry points convert
//! from the quad's vanilla-space colour and to the frame, which stores vanilla's 2.2 encoding
//! in every format the renderer draws today.
use naga::valid::{Capabilities, ValidationFlags, Validator};

/// The folder under each namespace's `shaders` the fragment files are read from.
pub const FOLDER: &str = "shaders/pipeline";

/// The function a pipeline's file defines.
const FUNCTION: &str = "pipeline_fragment";

/// What a pipeline's file can use, before it in the shader. Names starting `sunrise_` are the
/// renderer's own.
const PRELUDE_TYPES: &str = "\
// What `pipeline_fragment` is given for each fragment.
struct PipelineInput {
    // The quad's texture coordinate here, as its corners gave it.
    uv: vec2<f32>,
    // The quad's colour in linear light; alpha is coverage.
    color: vec4<f32>,
    // This point in blocks, relative to the camera.
    position: vec3<f32>,
}
// The frame's values, the same for every fragment.
struct PipelineFrame {
    // The world picture's size in pixels.
    viewport: vec2<f32>,
    // Seconds of game time, smooth between ticks, wrapping to 0 every hour.
    time: f32,
}
@group(0) @binding(0) var<uniform> sunrise_pipeline_view: mat4x4<f32>;
@group(0) @binding(1) var<uniform> frame: PipelineFrame;
";

/// What a textured pipeline's file can use as well (`RenderPipeline::textured`): its texture,
/// bound by the renderer in group 1, and the function that samples it.
const TEXTURE_PRELUDE: &str = "\
@group(1) @binding(0) var sunrise_pipeline_texture: texture_2d<f32>;
@group(1) @binding(1) var sunrise_pipeline_sampler: sampler;
// Where the sprite lies in the atlas: [u0, v0, u1, v1].
@group(1) @binding(2) var<uniform> sunrise_pipeline_rect: vec4<f32>;
// The pipeline's sprite at `uv`: 0 to 1 spans it once and beyond that it repeats. One texel by
// integer coordinates inside the sprite's rect at mip 0 (`repeat_texel` is the same arithmetic),
// in linear light, with its alpha.
fn pipeline_texture(uv: vec2<f32>) -> vec4<f32> {
    let size = vec2<f32>(textureDimensions(sunrise_pipeline_texture, 0));
    let origin = round(sunrise_pipeline_rect.xy * size);
    let extent = max(round(sunrise_pipeline_rect.zw * size) - origin, vec2<f32>(1.0));
    let local = clamp(floor((uv - floor(uv)) * extent), vec2<f32>(0.0), extent - 1.0);
    let texel = textureLoad(sunrise_pipeline_texture, vec2<i32>(origin + local), 0);
    return vec4<f32>(sunrise_decode(texel.rgb), texel.a);
}
";

/// `pipeline_texture`'s texel arithmetic, for testing without a GPU: the integer texel of an
/// atlas `size` texels wide and high that `uv` picks inside the sprite at `rect` (`[u0, v0, u1,
/// v1]`). The fraction of `uv` (negative values wrap) spans the rect once; the texel is always
/// one of the rect's own, its far edge excluded.
pub fn repeat_texel(uv: [f32; 2], rect: [f32; 4], size: [u32; 2]) -> [u32; 2] {
    std::array::from_fn(|axis| {
        let size = size[axis] as f32;
        let origin = (rect[axis] * size).round();
        let extent = ((rect[axis + 2] * size).round() - origin).max(1.0);
        let f = uv[axis] - uv[axis].floor();
        let local = (f * extent).floor().clamp(0.0, extent - 1.0);
        (origin + local) as u32
    })
}

/// The entry points, after the pipeline's file: the vertex stage, and a fragment stage that
/// hands the mod linear light and stores its answer as the frame holds colour.
const ENTRIES: &str = "
struct SunrisePipelineVertex {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) position: vec3<f32>,
}
@vertex fn sunrise_vs_pipeline(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>) -> SunrisePipelineVertex {
    var out: SunrisePipelineVertex;
    out.clip = sunrise_pipeline_view * vec4<f32>(position, 1.0);
    out.uv = uv;
    out.color = color;
    out.position = position;
    return out;
}
@fragment fn sunrise_fs_pipeline(in: SunrisePipelineVertex) -> @location(0) vec4<f32> {
    let result = pipeline_fragment(PipelineInput(in.uv, sunrise_decode_rgba(in.color), in.position));
    // Coverage only: no light rides in the frame's alpha, and NaN covers nothing.
    let coverage = clamp(select(0.0, result.a, result.a > 0.0), 0.0, 1.0);
    return vec4<f32>(sunrise_encode(result.rgb), coverage);
}
";

/// The renderer's entry points in a built pipeline's shader.
pub const VERTEX_ENTRY: &str = "sunrise_vs_pipeline";
pub const FRAGMENT_ENTRY: &str = "sunrise_fs_pipeline";

/// The prelude a pipeline's file follows.
fn prelude(textured: bool) -> String {
    format!(
        "{PRELUDE_TYPES}{}{}",
        if textured { TEXTURE_PRELUDE } else { "" },
        crate::stages::COLOR_HELPERS
    )
}

/// The whole shader of an untextured pipeline whose file is `text` ([`compose_with`]).
pub fn compose(text: &str) -> Result<String, String> {
    compose_with(text, false)
}

/// The whole shader of a pipeline whose file is `text`, with `pipeline_texture` when it is
/// `textured`, or why the file is refused, with the file's own line where there is one.
pub fn compose_with(text: &str, textured: bool) -> Result<String, String> {
    let prelude = prelude(textured);
    let mut shader = format!("{prelude}{text}\n");
    let located = |at: Option<usize>, message: String| match at
        .and_then(|at| at.checked_sub(prelude.len()))
        .filter(|at| *at <= text.len())
    {
        Some(at) => format!(
            "line {}: {message}",
            text.as_bytes()[..at]
                .iter()
                .filter(|b| **b == b'\n')
                .count()
                + 1
        ),
        None => message,
    };
    // First without the entry points, so a missing or mistyped function is reported as such
    // rather than as a call that doesn't resolve.
    let module = naga::front::wgsl::parse_str(&shader).map_err(|e| {
        let at = e
            .labels()
            .next()
            .and_then(|(span, _)| span.to_range())
            .map(|range| range.start);
        let mut message = e.message().to_owned();
        if !textured && text.contains("pipeline_texture") {
            message += " (`pipeline_texture` needs a texture: RenderPipeline::textured)";
        }
        located(at, message)
    })?;
    for (_, function) in module.functions.iter() {
        if function.expressions.iter().any(|(_, expression)| matches!(expression,
            naga::Expression::GlobalVariable(handle) if module.global_variables[*handle].name.as_deref() == Some("sunrise_pipeline_view"))) {
            return Err("sunrise_pipeline_view is vertex-only".into());
        }
    }
    if !module.overrides.is_empty() {
        return Err("override declarations are not supported".into());
    }
    signature(&module)?;
    if !module.entry_points.is_empty() {
        return Err("it declares an entry point; the game supplies both stages".into());
    }
    if module
        .global_variables
        .iter()
        .filter(|(_, variable)| variable.binding.is_some())
        .count()
        != if textured { 5 } else { 2 }
    {
        return Err("it declares a binding; a pipeline reads only `frame` and its input".into());
    }
    shader.push_str(ENTRIES);
    let module = naga::front::wgsl::parse_str(&shader).map_err(|e| e.message().to_owned())?;
    Validator::new(ValidationFlags::all(), Capabilities::default())
        .validate(&module)
        .map_err(|e| {
            let at = e
                .spans()
                .next()
                .and_then(|(span, _)| span.to_range())
                .map(|range| range.start);
            let mut message = e.as_inner().to_string();
            let mut source = std::error::Error::source(e.as_inner());
            while let Some(inner) = source {
                message += &format!(": {inner}");
                source = inner.source();
            }
            located(at, message)
        })?;
    Ok(shader)
}

/// `fn pipeline_fragment(input: PipelineInput) -> vec4<f32>`, or what the file has instead.
fn signature(module: &naga::Module) -> Result<(), String> {
    let expected = format!("expected `fn {FUNCTION}(input: PipelineInput) -> vec4<f32>`");
    let (_, function) = module
        .functions
        .iter()
        .find(|(_, function)| function.name.as_deref() == Some(FUNCTION))
        .ok_or_else(|| format!("no function `{FUNCTION}`: {expected}"))?;
    let takes_input = matches!(
        function.arguments.as_slice(),
        [argument] if module.types[argument.ty].name.as_deref() == Some("PipelineInput")
    );
    let returns_color = function.result.as_ref().is_some_and(|result| {
        matches!(
            module.types[result.ty].inner,
            naga::TypeInner::Vector {
                size: naga::VectorSize::Quad,
                scalar: naga::Scalar::F32,
            }
        )
    });
    if takes_input && returns_color {
        Ok(())
    } else {
        Err(format!("`{FUNCTION}` has another signature: {expected}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_cannot_read_vertex_uniform() {
        assert!(compose("fn pipeline_fragment(input: PipelineInput) -> vec4<f32> { return sunrise_pipeline_view * input.color; }").unwrap_err().contains("sunrise_pipeline_view"));
    }

    #[test]
    fn overrides_without_defaults_are_refused() {
        assert!(compose("override strength: f32; fn pipeline_fragment(input: PipelineInput) -> vec4<f32> { return input.color * strength; }").unwrap_err().contains("override"));
    }

    const GLOW: &str = "\
fn pipeline_fragment(input: PipelineInput) -> vec4<f32> {
    return vec4<f32>(input.color.rgb * (1.0 + sin(frame.time)), input.color.a * input.uv.y);
}
";

    #[test]
    fn a_file_with_the_function_composes_into_a_shader_with_both_entry_points() {
        let shader = compose(GLOW).unwrap();
        let module = naga::front::wgsl::parse_str(&shader).unwrap();
        let entries: Vec<_> = module
            .entry_points
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(entries, [VERTEX_ENTRY, FRAGMENT_ENTRY]);
    }

    #[test]
    fn a_refused_file_says_what_is_wrong_and_where() {
        let refused = |text: &str| compose(text).unwrap_err();
        assert!(refused("").starts_with("no function `pipeline_fragment`"));
        assert!(
            refused("fn pipeline_fragment(uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(1.0); }")
                .contains("another signature")
        );
        assert!(
            refused(
                "fn pipeline_fragment(input: PipelineInput) -> vec3<f32> { return input.position; }"
            )
            .contains("another signature")
        );
        // A syntax error is reported on the file's own line, not the composed shader's.
        let broken = "// a comment\nfn pipeline_fragment(input: PipelineInput) -> vec4<f32> {\n    return vec4<f32>(1.0)\n}\n";
        let message = refused(broken);
        assert!(
            message.starts_with("line 3:") || message.starts_with("line 4:"),
            "{message}"
        );
        // A type error is found by validation, with the file's line too.
        let mistyped = "fn pipeline_fragment(input: PipelineInput) -> vec4<f32> {\n    let x: f32 = input.uv;\n    return vec4<f32>(x);\n}\n";
        assert!(
            refused(mistyped).starts_with("line 2:"),
            "{}",
            refused(mistyped)
        );
        assert!(
            refused(&format!(
                "@group(0) @binding(2) var<uniform> mine: vec4<f32>;\n{GLOW}"
            ))
            .contains("declares a binding")
        );
        assert!(
            refused(&format!(
                "{GLOW}@fragment fn mine() -> @location(0) vec4<f32> {{ return vec4<f32>(1.0); }}\n"
            ))
            .contains("declares an entry point")
        );
    }

    /// A textured pipeline's file can call `pipeline_texture`; an untextured one is told why
    /// it can't, and neither may declare a binding of its own.
    #[test]
    fn only_a_textured_pipeline_samples_and_neither_binds_its_own() {
        let sample = "fn pipeline_fragment(input: PipelineInput) -> vec4<f32> { return pipeline_texture(input.uv) * input.color; }";
        let shader = compose_with(sample, true).unwrap();
        assert!(shader.contains("@group(1) @binding(0) var sunrise_pipeline_texture"));
        assert!(
            compose(sample)
                .unwrap_err()
                .contains("RenderPipeline::textured")
        );
        let own = "@group(1) @binding(3) var<uniform> mine: f32;
fn pipeline_fragment(input: PipelineInput) -> vec4<f32> { return pipeline_texture(input.uv) * mine; }";
        assert!(
            compose_with(own, true)
                .unwrap_err()
                .contains("declares a binding")
        );
        // Untextured pipelines compose as before: no group 1.
        assert!(!compose(GLOW).unwrap().contains("@group(1)"));
    }

    /// `pipeline_texture`'s texel arithmetic: `uv` 0 to 1 spans the sprite's rect once, the far
    /// edge and every repeat (negative too) land back inside it, never on a neighbour's texel.
    #[test]
    fn the_repeat_picks_integer_texels_inside_the_rect_for_any_uv() {
        // A 16 x 16 sprite at texel (32, 16) of a 64 x 64 atlas.
        let rect = [0.5, 0.25, 0.75, 0.5];
        let at = |u: f32, v: f32| repeat_texel([u, v], rect, [64, 64]);
        assert_eq!(at(0.0, 0.0), [32, 16], "the near corner");
        assert_eq!(at(0.5, 0.5), [40, 24]);
        assert_eq!(
            at(0.999_999, 0.999_999),
            [47, 31],
            "just inside the far edge"
        );
        assert_eq!(at(1.0, 1.0), [32, 16], "the far edge repeats to the start");
        assert_eq!(at(2.5, 3.0625), [40, 17], "repeated");
        assert_eq!(
            at(-0.0625, -0.25),
            [47, 28],
            "negative wraps from the far side"
        );
        assert_eq!(at(-1.0, -2.0), [32, 16]);
        // So small a negative that its fraction rounds to 1.0 still stays inside.
        assert_eq!(at(-1.0e-9, -1.0e-9), [47, 31]);
        assert_eq!(at(-1000.5, 1000.75), [40, 28]);
        // The 2x1 atlas's right half: always its one texel.
        for u in [0.0, 0.25, 0.999, 1.0, -0.5, 7.75] {
            assert_eq!(
                repeat_texel([u, u], [0.5, 0.0, 1.0, 1.0], [2, 1]),
                [1, 0],
                "{u}"
            );
        }
        // The left half: never the right's.
        for u in [0.0, 0.999, -0.001, 3.5] {
            assert_eq!(
                repeat_texel([u, 0.0], [0.0, 0.0, 0.5, 1.0], [2, 1]),
                [0, 0],
                "{u}"
            );
        }
    }
}
