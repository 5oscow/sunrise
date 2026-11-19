//! Resource packs' Polytone post chains (#11486, wiki "Shaders"): vanilla `post_effect` chains a
//! pack runs over the world while their `activation_condition` holds, with uniform blocks from
//! expressions (`expression_uniforms`), the `PolyGlobals` block, and targets of their own
//! (`post_targets`) kept between frames. `post_chain` runs them; this reads what they add.
use crate::environment::Frame;
use crate::post_chain::{Extras, InternalTarget};
use serde_json::Value as Json;
use sunrise_polytone::expr::Program;

/// A `polytone/post_chains/<path>.json`.
struct Chain {
    /// The `post_effect` chain it runs.
    effect: String,
    activation: Option<Program>,
    /// Each expression uniform: its block's name and expression.
    uniforms: Vec<(String, Program)>,
}

/// The packs' post chains and targets.
pub struct PolytonePost {
    chains: Vec<Chain>,
    targets: std::collections::BTreeMap<String, InternalTarget>,
    configs: sunrise_polytone::pack::Configs,
}

/// What `PolyGlobals` holds (wiki "Extra Shader Uniforms").
#[derive(Clone, Copy, Debug, Default)]
pub struct PolyGlobals {
    /// The world's projection (reverse-Z, as 26.3's and Sunrise's are), and its model-view (the
    /// camera's turn).
    pub projection: glam::Mat4,
    pub model_view: glam::Mat4,
    /// The sun's angle in radians, 0 at the horizon.
    pub sun_angle: f32,
    /// The time of day in ticks (0 to 24000), and the game ticks this frame advanced.
    pub day_time: f32,
    pub delta_time: f32,
    /// The player's feet: `PolyPlayerBlockPos`, and `PolyPlayerOffset` as vanilla's
    /// `CameraOffset` is (the block position less the position, so `polyPlayerPos()` is the
    /// block position less the offset).
    pub player: [f64; 3],
}

impl PolyGlobals {
    /// The std140 block: `mat4, mat4, float, float, float, ivec3, vec3`.
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(176);
        for m in [self.projection, self.model_view] {
            out.extend(m.to_cols_array().iter().flat_map(|v| v.to_le_bytes()));
        }
        for v in [self.sun_angle, self.day_time, self.delta_time, 0.0] {
            out.extend(v.to_le_bytes());
        }
        let block = self.player.map(f64::floor);
        for v in block {
            out.extend((v as i32).to_le_bytes());
        }
        out.extend(0i32.to_le_bytes());
        for i in 0..3 {
            out.extend(((block[i] - self.player[i]) as f32).to_le_bytes());
        }
        out.extend(0f32.to_le_bytes());
        out
    }
}

/// Every `.json` under `assets/<namespace>/polytone/<kind>`, by its implicit id.
fn files(
    root: &sunrise_assets::resource_source::ResourceSource,
    kind: &str,
) -> Vec<(String, Json)> {
    let mut out = Vec::new();
    for relative in root.list("assets/") {
        let Some(id) = sunrise_polytone::implicit_id(&relative, kind) else {
            continue;
        };
        match root
            .read_to_string(&relative)
            .ok()
            .and_then(|s| serde_json::from_str::<Json>(&s).ok())
        {
            Some(json) => out.push((id, json)),
            None => log::warn!("{relative}: invalid post configuration"),
        }
    }
    out
}

fn compile(where_: &str, src: &Json) -> Option<Program> {
    let text = match src {
        Json::String(s) => s.clone(),
        Json::Number(n) => n.to_string(),
        Json::Bool(b) => b.to_string(),
        _ => return None,
    };
    match Program::compile(&text) {
        Ok(p) => Some(p),
        Err(e) => {
            log::warn!("{where_}: {e}");
            None
        }
    }
}

impl PolytonePost {
    /// The post chains and targets of the packs merged at `root`; `None` without chains.
    pub fn read(
        root: &sunrise_assets::resource_source::ResourceSource,
        configs: sunrise_polytone::pack::Configs,
    ) -> Option<Self> {
        let chains: Vec<Chain> = files(root, "post_chains")
            .into_iter()
            .filter_map(|(id, json)| {
                let effect = json["post_chain"].as_str()?;
                let effect = if effect.contains(':') {
                    effect.to_owned()
                } else {
                    format!("minecraft:{effect}")
                };
                let activation = json
                    .get("activation_condition")
                    .and_then(|c| compile(&id, c));
                let uniforms = json["expression_uniforms"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .filter_map(|(name, src)| Some((name.clone(), compile(&id, src)?)))
                    .collect();
                Some(Chain {
                    effect,
                    activation,
                    uniforms,
                })
            })
            .collect();
        if chains.is_empty() {
            return None;
        }
        let targets = files(root, "post_targets")
            .into_iter()
            .map(|(id, json)| {
                let size = |key: &str| json[key].as_u64().filter(|n| *n > 0).map(|n| n as u32);
                let target = InternalTarget {
                    width: size("width"),
                    height: size("height"),
                    persistent: true,
                    clear: [0.0; 4],
                };
                (id, target)
            })
            .collect();
        Some(Self {
            chains,
            targets,
            configs,
        })
    }

    /// The chains to run this frame (those whose condition holds), and what they add.
    pub fn frame(&self, frame: &Frame, globals: &PolyGlobals) -> (Vec<String>, Extras) {
        let mut effects = Vec::new();
        let mut extras = Extras {
            targets: self.targets.clone(),
            blocks: Default::default(),
        };
        extras
            .blocks
            .insert("PolyGlobals".to_owned(), globals.bytes());
        for chain in &self.chains {
            let on = chain.activation.as_ref().is_none_or(|c| {
                crate::environment::eval_frame(c, frame, &self.configs).is_some_and(|v| v.truthy())
            });
            if !on {
                continue;
            }
            for (name, program) in &chain.uniforms {
                let value = crate::environment::eval_frame(program, frame, &self.configs)
                    .map_or(0.0, |v| v.num()) as f32;
                let mut block = value.to_le_bytes().to_vec();
                block.resize(16, 0);
                extras.blocks.insert(name.clone(), block);
            }
            effects.push(chain.effect.clone());
        }
        (effects, extras)
    }
}

/// The packs' `polytone/shader_modifiers` (`sunrise_polytone::shader_modifiers`): each shader's
/// expression uniforms, evaluated with the frame's expression context.
pub struct ShaderModifiers {
    /// Each shader's id (`minecraft:core/lightmap`) and its uniforms.
    shaders: Vec<(String, sunrise_polytone::shader_modifiers::ShaderModifier)>,
    configs: sunrise_polytone::pack::Configs,
}

/// The largest `shader_modifiers` file read: far more than [`MAX_UNIFORMS`] expressions need.
///
/// [`MAX_UNIFORMS`]: sunrise_polytone::shader_modifiers::MAX_UNIFORMS
pub const MAX_SHADER_MODIFIERS_FILE: u64 = 64 * 1024;

impl ShaderModifiers {
    /// The packs' files at `root`; `None` without any. A file over
    /// [`MAX_SHADER_MODIFIERS_FILE`], not JSON, or refused by its contents is left out, named
    /// in the log.
    pub fn read(
        root: &sunrise_assets::resource_source::ResourceSource,
        configs: sunrise_polytone::pack::Configs,
    ) -> Option<Self> {
        let shaders = Self::files(root, &|path, e| log::warn!("{path}: not used: {e}"));
        (!shaders.is_empty()).then_some(Self { shaders, configs })
    }

    /// The uniforms' names a pipeline drawing with `shaders` is given by the packs at `root`,
    /// read as [`Self::read`] reads them, for a loader that only needs the names (the game's
    /// read names the files it leaves out).
    pub fn names_at(
        root: &sunrise_assets::resource_source::ResourceSource,
        shaders: &[&str],
    ) -> Vec<String> {
        Self {
            shaders: Self::files(root, &|_, _| {}),
            configs: Default::default(),
        }
        .names(shaders)
    }

    /// Each file's shader and uniforms; `refused` hears of each one left out.
    fn files(
        root: &sunrise_assets::resource_source::ResourceSource,
        refused: &dyn Fn(&str, String),
    ) -> Vec<(String, sunrise_polytone::shader_modifiers::ShaderModifier)> {
        use sunrise_polytone::shader_modifiers::ShaderModifier;
        let mut shaders = Vec::new();
        for namespace in root.namespaces("assets") {
            for path in root.list(&format!("assets/{namespace}/polytone/shader_modifiers/")) {
                let Some(id) = sunrise_polytone::implicit_id(&path, "shader_modifiers") else {
                    continue;
                };
                let read = root
                    .read_limited(&path, MAX_SHADER_MODIFIERS_FILE)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| {
                        serde_json::from_slice::<Json>(&bytes).map_err(|e| e.to_string())
                    })
                    .and_then(|json| ShaderModifier::parse(&json).map_err(|e| e.to_string()));
                match read {
                    Ok(modifier) => shaders.push((id, modifier)),
                    Err(e) => refused(&path, e),
                }
            }
        }
        shaders
    }

    /// The uniforms a pipeline drawing with `shaders` (its stages' ids) is given.
    fn uniforms<'a>(
        &'a self,
        shaders: &'a [&str],
    ) -> impl Iterator<Item = &'a (String, Program)> + 'a {
        self.shaders
            .iter()
            .filter(|(id, _)| shaders.contains(&id.as_str()))
            .flat_map(|(_, modifier)| &modifier.uniforms)
    }

    /// Their names.
    pub fn names(&self, shaders: &[&str]) -> Vec<String> {
        self.uniforms(shaders)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// This frame's value of each of `wanted` (the uniforms a drawn shader reads), by name: its
    /// expression's, or 0 when it fails or none of `shaders`' files gives it. Only `wanted` is
    /// evaluated.
    pub fn evaluate(&self, shaders: &[&str], frame: &Frame, wanted: &mut [(String, f32)]) {
        for (name, value) in wanted {
            *value = self
                .uniforms(shaders)
                .find(|(n, _)| n == name)
                .and_then(|(_, program)| {
                    crate::environment::eval_frame(program, frame, &self.configs)
                })
                .map_or(0.0, |v| v.num() as f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Packs in memory: `files` (path, text).
    fn pack(files: &[(&str, String)]) -> sunrise_assets::resource_source::ResourceSource {
        use sunrise_assets::resource_source::{Resource, ResourceSource};
        ResourceSource::indexed(
            files
                .iter()
                .map(|(p, t)| ((*p).to_owned(), Resource::bytes(t.clone().into_bytes())))
                .collect(),
        )
    }

    const LIGHTMAP: &str = "assets/minecraft/polytone/shader_modifiers/core/lightmap.json";
    const SCREENQUAD: &str = "assets/minecraft/polytone/shader_modifiers/core/screenquad.json";

    /// A shader's files give its pipeline their uniforms, by its stages' ids; each frame
    /// evaluates those a drawn shader reads, 0 for one no file gives.
    #[test]
    fn shader_modifiers_give_a_pipelines_shaders_their_uniforms() {
        let modifiers = ShaderModifiers::read(
            &pack(&[
                (
                    LIGHTMAP,
                    r#"{"InNether": "g.dimensionType == 'minecraft:the_nether' ? 1 : 0"}"#.into(),
                ),
                (SCREENQUAD, r#"{"Wet": "g.rain()"}"#.into()),
                (
                    "assets/minecraft/polytone/shader_modifiers/core/sky.json",
                    r#"{"Sky": "1"}"#.into(),
                ),
            ]),
            Default::default(),
        )
        .expect("the packs' modifiers");
        let lightmap = crate::gfx::LIGHTMAP_SHADERS;
        assert_eq!(modifiers.names(&lightmap), ["InNether", "Wet"]);
        assert_eq!(modifiers.names(&["minecraft:core/sky"]), ["Sky"]);
        let mut wanted = vec![
            ("InNether".to_owned(), 7.0),
            ("Wet".to_owned(), 7.0),
            ("Sky".to_owned(), 7.0),
        ];
        let frame = Frame {
            dimension: "minecraft:the_nether".into(),
            rain: 1.0,
            ..Frame::default()
        };
        modifiers.evaluate(&lightmap, &frame, &mut wanted);
        // `g.rain` is the mean of rain (1) and thunder (0); `Sky` is another shader's.
        assert_eq!(
            wanted.iter().map(|(_, v)| *v).collect::<Vec<_>>(),
            [1.0, 0.5, 0.0]
        );
        let overworld = Frame {
            dimension: "minecraft:overworld".into(),
            ..Frame::default()
        };
        modifiers.evaluate(&lightmap, &overworld, &mut wanted[..1]);
        assert_eq!(wanted[0].1, 0.0);
        assert!(ShaderModifiers::read(&pack(&[]), Default::default()).is_none());
    }

    /// A file over the read budget, not JSON, or refused by its contents is left out: its
    /// uniforms are no shader's, so a shader reading one is refused when it loads.
    #[test]
    fn a_bad_shader_modifiers_file_is_left_out() {
        let budget = MAX_SHADER_MODIFIERS_FILE as usize;
        let padded = |len: usize| {
            let body = r#"{"Tint": "1"}"#;
            format!("{body}{}", " ".repeat(len - body.len()))
        };
        let read = |text: String| {
            ShaderModifiers::read(&pack(&[(LIGHTMAP, text)]), Default::default())
                .map(|m| m.names(&crate::gfx::LIGHTMAP_SHADERS))
        };
        assert_eq!(read(padded(budget)), Some(vec!["Tint".to_owned()]));
        assert_eq!(read(padded(budget + 1)), None);
        assert_eq!(read("{\"Tint\": ".into()), None);
        assert_eq!(read(r#"{"gl_Tint": "1"}"#.into()), None);
    }

    #[test]
    fn poly_globals_follow_std140() {
        let g = PolyGlobals {
            projection: glam::Mat4::from_diagonal(glam::Vec4::splat(2.0)),
            sun_angle: 1.5,
            day_time: 6000.0,
            delta_time: 0.5,
            player: [10.25, -3.5, 7.0],
            ..Default::default()
        };
        let b = g.bytes();
        let f = |at: usize| f32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        let i = |at: usize| i32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        assert_eq!(b.len(), 176);
        assert_eq!(f(0), 2.0);
        assert_eq!((f(128), f(132), f(136)), (1.5, 6000.0, 0.5));
        // ivec3 at 144, vec3 at 160 (std140 aligns both to 16). The offset is the block position
        // less the position, as vanilla's CameraOffset: the block position less it is the position.
        assert_eq!((i(144), i(148), i(152)), (10, -4, 7));
        assert_eq!((f(160), f(164), f(168)), (-0.25, -0.5, 0.0));
    }

    #[test]
    fn chains_run_while_their_condition_holds_with_their_uniforms() {
        let root =
            std::env::temp_dir().join(format!("sunrise-polytone-post-{}", std::process::id()));
        let write = |path: &str, text: &str| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "assets/p/polytone/post_chains/glare.json",
            r#"{"activation_condition": "g.rain() < 0.5", "post_chain": "p:glare",
                "expression_uniforms": {"Weather": "1 - g.rain()"}}"#,
        );
        write(
            "assets/p/polytone/post_targets/history.json",
            r#"{"width": 1, "height": 1}"#,
        );
        let post = PolytonePost::read(
            &sunrise_assets::resource_source::ResourceSource::directory(root.clone()),
            Default::default(),
        )
        .unwrap();
        let mut frame = Frame {
            rain: 0.25,
            ..Frame::default()
        };
        let (effects, extras) = post.frame(&frame, &PolyGlobals::default());
        assert_eq!(effects, ["p:glare"]);
        // `g.rain` is the mean of rain (0.25) and thunder (0): 0.125.
        assert_eq!(
            f32::from_le_bytes(extras.blocks["Weather"][..4].try_into().unwrap()),
            0.875
        );
        assert!(extras.targets["p:history"].persistent);
        frame.rain = 1.0;
        assert!(post.frame(&frame, &PolyGlobals::default()).0.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
