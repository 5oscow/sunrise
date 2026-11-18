//! A GPU particle type's own core shader (wiki "GPU Particles", the 1.21.11 format): the
//! `shader` and `uniforms` it names, and the `polytone:gpu_particle.glsl` block of its knobs the
//! shader imports, as Sunrise lays it out.
use crate::expr::Program;
use crate::particles::{CustomParticle, Gpu, RenderType, program};
use serde_json::Value as Json;

/// A GPU particle's own core shader (`shader`, `ns:name`, `minecraft` when it names no
/// namespace) and its `uniforms`: each a block of its own holding one float, set every frame
/// from its expression.
#[derive(Debug)]
pub struct ParticleShader {
    pub namespace: String,
    pub name: String,
    pub uniforms: Vec<(String, Program)>,
    /// The uniforms whose expression doesn't compile, and why.
    pub refused: Vec<(String, String)>,
}

impl ParticleShader {
    /// The shader's id as the pack names it (`ns:name`).
    pub fn id(&self) -> String {
        format!("{}:{}", self.namespace, self.name)
    }

    /// The pack file of one of its stages (`vsh` or `fsh`).
    pub fn stage_path(&self, extension: &str) -> String {
        format!(
            "assets/{}/shaders/core/{}.{extension}",
            self.namespace, self.name
        )
    }
}

impl CustomParticle {
    /// A GPU particle's own `shader`; `None` for any other particle, or one without it.
    pub fn gpu_shader(&self) -> Option<ParticleShader> {
        self.gpu.as_ref()?;
        let shader = self.render.get("shader")?.as_str()?;
        let (namespace, name) = shader.split_once(':').unwrap_or(("minecraft", shader));
        let mut uniforms = Vec::new();
        let mut refused = Vec::new();
        for (uniform, expression) in self
            .render
            .get("uniforms")
            .and_then(Json::as_object)
            .into_iter()
            .flatten()
        {
            let compiled = match expression {
                Json::String(_) | Json::Number(_) => program(expression),
                _ => Err(format!("{expression} is not an expression")),
            };
            match compiled {
                Ok(compiled) => uniforms.push((uniform.clone(), compiled)),
                Err(e) => refused.push((uniform.clone(), e)),
            }
        }
        Some(ParticleShader {
            namespace: namespace.to_owned(),
            name: name.to_owned(),
            uniforms,
            refused,
        })
    }
}

impl Gpu {
    /// The pack file of `texture` (`ns:textures/particle/rain.png`, `minecraft` when it names
    /// no namespace).
    pub fn texture_path(&self) -> String {
        let (namespace, path) = self
            .texture
            .split_once(':')
            .unwrap_or(("minecraft", &self.texture));
        format!("assets/{namespace}/{path}")
    }
}

/// The include a GPU particle's shader imports (`#moj_import <polytone:gpu_particle.glsl>`):
/// the type's knobs as one std140 block, laid out as [`gpu_block`] writes it.
pub const GPU_PARTICLE_GLSL: &str = "layout(std140) uniform PolytoneGpuParticle {\n\
    vec3 Origin;\n    float Time;\n\
    float Gravity;\n    float Drag;\n    float Sway;\n    float Spin;\n\
    float SizeEnd;\n    float Aspect;\n    float AlphaCutoff;\n    int Frames;\n\
    vec2 Fade;\n    int Billboard;\n    int RandomSprite;\n\
    int UseColorEnd;\n    int KillBelowHeightmap;\n    float CameraY;\n    float HzPad;\n\
    vec4 ColorEnd;\n\
    vec4 HeightmapInfo;\n\
};\n";

/// The include's id as a core shader names it.
pub const GPU_PARTICLE_INCLUDE: &str = "polytone:shaders/include/gpu_particle.glsl";

/// The block's name in [`GPU_PARTICLE_GLSL`], and its size.
pub const GPU_BLOCK: &str = "PolytoneGpuParticle";
pub const GPU_BLOCK_SIZE: usize = 112;

/// The heightmap a GPU particle's shader samples (`Sampler1`): `size` × `size` columns from
/// the world column `origin` (x, z), heights above `min_y`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeightmapArea {
    pub origin: [i32; 2],
    pub size: u32,
    pub min_y: i32,
}

/// The `polytone:gpu_particle.glsl` block for `gpu`: `origin` relative to the camera at
/// `camera`, `time` on the records' clock, `billboard` (0 facing the camera, 1 around Y, 2 flat,
/// 3 along its velocity), the camera's Y and the heightmap's place relative to the camera.
#[allow(clippy::too_many_arguments)]
pub fn gpu_block(
    gpu: &Gpu,
    render_type: RenderType,
    billboard: i32,
    random_sprite: bool,
    origin: [f32; 3],
    time: f32,
    camera: [f64; 3],
    heightmap: Option<HeightmapArea>,
) -> [u8; GPU_BLOCK_SIZE] {
    let mut floats = [0f32; GPU_BLOCK_SIZE / 4];
    let int = |v: i32| f32::from_bits(v as u32);
    floats[..3].copy_from_slice(&origin);
    floats[3] = time;
    floats[4] = gpu.gravity as f32;
    // Drag is `-ln(friction)`: none for a friction of 1 (or out of range).
    floats[5] = if gpu.friction > 0.0 && gpu.friction < 1.0 {
        -(gpu.friction.ln()) as f32
    } else {
        0.0
    };
    floats[6] = gpu.sway as f32;
    floats[7] = gpu.spin as f32;
    floats[8] = gpu.size_end.map_or(-1.0, |s| s as f32);
    floats[9] = gpu.aspect as f32;
    // Wiki "GPU Particles": the two translucent types blend, anything else is alpha cutout.
    floats[10] = match render_type {
        RenderType::Translucent | RenderType::AdditiveTranslucent => 0.0,
        _ => 0.1,
    };
    floats[11] = int(gpu.frames.max(1) as i32);
    floats[12] = gpu.fade[0] as f32;
    floats[13] = gpu.fade[1] as f32;
    floats[14] = int(billboard);
    floats[15] = int(i32::from(random_sprite));
    floats[16] = int(i32::from(gpu.color_end.is_some()));
    floats[17] = int(i32::from(gpu.kill_below_heightmap));
    floats[18] = camera[1] as f32;
    if let Some(end) = gpu.color_end {
        let channel = |shift: u32| ((end >> shift) & 0xff) as f32 / 255.0;
        floats[20..24].copy_from_slice(&[channel(16), channel(8), channel(0), channel(24)]);
    }
    if let Some(h) = heightmap {
        floats[24] = (f64::from(h.origin[0]) - camera[0]) as f32;
        floats[25] = (f64::from(h.origin[1]) - camera[2]) as f32;
        floats[26] = h.size as f32;
        floats[27] = h.min_y as f32;
    }
    let mut bytes = [0u8; GPU_BLOCK_SIZE];
    for (out, float) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(floats) {
        *out = float.to_le_bytes();
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn gpu(gravity: f64, friction: f64) -> Gpu {
        Gpu {
            texture: String::new(),
            frames: 1,
            gravity,
            friction,
            size_end: None,
            color_end: None,
            fade: [0.0; 2],
            sway: 0.0,
            spin: 0.0,
            aspect: 1.0,
            area: None,
            kill_below_heightmap: false,
        }
    }

    /// The block as floats, as the shader reads it.
    fn floats(block: &[u8; GPU_BLOCK_SIZE]) -> Vec<f32> {
        block
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect()
    }

    /// Wiki "GPU Particles": `shader` names a core shader (`minecraft` without a namespace) and
    /// each of `uniforms` is an expression; one that doesn't compile is refused by name.
    #[test]
    fn a_gpu_particle_names_its_own_shader_and_uniforms() {
        let parse = |extra: Json| {
            let mut json = json!({"type": "gpu", "texture": "a:textures/particle/b.png"});
            json.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            CustomParticle::parse(&json).unwrap()
        };
        let shader = parse(json!({
            "shader": "my_pack:rain",
            "uniforms": {"Brightness": "0.5", "Wind": 2, "Bad": "1 +", "Flag": true}
        }))
        .gpu_shader()
        .unwrap();
        assert_eq!(shader.id(), "my_pack:rain");
        assert_eq!(
            shader.stage_path("fsh"),
            "assets/my_pack/shaders/core/rain.fsh"
        );
        let mut names: Vec<&str> = shader.uniforms.iter().map(|(n, _)| n.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["Brightness", "Wind"]);
        let mut refused: Vec<&str> = shader.refused.iter().map(|(n, _)| n.as_str()).collect();
        refused.sort_unstable();
        assert_eq!(refused, ["Bad", "Flag"]);
        let plain = parse(json!({"shader": "rain"})).gpu_shader().unwrap();
        assert_eq!(plain.id(), "minecraft:rain");
        assert!(parse(json!({})).gpu_shader().is_none());
        // A custom (CPU) particle's `shader` is not a GPU particle's.
        let custom = CustomParticle::parse(&json!({"shader": "my_pack:rain"})).unwrap();
        assert!(custom.gpu_shader().is_none());
        let texture = parse(json!({})).gpu.unwrap().texture_path();
        assert_eq!(texture, "assets/a/textures/particle/b.png");
    }

    /// The `polytone:gpu_particle.glsl` block as its std140 layout reads it, each knob at its
    /// offset: drag `-ln(friction)`, `SizeEnd`, the ints as their bits, `ColorEnd` from ARGB, and
    /// `HeightmapInfo` the heightmap's corner relative to the camera, its size and bottom.
    #[test]
    fn the_shader_block_lays_out_the_types_knobs() {
        let knobs = Gpu {
            frames: 4,
            size_end: Some(0.2),
            color_end: Some(0x80FF_0000),
            fade: [0.25, 0.5],
            sway: 0.25,
            spin: 0.125,
            aspect: 2.0,
            kill_below_heightmap: true,
            ..gpu(0.04, 0.25)
        };
        let heightmap = HeightmapArea {
            origin: [-54, -68],
            size: 128,
            min_y: -64,
        };
        let block = gpu_block(
            &knobs,
            RenderType::Opaque,
            3,
            true,
            [-0.5, -0.25, -0.75],
            12.5,
            [10.5, 70.25, -3.75],
            Some(heightmap),
        );
        let f = floats(&block);
        let int = |i: usize| f[i].to_bits() as i32;
        assert_eq!(f[..5], [-0.5, -0.25, -0.75, 12.5, 0.04]);
        assert_eq!(f[5], 1.386_294_4);
        assert_eq!(f[6..11], [0.25, 0.125, 0.2, 2.0, 0.1]);
        assert_eq!(int(11), 4);
        assert_eq!(f[12..14], [0.25, 0.5]);
        assert_eq!([int(14), int(15), int(16), int(17)], [3, 1, 1, 1]);
        assert_eq!(f[18], 70.25);
        assert_eq!(f[20..24], [1.0, 0.0, 0.0, 128.0 / 255.0]);
        assert_eq!(f[24..28], [-64.5, -64.25, 128.0, -64.0]);
        // No end colour, size or heightmap: their defaults.
        let plain = gpu_block(
            &gpu(0.0, 1.0),
            RenderType::Opaque,
            0,
            false,
            [0.0; 3],
            0.0,
            [0.0; 3],
            None,
        );
        let f = floats(&plain);
        assert_eq!([f[5], f[8], f[16].to_bits() as f32], [0.0, -1.0, 0.0]);
        assert_eq!(f[20..28], [0.0; 8]);
    }

    /// Wiki "GPU Particles": only the non-translucent types are alpha cutout. A translucent
    /// type's faint records (a storm's rain at alpha 0.05) must not fall under the cutoff a
    /// custom shader discards below.
    #[test]
    fn only_cutout_types_have_an_alpha_cutoff() {
        let cutoff = |render_type| {
            let block = gpu_block(
                &gpu(0.0, 1.0),
                render_type,
                0,
                false,
                [0.0; 3],
                0.0,
                [0.0; 3],
                None,
            );
            floats(&block)[10]
        };
        assert_eq!(cutoff(RenderType::Translucent), 0.0);
        assert_eq!(cutoff(RenderType::AdditiveTranslucent), 0.0);
        assert_eq!(cutoff(RenderType::Opaque), 0.1);
    }
}
