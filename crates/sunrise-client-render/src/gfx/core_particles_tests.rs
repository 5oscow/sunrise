use super::*;
use crate::particles::polytone_gpu::GLUE;
use glam::Mat4;
use sunrise_polytone::gpu_particle::{GPU_PARTICLE_GLSL, GPU_PARTICLE_INCLUDE};

const INCLUDES: [(&str, &str); 2] = [
    (
        "minecraft:shaders/include/dynamictransforms.glsl",
        "layout(std140) uniform DynamicTransforms {\n    mat4 ModelViewMat;\n    mat4 TextureMat;\n    vec4 ColorModulator;\n    vec3 ModelOffset;\n};\n",
    ),
    (
        "minecraft:shaders/include/projection.glsl",
        "layout(std140) uniform Projection {\n    mat4 ProjMat;\n};\n",
    ),
];

fn include(id: &str) -> Option<String> {
    if id == GPU_PARTICLE_INCLUDE {
        return Some(GPU_PARTICLE_GLSL.to_owned());
    }
    INCLUDES
        .iter()
        .find(|(path, _)| *path == id)
        .map(|(_, text)| (*text).to_owned())
}

/// A custom shader in the wiki's 1.21.11 form: the record's quad around `Position + Origin`,
/// `Params.x` its half-width, the corner from `gl_VertexIndex & 3`.
const VSH: &str = "#version 330\n#extension GL_ARB_separate_shader_objects : require\n\
    #include <minecraft:dynamictransforms.glsl>\n#include <minecraft:projection.glsl>\n\
    #include <polytone:gpu_particle.glsl>\n\
    layout(location = 0) in vec3 Position;\nlayout(location = 1) in vec3 Velocity;\n\
    layout(location = 2) in vec2 SpawnLife;\nlayout(location = 3) in vec4 Params;\n\
    layout(location = 4) in vec4 Color;\nlayout(location = 5) in ivec2 UV2;\n\
    layout(location = 0) out vec4 vertexColor;\n\
    void main() {\n    int corner = gl_VertexIndex & 3;\n\
        vec2 side = vec2(corner == 1 || corner == 2 ? 1.0 : -1.0, corner >= 2 ? 1.0 : -1.0);\n\
        vec3 at = Position + Origin + vec3(side * Params.x, 0.0);\n\
        gl_Position = ProjMat * ModelViewMat * vec4(at, 1.0);\n\
        vertexColor = Color;\n}\n";

/// The record's colour times the pack uniform `Brightness`, a block of its own.
const FSH: &str = "#version 330\n#extension GL_ARB_separate_shader_objects : require\n\
    layout(std140) uniform Brightness {\n    float brightnessValue;\n};\n\
    layout(location = 0) in vec4 vertexColor;\nlayout(location = 0) out vec4 fragColor;\n\
    void main() {\n    fragColor = vec4(vertexColor.rgb * brightnessValue, 1.0);\n}\n";

/// The test shader, translated as if `Brightness` were given, with the pack uniforms named
/// `uniforms` (each 0.5).
fn shader(uniforms: &[&str]) -> Arc<GpuShader> {
    let provided = |name: &str| matches!(name, GPU_BLOCK | "Brightness");
    let program =
        crate::core_shader::translate_core_with(VSH, FSH, &[], &include, &GLUE, &provided)
            .expect("the shaders translate");
    Arc::new(GpuShader::new(
        "test:particle".to_owned(),
        Arc::new(program),
        None,
        RenderType::Opaque,
        uniforms
            .iter()
            .map(|name| {
                let program = sunrise_polytone::expr::Program::compile("0.5").unwrap();
                ((*name).to_owned(), program)
            })
            .collect(),
    ))
}

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    crate::gfx::test_gpu::optional_device().map(|(_, device, queue)| (device, queue))
}

const SIZE: u32 = 64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// One red record at the batch's `Origin`, 2 blocks ahead of the camera, a block wide, with
/// `Brightness` 0.5 and `copies` copies.
fn batch(shader: Arc<GpuShader>, copies: u32) -> ShaderBatch {
    let mut block = [0u8; GPU_BLOCK_SIZE];
    block[..12].copy_from_slice(bytemuck::cast_slice(&[0.0f32, 0.0, -2.0]));
    let mut vertex: ShaderVertex = bytemuck::Zeroable::zeroed();
    vertex.spawn_life = [0.0, 100.0];
    vertex.params = [0.5, 0.0, 0.0, 0.0];
    vertex.color = [255, 0, 0, 255];
    ShaderBatch {
        shader,
        vertices: vec![vertex; 4],
        copies,
        block,
        uniforms: vec![("Brightness".to_owned(), 0.5)],
    }
}

/// Prepares `particles` for one frame of `batches` into a frame of `linear` light.
fn prepare(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    particles: &mut CoreParticles,
    batches: Vec<ShaderBatch>,
    linear: bool,
) {
    let lightmap = texture(device, queue, 1, 1, &[255; 4]).create_view(&Default::default());
    let lightmap_sampler = device.create_sampler(&Default::default());
    let view = super::super::View {
        view: Mat4::IDENTITY,
        projection: super::super::world_projection(70.0, 1.0, 64.0),
        aspect: 1.0,
        tan_half_fov: 35f32.to_radians().tan(),
    };
    particles.batches = batches;
    particles.prepare_frame(
        device,
        queue,
        (FORMAT, linear),
        (&lightmap, &lightmap_sampler),
        false,
        || {
            super::super::core_lines::uniforms(
                [0; 48],
                &view,
                Mat4::IDENTITY,
                Mat4::IDENTITY,
                1.0,
                [0.0; 4],
                [0.0; 6],
            )
        },
    );
}

/// Draws what `particles` prepared on a 64 by 64 target cleared to transparent black, one quad
/// of indices bound: the RGBA rows read back.
fn draw(device: &wgpu::Device, queue: &wgpu::Queue, particles: &CoreParticles) -> Vec<u8> {
    let target = |format, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | usage,
            view_formats: &[],
        })
    };
    let color = target(FORMAT, wgpu::TextureUsages::COPY_SRC);
    let depth = target(
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureUsages::empty(),
    );
    let indices = wgpu::util::DeviceExt::create_buffer_init(
        device,
        &wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[0u32, 1, 2, 0, 2, 3]),
            usage: wgpu::BufferUsages::INDEX,
        },
    );
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(SIZE * SIZE * 4),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let color_view = color.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        particles.draw(&mut pass, &indices, 1);
    }
    encoder.copy_texture_to_buffer(
        color.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &read,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE * 4),
                rows_per_image: Some(SIZE),
            },
        },
        color.size(),
    );
    queue.submit([encoder.finish()]);
    read.slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    read.slice(..).get_mapped_range().unwrap().to_vec()
}

fn at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE + x) * 4) as usize;
    pixels[i..i + 4].try_into().unwrap()
}

/// The record reaches the shader as its vertex format (position, size, colour), the knobs block
/// carries `Origin` and the pack's uniform is its own block: a half-red quad in the middle,
/// nothing at the edge, the frame's alpha (HDR output's marks) left as it was. The frame its
/// pipeline is built in, the type draws its quads instead; from the next one, the shader.
#[test]
#[ignore = "requires a Vulkan/Metal/DX12 adapter"]
fn a_custom_shader_draws_its_records_with_its_uniforms() {
    let _gpu = crate::gfx::test_gpu::lock();
    let (device, queue) = device().expect("a GPU");
    let mut particles = CoreParticles::new(&device, &queue, (FORMAT, false));
    let shader = shader(&["Brightness"]);
    prepare(
        &device,
        &queue,
        &mut particles,
        vec![batch(shader.clone(), 1)],
        false,
    );
    assert!(
        shader.drawn() && !particles.drawing(),
        "built: from the next frame"
    );
    prepare(
        &device,
        &queue,
        &mut particles,
        vec![batch(shader.clone(), 1)],
        false,
    );
    assert!(particles.drawing());
    let pixels = draw(&device, &queue, &particles);
    let middle = at(&pixels, 32, 32);
    assert!(
        middle[0].abs_diff(128) <= 1 && middle[1..] == [0, 0, 0],
        "{middle:?}"
    );
    assert_eq!(at(&pixels, 2, 2), [0, 0, 0, 0]);
}

/// A shader reading a block that is neither the knobs nor one of its type's `uniforms` is not
/// used: nothing draws, rather than failing at the draw, and its type keeps its quads.
#[test]
#[ignore = "requires a Vulkan/Metal/DX12 adapter"]
fn a_shader_reading_a_block_it_is_not_given_is_refused() {
    let _gpu = crate::gfx::test_gpu::lock();
    let (device, queue) = device().expect("a GPU");
    let mut particles = CoreParticles::new(&device, &queue, (FORMAT, false));
    let shader = shader(&[]);
    for _ in 0..2 {
        prepare(
            &device,
            &queue,
            &mut particles,
            vec![batch(shader.clone(), 1)],
            false,
        );
    }
    assert!(shader.refused() && !particles.drawing());
    assert_eq!(at(&draw(&device, &queue, &particles), 32, 32), [0, 0, 0, 0]);
}

/// A pack's GLSL writes vanilla's gamma: on a frame of linear light the type keeps its quads,
/// and once the frame holds gamma again the shader is tried again.
#[test]
#[ignore = "requires a Vulkan/Metal/DX12 adapter"]
fn a_frame_of_linear_light_keeps_the_types_quads() {
    let _gpu = crate::gfx::test_gpu::lock();
    let (device, queue) = device().expect("a GPU");
    let mut particles = CoreParticles::new(&device, &queue, (FORMAT, true));
    let shader = shader(&["Brightness"]);
    prepare(
        &device,
        &queue,
        &mut particles,
        vec![batch(shader.clone(), 1)],
        true,
    );
    assert!(shader.refused() && !particles.drawing());
    // No batch while refused (`Store::shader_batch`), only the frame changing.
    prepare(&device, &queue, &mut particles, Vec::new(), false);
    assert!(!shader.refused() && !shader.drawn(), "to be tried again");
    prepare(
        &device,
        &queue,
        &mut particles,
        vec![batch(shader.clone(), 1)],
        false,
    );
    assert!(shader.drawn());
}

/// Additive copies of one record stack on its place: three at `Brightness` 0.25 add up to
/// three quarters of red, where one is a quarter.
#[test]
#[ignore = "requires a Vulkan/Metal/DX12 adapter"]
fn a_records_copies_draw_on_its_place() {
    let _gpu = crate::gfx::test_gpu::lock();
    let (device, queue) = device().expect("a GPU");
    let mut particles = CoreParticles::new(&device, &queue, (FORMAT, false));
    let program = shader(&["Brightness"]).program.clone();
    let additive = Arc::new(GpuShader::new(
        "test:additive".to_owned(),
        program,
        None,
        RenderType::AdditiveTranslucent,
        vec![(
            "Brightness".to_owned(),
            sunrise_polytone::expr::Program::compile("0.5").unwrap(),
        )],
    ));
    let copies = |n| {
        let mut b = batch(additive.clone(), n);
        b.uniforms = vec![("Brightness".to_owned(), 0.25)];
        b
    };
    for _ in 0..2 {
        prepare(&device, &queue, &mut particles, vec![copies(3)], false);
    }
    let middle = at(&draw(&device, &queue, &particles), 32, 32);
    assert!(
        middle[0].abs_diff(191) <= 1 && middle[1..3] == [0, 0],
        "{middle:?}"
    );
}
