use super::*;

/// Two states: a dim, tinted End-like one (green ambient, no sky light) and a night with
/// night vision, a boss bar and some darkness.
const STATES: [LightmapInfo; 2] = [
    LightmapInfo {
        sky_factor: 0.0,
        block_factor: 1.4,
        night_vision: 0.0,
        darkness: 0.0,
        boss_darkening: 0.0,
        brightness: 1.0,
        block_light_tint: [1.0, 0.847, 0.549],
        sky_light_color: [0.675, 0.376, 0.804],
        ambient_color: [0.247, 0.278, 0.247],
        night_vision_color: [0.6; 3],
    },
    LightmapInfo {
        sky_factor: 0.7,
        block_factor: 1.45,
        night_vision: 0.3,
        darkness: 0.1,
        boss_darkening: 0.5,
        brightness: 0.5,
        block_light_tint: [1.0, 0.6, 0.3],
        sky_light_color: [0.8, 0.8, 1.0],
        ambient_color: [0.04; 3],
        night_vision_color: [0.6; 3],
    },
];

/// The probe shader's state: values a unorm byte holds exactly (0.6 is 153 of 255, 0.4 is 102).
const PROBE: LightmapInfo = LightmapInfo {
    sky_factor: 0.6,
    night_vision_color: [0.0, 0.4, 0.0],
    ..STATES[1]
};

/// `LightmapInfo` as `lightmap.fsh` declares it in std140: six floats, then four vec3s each on
/// its own 16 bytes.
#[test]
fn the_lightmap_info_block_is_laid_out_as_std140() {
    let bytes = STATES[1].std140();
    let f: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    assert_eq!(f.len(), 24);
    assert_eq!(f[..6], [0.7, 1.45, 0.3, 0.1, 0.5, 0.5]);
    assert_eq!(f[8..11], [1.0, 0.6, 0.3]);
    assert_eq!(f[12..15], [0.8, 0.8, 1.0]);
    assert_eq!(f[16..19], [0.04; 3]);
    assert_eq!(f[20..23], [0.6; 3]);
    assert_eq!([f[6], f[7], f[11], f[15], f[19], f[23]], [0.0; 6]);
}

/// The adapter's device, or none (said on stderr) where the machine has no adapter.
fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let device = crate::gfx::test_gpu::optional_device();
    if device.is_none() {
        eprintln!("skipped: no adapter");
    }
    device.map(|(_, device, queue)| (device, queue))
}

/// A 16x16 light texture to draw into.
fn target(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// The light texture `core` draws for `info`, read back through its own readback (the copy,
/// the mapping after the submission, then `collect`): RGBA rows of sky level by block level.
fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    core: &mut CoreLightmap,
    info: &LightmapInfo,
) -> [u8; 16 * 16 * 4] {
    let target = target(device);
    let (buffer, data) = core.uniforms([0; 48], info);
    queue.write_buffer(buffer, 0, data);
    let mut encoder = device.create_command_encoder(&Default::default());
    core.draw(
        &mut encoder,
        &target,
        &target.create_view(&Default::default()),
    );
    queue.submit([encoder.finish()]);
    (core.submitted().expect("a readback"))();
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let mut pixels = [0u8; 16 * 16 * 4];
    assert!(core.collect(&mut pixels), "read back");
    pixels
}

/// A pack's own shader: each texel's block level in red, its sky level in green, `SkyFactor`
/// in blue and `NightVisionColor`'s green in alpha.
const PROBE_FSH: &str = "#version 330\nlayout(std140) uniform LightmapInfo {\n\
    float SkyFactor;\n    float BlockFactor;\n    float NightVisionFactor;\n\
    float DarknessScale;\n    float BossOverlayWorldDarkeningFactor;\n    float BrightnessFactor;\n\
    vec3 BlockLightTint;\n    vec3 SkyLightColor;\n    vec3 AmbientColor;\n    vec3 NightVisionColor;\n\
} lightmapInfo;\n\
    layout(location = 0) in vec2 texCoord;\nlayout(location = 0) out vec4 fragColor;\n\
    void main() {\n    float block_level = floor(texCoord.x * 16) / 15;\n\
        float sky_level = floor(texCoord.y * 16) / 15;\n\
        fragColor = vec4(block_level, sky_level, lightmapInfo.SkyFactor, \
        lightmapInfo.NightVisionColor.g);\n}\n";

const SCREENQUAD_VSH: &str = "#version 330\nlayout(location = 0) out vec2 texCoord;\n\
    void main() {\n    vec2 uv = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);\n\
        gl_Position = vec4(uv * vec2(2, 2) + vec2(-1, -1), 0, 1);\n    texCoord = uv;\n}\n";

/// The pack's shader covers the whole texture with row `n` at sky level `n` and column `n` at
/// block level `n` (GL's rows run from the bottom), reads `LightmapInfo` where `Lightmap.render`
/// writes it, and its pixels come back for F3's lightmap chart.
#[test]
#[ignore = "daily"]
fn a_pack_lightmap_shader_fills_the_texture_by_sky_row_and_block_column() {
    let _gpu = crate::gfx::test_gpu::lock();
    let Some((device, queue)) = device() else {
        return;
    };
    let program = crate::core_shader::translate_core_with(
        SCREENQUAD_VSH,
        PROBE_FSH,
        &[],
        &|_| None,
        &GLUE,
        &|b| b == INFO_BLOCK,
    )
    .expect("the shaders translate");
    let mut core = CoreLightmap::new(&device, &program).expect("the lightmap pipeline");
    core.set_readback(&device, true);
    let pixels = draw(&device, &queue, &mut core, &PROBE);
    for sky in 0..16usize {
        for block in 0..16usize {
            let got = &pixels[(sky * 16 + block) * 4..][..4];
            // Level n of 15 is byte 17n; `SkyFactor` 0.6 is 153, `NightVisionColor.g` 0.4 is 102.
            let want = [block as u8 * 17, sky as u8 * 17, 153, 102];
            assert_eq!(got, want, "sky {sky} block {block}");
        }
    }
    // Frames read back out of their buffers' order: the newest one wins. P lands in buffer 0
    // and is taken in; Q goes to buffer 1, R to buffer 0 again, and both land before F3 looks.
    let frame = |core: &mut CoreLightmap, info: &LightmapInfo, poll: bool| {
        let (buffer, data) = core.uniforms([0; 48], info);
        queue.write_buffer(buffer, 0, data);
        let mut encoder = device.create_command_encoder(&Default::default());
        let target = target(&device);
        core.draw(
            &mut encoder,
            &target,
            &target.create_view(&Default::default()),
        );
        queue.submit([encoder.finish()]);
        (core.submitted().expect("a readback"))();
        if poll {
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        }
    };
    let mut pixels = [0u8; 16 * 16 * 4];
    frame(&mut core, &STATES[0], true);
    frame(&mut core, &PROBE, false);
    assert!(core.collect(&mut pixels), "P");
    frame(&mut core, &STATES[0], true);
    assert!(core.collect(&mut pixels));
    assert_eq!(pixels[2], 0, "R's SkyFactor (0), not Q's (153)");
}

fn resources() -> std::path::PathBuf {
    std::env::var_os("SUNRISE_RESOURCES")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../.local/sunrise-client")
                .join(sunrise_core::VANILLA_VERSION)
        })
}

/// Vanilla's own `screenquad` and `lightmap` through the pack path draw vanilla's light texture
/// in two lighting states. The expected bytes are not Sunrise's: they come from
/// `lightmap.fsh`'s math run line by line in float32 by an independent script (numpy, written
/// for this test; rounded to the nearest byte as a unorm target stores them), at texels whose
/// every channel is at least a tenth of a byte from a rounding tie, row by sky level and column
/// by block level.
#[test]
#[ignore = "daily"]
fn vanillas_lightmap_shaders_draw_vanillas_light_texture() {
    let _gpu = crate::gfx::test_gpu::lock();
    let root = crate::resource_source::ResourceSource::directory(resources());
    let shader = |name: &str| root.read_to_string(&format!("assets/minecraft/shaders/core/{name}"));
    let (Ok(vertex), Ok(fragment)) = (shader("screenquad.vsh"), shader("lightmap.fsh")) else {
        eprintln!(
            "skipped: no prepared resources at {}",
            resources().display()
        );
        return;
    };
    let include = |id: &str| {
        let (namespace, path) = id.split_once(':')?;
        root.read_to_string(&format!("assets/{namespace}/{path}"))
            .ok()
    };
    let program =
        crate::core_shader::translate_core_with(&vertex, &fragment, &[], &include, &GLUE, &|b| {
            b == INFO_BLOCK
        })
        .expect("vanilla's lightmap shaders translate");
    let Some((device, queue)) = device() else {
        return;
    };
    let mut core = CoreLightmap::new(&device, &program).expect("the lightmap pipeline");
    core.set_readback(&device, true);
    // (sky level, block level, RGBA) per state.
    let expected: [&[(usize, usize, [u8; 4])]; 2] = [
        &[
            (0, 0, [165, 186, 165, 255]),
            (0, 15, [255, 255, 255, 255]),
            (15, 0, [165, 186, 165, 255]),
            (15, 15, [255, 255, 255, 255]),
            (7, 7, [239, 235, 185, 255]),
            (8, 1, [175, 194, 173, 255]),
            (1, 8, [245, 238, 184, 255]),
        ],
        &[
            (0, 0, [32, 26, 26, 255]),
            (0, 15, [255, 255, 255, 255]),
            (15, 0, [176, 164, 201, 255]),
            (15, 15, [255, 255, 255, 255]),
            (1, 8, [145, 90, 57, 255]),
            (10, 10, [204, 145, 123, 255]),
        ],
    ];
    for (info, texels) in STATES.iter().zip(expected) {
        let gpu = draw(&device, &queue, &mut core, info);
        for &(sky, block, want) in texels {
            let i = (sky * 16 + block) * 4;
            assert_eq!(gpu[i..i + 4], want, "sky {sky} block {block}");
        }
    }
}

/// An expression uniform is one float (wiki "Shaders"): a shader declaring the block larger
/// is not used, and the texture stays native, rather than failing at the draw. (A block no
/// pack's `shader_modifiers` gives is refused earlier, when the shader loads.)
#[test]
#[ignore = "daily"]
fn a_lightmap_shader_whose_expression_block_is_more_than_a_float_is_not_used() {
    let _gpu = crate::gfx::test_gpu::lock();
    let Some((device, _)) = device() else {
        return;
    };
    let fragment = |block: &str| {
        PROBE_FSH
            .replace("lightmapInfo.SkyFactor", "other.Value.x")
            .replace(
                "} lightmapInfo;\n",
                &format!("}} lightmapInfo;\nlayout(std140) uniform Other {{\n{block}}} other;\n"),
            )
    };
    let program = |block: &str| {
        crate::core_shader::translate_core_with(
            SCREENQUAD_VSH,
            &fragment(block),
            &[],
            &|_| None,
            &GLUE,
            &|b| b == INFO_BLOCK || b == "Other",
        )
        .expect("the shaders translate")
    };
    // One vec4 (16 bytes) fits the block's binding; two don't.
    assert!(CoreLightmap::new(&device, &program("    vec4 Value;\n")).is_some());
    let program = program("    vec4 Value;\n    vec4 More;\n");
    assert!(CoreLightmap::new(&device, &program).is_none());
}

/// F3's lightmap chart closed (the default), a frame draws the texture and copies nothing:
/// no readback buffer exists and nothing is mapped after the submission. Opened, the frame's
/// texture comes back; closed again, the buffers go, and a chart opened later starts from new
/// buffers rather than an old frame's copy.
#[test]
#[ignore = "daily"]
fn the_light_texture_is_read_back_only_while_f3_charts_it() {
    let _gpu = crate::gfx::test_gpu::lock();
    let Some((device, queue)) = device() else {
        return;
    };
    let program = crate::core_shader::translate_core_with(
        SCREENQUAD_VSH,
        PROBE_FSH,
        &[],
        &|_| None,
        &GLUE,
        &|b| b == INFO_BLOCK,
    )
    .expect("the shaders translate");
    let mut core = CoreLightmap::new(&device, &program).expect("the lightmap pipeline");
    // One frame: its uniforms, its draw, its submission, and the readback it starts (if any).
    let frame = |core: &mut CoreLightmap, info: &LightmapInfo| {
        let (buffer, data) = core.uniforms([0; 48], info);
        queue.write_buffer(buffer, 0, data);
        let mut encoder = device.create_command_encoder(&Default::default());
        let target = target(&device);
        core.draw(
            &mut encoder,
            &target,
            &target.create_view(&Default::default()),
        );
        queue.submit([encoder.finish()]);
        let started = core.submitted().map(|read| read()).is_some();
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        started
    };
    assert!(core.readback.is_none(), "closed when made");
    assert!(!frame(&mut core, &PROBE), "no copy while closed");
    let mut pixels = [7u8; 16 * 16 * 4];
    assert!(!core.collect(&mut pixels));
    assert_eq!(pixels, [7; 16 * 16 * 4], "nothing read while closed");

    core.set_readback(&device, true);
    assert!(frame(&mut core, &PROBE), "a copy while open");
    assert!(core.collect(&mut pixels));
    // PROBE's SkyFactor (0.6, 153 of 255) in blue.
    assert_eq!(pixels[2], 153);

    // A copy is mapped and waiting when the chart closes: the buffers go with it.
    assert!(frame(&mut core, &STATES[0]));
    core.set_readback(&device, false);
    assert!(core.readback.is_none());
    assert!(!frame(&mut core, &STATES[0]), "no copy once closed");
    core.set_readback(&device, true);
    let mut fresh = [7u8; 16 * 16 * 4];
    assert!(
        !core.collect(&mut fresh),
        "the copy from before the close is gone"
    );
    assert_eq!(fresh, [7; 16 * 16 * 4]);
}

/// A pack's shader reading `n` expression uniforms (`E0`, `E1`, ...): the first one's value in
/// red, `SkyFactor` in green.
fn expression_shader(n: usize) -> CoreProgram {
    let blocks: String = (0..n)
        .map(|i| format!("layout(std140) uniform E{i} {{\n    float e{i};\n}};\n"))
        .collect();
    let reads: String = (1..n).map(|i| format!(" + 0.0 * e{i}")).collect();
    let fragment = format!(
        "#version 330\nlayout(std140) uniform LightmapInfo {{\n    float SkyFactor;\n}} info;\n\
         {blocks}layout(location = 0) in vec2 texCoord;\nlayout(location = 0) out vec4 fragColor;\n\
         void main() {{\n    fragColor = vec4(e0{reads}, info.SkyFactor, 0.0, 1.0);\n}}\n"
    );
    let names: Vec<String> = (0..n).map(|i| format!("E{i}")).collect();
    crate::core_shader::translate_core_with(
        SCREENQUAD_VSH,
        &fragment,
        &[],
        &|_| None,
        &GLUE,
        &|b| b == INFO_BLOCK || names.iter().any(|n| n == b),
    )
    .expect("the shaders translate")
}

/// The expression uniforms' values the game gives (`Renderer::lightmap_expressions`) reach the
/// shader's blocks each frame, beside `LightmapInfo`.
#[test]
#[ignore = "daily"]
fn a_lightmap_shader_reads_its_expression_uniforms_values() {
    let _gpu = crate::gfx::test_gpu::lock();
    let Some((device, queue)) = device() else {
        return;
    };
    let mut core = CoreLightmap::new(&device, &expression_shader(2)).expect("the pipeline");
    core.set_readback(&device, true);
    assert_eq!(
        core.expressions,
        [("E0".to_owned(), 0.0), ("E1".to_owned(), 0.0)]
    );
    for (value, red) in [(0.75, 191), (0.25, 64)] {
        core.expressions[0].1 = value;
        let pixels = draw(&device, &queue, &mut core, &STATES[1]);
        assert!(pixels[0].abs_diff(red) <= 1, "{value}: {}", pixels[0]);
        assert!(pixels[1].abs_diff(179) <= 1, "SkyFactor: {}", pixels[1]);
    }
}

/// A device binds at most `max_uniform_buffers_per_shader_stage` blocks to a shader (12 by
/// default), five of them the lightmap's own: seven expression uniforms are drawn, eight
/// refused when the pipeline is built, so the texture stays native from the first frame.
#[test]
#[ignore = "daily"]
fn a_lightmap_shader_reading_more_expression_uniforms_than_the_device_binds_is_refused() {
    let _gpu = crate::gfx::test_gpu::lock();
    let Some((device, _)) = device() else {
        return;
    };
    assert_eq!(device.limits().max_uniform_buffers_per_shader_stage, 12);
    assert!(CoreLightmap::new(&device, &expression_shader(7)).is_some());
    assert!(CoreLightmap::new(&device, &expression_shader(8)).is_none());
}

/// What F3's lightmap chart paints across the chart closing and opening again and fullbright
/// turning on and off (the renderer's `lightmap_sample`): only pixels of the texture drawn now,
/// never an old sample. No adapter needed: the chart's state alone.
#[test]
fn the_lightmap_chart_never_shows_a_stale_sample() {
    let mut chart = LightmapChart::new();
    let pack = Drawn::now(false, true);
    let (native, white) = (Drawn::now(false, false), Drawn::now(true, true));
    assert_eq!(
        (pack, native, white),
        (Drawn::Pack, Drawn::Native, Drawn::Native)
    );
    assert_eq!(chart.shown(pack), None, "nothing read yet");
    // The chart open: a pack-drawn copy lands.
    chart.pixels = [9; 16 * 16 * 4];
    chart.pack_copied();
    assert_eq!(chart.shown(pack), Some(&[9; 16 * 16 * 4]));
    // Closed, then opened again: nothing until a fresh copy.
    chart.forget_pack();
    assert_eq!(chart.shown(pack), None);
    chart.pixels = [10; 16 * 16 * 4];
    chart.pack_copied();
    assert_eq!(chart.shown(pack), Some(&[10; 16 * 16 * 4]));
    // Fullbright on: the pack's copy is not the white texture drawn now, until the native path
    // writes it.
    assert_eq!(chart.shown(white), None);
    chart.native([255; 16 * 16 * 4]);
    assert_eq!(chart.shown(white), Some(&[255; 16 * 16 * 4]));
    // Fullbright off with the pack's shader: the white pixels are not its texture.
    assert_eq!(chart.shown(pack), None);
    // Closing the chart leaves a native sample alone: it is current.
    chart.forget_pack();
    assert_eq!(chart.shown(native), Some(&[255; 16 * 16 * 4]));
}
