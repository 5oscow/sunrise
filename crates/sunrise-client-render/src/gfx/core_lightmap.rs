//! A resource pack's `core/lightmap` shader (26.3's `Lightmap.render`, `RenderPipelines.LIGHTMAP`):
//! the 16x16 light texture drawn by the pack's GLSL from the frame's `LightmapInfo`, as a pack that
//! darkens caves or tints the night expects (wiki "Lightmaps", 1.21.11 and up). Without one the
//! texture is computed natively ([`native_lightmap`]). `Renderer::update_lightmap` is the one
//! entry point for the frame's light texture, either way.

use crate::core_shader::{CoreProgram, Glue};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// `core/screenquad`: no vertex input, three vertices from `gl_VertexIndex`. GL's framebuffer
/// rows run from the bottom and wgpu's from the top: the flip keeps row `n` at sky level `n`.
pub(crate) const GLUE: Glue = Glue {
    inputs: &[],
    prelude: "",
    setup: "",
    finish: "gl_Position.y = -gl_Position.y;",
    vertex_id: "int(gl_VertexIndex)",
};

/// The block `Lightmap.render` fills, beside the vanilla ones.
pub(crate) const INFO_BLOCK: &str = "LightmapInfo";

/// Each vanilla uniform block's offset in the one buffer (`core_shader::BLOCKS` order), then
/// `LightmapInfo`'s.
const BLOCK_OFFSETS: [u64; 4] = [0, 256, 512, 768];
const BLOCK_SIZES: [u64; 4] = [48, 64, 160, 48];
const INFO_OFFSET: u64 = 1024;
const INFO_SIZE: u64 = 96;

/// `LightmapRenderState` as `Lightmap.render` writes it (std140: six floats, four vec3s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LightmapInfo {
    pub sky_factor: f32,
    pub block_factor: f32,
    pub night_vision: f32,
    pub darkness: f32,
    pub boss_darkening: f32,
    pub brightness: f32,
    pub block_light_tint: [f32; 3],
    pub sky_light_color: [f32; 3],
    pub ambient_color: [f32; 3],
    pub night_vision_color: [f32; 3],
}

impl LightmapInfo {
    /// The block's bytes, as `lightmap.fsh` reads them.
    pub(crate) fn std140(&self) -> [u8; INFO_SIZE as usize] {
        let mut floats = [0f32; 24];
        floats[..6].copy_from_slice(&[
            self.sky_factor,
            self.block_factor,
            self.night_vision,
            self.darkness,
            self.boss_darkening,
            self.brightness,
        ]);
        // vec3s start on 16 bytes: floats 8, 12, 16 and 20.
        for (i, v) in [
            self.block_light_tint,
            self.sky_light_color,
            self.ambient_color,
            self.night_vision_color,
        ]
        .into_iter()
        .enumerate()
        {
            floats[8 + 4 * i..11 + 4 * i].copy_from_slice(&v);
        }
        let mut bytes = [0u8; INFO_SIZE as usize];
        bytes.copy_from_slice(bytemuck::cast_slice(&floats));
        bytes
    }
}

/// The light texture `lightmap.fsh` draws for `info`, computed here: RGBA rows of sky level by
/// block level, what the native path uploads.
pub(crate) fn native_lightmap(info: &LightmapInfo) -> [u8; 16 * 16 * 4] {
    use glam::Vec3;
    let brightness = |level: f32| level / (4.0 - 3.0 * level);
    let mut pixels = [0u8; 16 * 16 * 4];
    for s in 0..16 {
        for b in 0..16 {
            let block_level = b as f32 / 15.0;
            let sky_level = s as f32 / 15.0;
            let block_brightness = brightness(block_level) * info.block_factor;
            let sky_brightness = brightness(sky_level) * info.sky_factor;
            let mut color = Vec3::from(info.ambient_color)
                .max(Vec3::from(info.night_vision_color) * info.night_vision);
            color += Vec3::from(info.sky_light_color) * sky_brightness;
            let parabolic = (2.0 * block_level - 1.0).powi(2);
            let block_color = Vec3::from(info.block_light_tint).lerp(Vec3::ONE, 0.9 * parabolic);
            color += block_color * block_brightness;
            // A boss bar mixes the light toward 70/60/60%.
            color = color.lerp(color * Vec3::new(0.7, 0.6, 0.6), info.boss_darkening);
            color -= Vec3::splat(info.darkness);
            color = color.clamp(Vec3::ZERO, Vec3::ONE);
            let max = color.max_element().max(1e-6);
            let inverted = 1.0 - max;
            let scaled = 1.0 - inverted.powi(4);
            let not_gamma = color * (scaled / max);
            color = color.lerp(not_gamma, info.brightness);
            let i = (s * 16 + b) * 4;
            pixels[i] = (color.x * 255.0) as u8;
            pixels[i + 1] = (color.y * 255.0) as u8;
            pixels[i + 2] = (color.z * 255.0) as u8;
            pixels[i + 3] = 255;
        }
    }
    pixels
}

/// A readback buffer's state.
const IDLE: u8 = 0;
/// The frame's commands copy the texture into it; mapped once they are submitted.
const COPIED: u8 = 1;
const MAPPING: u8 = 2;
const MAPPED: u8 = 3;
/// A readback's row: copies need 256 bytes a row, and the texture has 64.
const ROW: u64 = 256;

/// The pack-drawn texture read back for F3's lightmap chart (`Renderer::lightmap_pixels`), one
/// frame late, through two buffers in turn.
struct Readback {
    buffers: [(wgpu::Buffer, Arc<AtomicU8>); 2],
    /// The buffer this frame copies into, to be mapped once it is submitted.
    copied: Option<usize>,
    /// Each buffer's copy, numbered in frame order, so the newest one wins.
    serials: [u64; 2],
    next: u64,
}

/// The packs' lightmap pipeline, its uniforms and its readback.
pub(super) struct CoreLightmap {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    readback: Readback,
}

impl CoreLightmap {
    /// The pipeline for `program`, drawing into an `Rgba8Unorm` texture; `None` when the pack's
    /// shader does not compile for the backend or reads a block it is not given (the layout
    /// has no binding for it, which the error scope catches).
    pub(super) fn new(device: &wgpu::Device, program: &CoreProgram) -> Option<Self> {
        let info_binding = program
            .other
            .iter()
            .position(|n| n == INFO_BLOCK)
            .map(crate::core_shader::other_binding);
        // A pack's shader is untrusted: the backend failing to compile it is an internal error.
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
        let module = |label, module: &naga::Module| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module.clone())),
            })
        };
        let vertex = module("Resource pack lightmap (vertex)", &program.vertex);
        let fragment = module("Resource pack lightmap (fragment)", &program.fragment);
        let block = |binding: u32, size: u64| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(size),
            },
            count: None,
        };
        let mut entries: Vec<wgpu::BindGroupLayoutEntry> = (0..4)
            .map(|binding| block(binding, BLOCK_SIZES[binding as usize]))
            .collect();
        entries.extend(info_binding.map(|b| block(b, INFO_SIZE)));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Resource pack lightmap"),
            entries: &entries,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Resource pack lightmap"),
            size: INFO_OFFSET + 256,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let range = |binding: u32, offset: u64, size: u64| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &uniform,
                offset,
                size: wgpu::BufferSize::new(size),
            }),
        };
        let mut resources: Vec<wgpu::BindGroupEntry> = (0..4)
            .map(|i| range(i as u32, BLOCK_OFFSETS[i], BLOCK_SIZES[i]))
            .collect();
        resources.extend(info_binding.map(|b| range(b, INFO_OFFSET, INFO_SIZE)));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Resource pack lightmap"),
            layout: &layout,
            entries: &resources,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Resource pack lightmap"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Resource pack lightmap"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &fragment,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let internal = pollster::block_on(internal.pop());
        if let Some(e) = internal.or(pollster::block_on(validation.pop())) {
            log::warn!("the resource pack's lightmap shader is not used: {e}");
            return None;
        }
        let buffer = || {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Resource pack lightmap readback"),
                size: ROW * 16,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            (buffer, Arc::new(AtomicU8::new(IDLE)))
        };
        Some(Self {
            pipeline,
            group,
            uniform,
            readback: Readback {
                buffers: [buffer(), buffer()],
                copied: None,
                serials: [0; 2],
                next: 0,
            },
        })
    }

    /// The frame's blocks in the uniform buffer's layout: `globals` at 0, `info` at its own.
    fn uniforms(globals: [u8; 48], info: &LightmapInfo) -> Vec<u8> {
        let mut data = vec![0u8; (INFO_OFFSET + INFO_SIZE) as usize];
        data[..48].copy_from_slice(&globals);
        data[INFO_OFFSET as usize..].copy_from_slice(&info.std140());
        data
    }

    /// `Lightmap.render`'s pass: the screen quad over the whole of `texture`, then its copy
    /// for the readback when a buffer is free.
    fn draw(&mut self, encoder: &mut wgpu::CommandEncoder, texture: &wgpu::Texture) {
        let view = texture.create_view(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Resource pack lightmap"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.group, &[]);
            pass.draw(0..3, 0..1);
        }
        let readback = &mut self.readback;
        // A copy no submission mapped (a frame submitted another way): its buffer is free.
        if let Some(i) = readback.copied.take() {
            readback.buffers[i].1.store(IDLE, Ordering::Release);
        }
        let Some(i) = (0..2).find(|&i| readback.buffers[i].1.load(Ordering::Acquire) == IDLE)
        else {
            return;
        };
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback.buffers[i].0,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ROW as u32),
                    rows_per_image: Some(16),
                },
            },
            texture.size(),
        );
        readback.buffers[i].1.store(COPIED, Ordering::Release);
        readback.copied = Some(i);
        readback.next += 1;
        readback.serials[i] = readback.next;
    }

    /// The readback to start once the frame is submitted: mapping the buffer it copied into.
    pub(super) fn submitted(&mut self) -> Option<super::submit::AfterSubmit> {
        let i = self.readback.copied.take()?;
        let (buffer, state) = self.readback.buffers[i].clone();
        state.store(MAPPING, Ordering::Release);
        Some(Box::new(move || {
            let mapped = state.clone();
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    mapped.store(
                        if result.is_ok() { MAPPED } else { IDLE },
                        Ordering::Release,
                    );
                });
        }))
    }

    /// The newest texture read back, into `pixels`; whether one was.
    fn collect(&self, pixels: &mut [u8; 16 * 16 * 4]) -> bool {
        let readback = &self.readback;
        let mut any = false;
        let mut order = [0, 1];
        order.sort_by_key(|&i| readback.serials[i]);
        for (buffer, state) in order.map(|i| &readback.buffers[i]) {
            if state.load(Ordering::Acquire) != MAPPED {
                continue;
            }
            if let Ok(range) = buffer.slice(..).get_mapped_range() {
                for row in 0..16 {
                    let at = row * ROW as usize;
                    pixels[row * 64..][..64].copy_from_slice(&range[at..at + 64]);
                }
                any = true;
            }
            buffer.unmap();
            state.store(IDLE, Ordering::Release);
        }
        any
    }
}

/// The renderer's light texture: the pack's shader draws it, or it is computed here.
impl super::Renderer {
    /// The resource packs' `core/lightmap` (`HudAtlas::lightmap_shader`), or the native one.
    pub fn set_lightmap_shader(&mut self, program: Option<&CoreProgram>) {
        self.core_lightmap = program.and_then(|p| CoreLightmap::new(&self.device, p));
        self.lightmap_uploaded = false;
    }

    /// The frame's light texture (`LightmapRenderStateExtractor.extract`, then
    /// `Lightmap.render`): a pack's `core/lightmap` draws it from the frame's `LightmapInfo`,
    /// or its pixels are computed and uploaded when they change. Fullbright keeps it white.
    pub(super) fn update_lightmap(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &super::Frame,
    ) {
        let sky = &frame.sky;
        // What lightmap.fsh reads (NightVisionColor's default is #999999).
        let info = LightmapInfo {
            sky_factor: sky.sky_light_factor,
            block_factor: 1.4 + frame.block_flicker,
            night_vision: frame.night_vision,
            darkness: frame.darkness.1,
            boss_darkening: frame.boss_darken,
            brightness: (self.gamma - frame.darkness.0).max(0.0),
            block_light_tint: sky.block_light_tint,
            sky_light_color: sky.sky_light_color,
            ambient_color: sky.ambient_light_color,
            night_vision_color: [0.6; 3],
        };
        let globals = self.pack_globals(frame);
        if !self.fullbright
            && let Some(core) = self.core_lightmap.as_mut()
        {
            let data = CoreLightmap::uniforms(globals, &info);
            self.uploads.buffer(encoder, &core.uniform, 0, &data);
            core.collect(&mut self.lightmap_pixels);
            core.draw(encoder, &self.lightmap);
            // The native pixels go up again when the pack's shader stops drawing.
            self.lightmap_uploaded = false;
            return;
        }
        let mut pixels = native_lightmap(&info);
        if self.fullbright {
            pixels.fill(255);
        }
        // The lightmap changes with the time of day, flicker and effects,
        // not every frame: upload only what differs from the texture.
        if pixels == self.lightmap_pixels && self.lightmap_uploaded {
            return;
        }
        self.uploads.texture(
            encoder,
            wgpu::TexelCopyTextureInfo {
                texture: &self.lightmap,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            16,
            16,
            &pixels,
        );
        self.lightmap_pixels = pixels;
        self.lightmap_uploaded = true;
    }
}

#[cfg(test)]
#[path = "core_lightmap_tests.rs"]
mod tests;
