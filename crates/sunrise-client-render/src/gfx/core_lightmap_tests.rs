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

fn device() -> (wgpu::Device, wgpu::Queue) {
    let (_, device, queue) = crate::gfx::test_gpu::optional_device().expect("a GPU");
    (device, queue)
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
    queue.write_buffer(&core.uniform, 0, &CoreLightmap::uniforms([0; 48], info));
    let mut encoder = device.create_command_encoder(&Default::default());
    core.draw(&mut encoder, &target);
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
#[ignore = "requires a Vulkan/Metal/DX12 adapter"]
fn a_pack_lightmap_shader_fills_the_texture_by_sky_row_and_block_column() {
    let _gpu = crate::gfx::test_gpu::lock();
    let (device, queue) = device();
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
    let pixels = draw(&device, &queue, &mut core, &STATES[1]);
    for sky in 0..16usize {
        for block in 0..16usize {
            let got = &pixels[(sky * 16 + block) * 4..][..4];
            let want = [block as u8 * 17, sky as u8 * 17, 179, 153];
            assert!(
                got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 1),
                "sky {sky} block {block}: {got:?}, want {want:?}"
            );
        }
    }
    // Frames read back out of their buffers' order: the newest one wins. P lands in buffer 0
    // and is taken in; Q goes to buffer 1, R to buffer 0 again, and both land before F3 looks.
    let frame = |core: &mut CoreLightmap, info: &LightmapInfo, poll: bool| {
        queue.write_buffer(&core.uniform, 0, &CoreLightmap::uniforms([0; 48], info));
        let mut encoder = device.create_command_encoder(&Default::default());
        core.draw(&mut encoder, &target(&device));
        queue.submit([encoder.finish()]);
        (core.submitted().expect("a readback"))();
        if poll {
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        }
    };
    let mut pixels = [0u8; 16 * 16 * 4];
    frame(&mut core, &STATES[0], true);
    frame(&mut core, &STATES[1], false);
    assert!(core.collect(&mut pixels), "P");
    frame(&mut core, &STATES[0], true);
    assert!(core.collect(&mut pixels));
    assert_eq!(pixels[2], 0, "R's SkyFactor, not Q's");
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

/// Vanilla's own `screenquad` and `lightmap` through the pack path draw the texture the
/// native path computes, row by sky level and column by block level, in two lighting states.
#[test]
#[ignore = "requires a Vulkan/Metal/DX12 adapter"]
fn vanillas_lightmap_shaders_draw_the_native_lightmap() {
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
    let (device, queue) = device();
    let mut core = CoreLightmap::new(&device, &program).expect("the lightmap pipeline");
    for info in STATES {
        let gpu = draw(&device, &queue, &mut core, &info);
        let native = native_lightmap(&info);
        for sky in 0..16 {
            for block in 0..16 {
                let i = (sky * 16 + block) * 4;
                let (n, g) = (&native[i..i + 4], &gpu[i..i + 4]);
                // The native path truncates, the GPU rounds.
                assert!(
                    n.iter().zip(g).all(|(a, b)| a.abs_diff(*b) <= 1),
                    "sky {sky} block {block}: native {n:?} shader {g:?}"
                );
            }
        }
    }
}

/// A shader reading a block the lightmap pass doesn't give is not used: the texture stays
/// native, rather than failing at the draw.
#[test]
#[ignore = "requires a Vulkan/Metal/DX12 adapter"]
fn a_lightmap_shader_reading_another_block_is_not_used() {
    let _gpu = crate::gfx::test_gpu::lock();
    let (device, _) = device();
    let fragment = PROBE_FSH
        .replace("lightmapInfo.SkyFactor", "other.Value")
        .replace(
            "} lightmapInfo;\n",
            "} lightmapInfo;\nlayout(std140) uniform Other {\n    float Value;\n} other;\n",
        );
    let program = crate::core_shader::translate_core_with(
        SCREENQUAD_VSH,
        &fragment,
        &[],
        &|_| None,
        &GLUE,
        &|b| b == INFO_BLOCK || b == "Other",
    )
    .expect("the shaders translate");
    assert!(CoreLightmap::new(&device, &program).is_none());
}
