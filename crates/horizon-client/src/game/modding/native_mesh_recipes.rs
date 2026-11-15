//! State mesh recipes a client mod gives (`native_section_mesh_recipes`, the section meshing
//! design): asked on the world thread once per resource load and on
//! `refresh_render_visibility`, checked whole per mod, resolved to atlas layers and dense state
//! slots, and frozen into the table every meshing job reads. No mod code runs on a worker for it.

use super::{BlockStateId, Game};
use horizon_client::fast_hash::FxHashMap;
use horizon_client::mesher::{MeshRecipes, MeshRecipesBuilder, ResolvedQuad};
use mod_api::{Answered, MeshRecipeRefusal, MeshRecipeTarget, StateMeshRecipe, MAX_RECIPE_QUADS};
use std::sync::Arc;

impl Game<'_> {
    /// Asks the mods again when the resources or the visibility refresh made the table stale.
    /// Nothing is asked, built or remeshed while no mod listens and no table is in use.
    pub(in crate::game) fn prepare_mesh_recipes(&mut self) {
        if !self.streaming.section_visibility.recipes_need_asking() {
            return;
        }
        let table = if mod_api::__private::listened(mod_api::HookId::SectionMeshRecipes) {
            self.resolve_mesh_recipes().map(Arc::new)
        } else {
            None
        };
        self.streaming.set_mesh_recipes(&mut self.world, table);
    }

    /// Every listening mod's recipes, each mod's taken whole or refused whole with one log line.
    fn resolve_mesh_recipes(&mut self) -> Option<MeshRecipes> {
        let mut answers: Vec<Answered<StateMeshRecipe>> = Vec::new();
        let asked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.native_section_mesh_recipes(mod_api::__private::SectionMeshRecipesCall {
                recipes: &mut answers,
            })
        }));
        if asked.is_err() {
            log::warn!(
                "Horizon: section mesh recipes: {}; blocks draw their own models",
                MeshRecipeRefusal::Failed
            );
            return None;
        }
        let context = self.worker_context();
        let registry = context.assets.registry;
        let sprites = &context.render.sprite_layers;
        let mut by_block: Option<FxHashMap<&str, Vec<u32>>> = None;
        let mut builder = MeshRecipesBuilder::new(registry.len());
        let mut replaced: FxHashMap<u32, &'static str> = FxHashMap::default();
        let mut start = 0;
        while start < answers.len() {
            let mod_id = answers[start].mod_id;
            let end = answers[start..]
                .iter()
                .position(|a| a.mod_id != mod_id)
                .map_or(answers.len(), |n| start + n);
            let staged = (|| {
                let mut staged = Vec::new();
                let mut counts: FxHashMap<u32, usize> = FxHashMap::default();
                for Answered { answer: recipe, .. } in &answers[start..end] {
                    recipe.check()?;
                    let mut quads = Vec::with_capacity(recipe.quads.len());
                    for quad in &recipe.quads {
                        let Some(&layer) = sprites.get(&quad.sprite) else {
                            return Err(MeshRecipeRefusal::UnknownSprite {
                                sprite: quad.sprite.clone(),
                            });
                        };
                        quads.push(ResolvedQuad::new(
                            quad.positions,
                            quad.uvs,
                            quad.color,
                            layer as u16,
                            quad.cull,
                            quad.translucent,
                        ));
                    }
                    let states: Vec<u32> = match &recipe.target {
                        MeshRecipeTarget::State(id) if (id.0 as usize) < registry.len() => {
                            vec![id.0]
                        }
                        MeshRecipeTarget::Block(name) => {
                            let index = by_block.get_or_insert_with(|| {
                                let mut map: FxHashMap<&str, Vec<u32>> = FxHashMap::default();
                                for id in 0..registry.len() as u32 {
                                    if let Some(state) =
                                        registry.state(BlockStateId(id))
                                    {
                                        map.entry(state.name.as_str()).or_default().push(id);
                                    }
                                }
                                map
                            });
                            match index.get(name.as_str()) {
                                Some(states) => states.clone(),
                                None => return Err(MeshRecipeRefusal::UnknownTarget),
                            }
                        }
                        _ => return Err(MeshRecipeRefusal::UnknownTarget),
                    };
                    for &state in &states {
                        if recipe.replace
                            && let Some(&by) = replaced.get(&state)
                            && by != mod_id
                        {
                            return Err(MeshRecipeRefusal::Replaced { by: by.to_owned() });
                        }
                        let count = counts.entry(state).or_insert_with(|| {
                            builder.get(state).map_or(0, |r| r.quads())
                        });
                        *count += quads.len();
                        if *count > MAX_RECIPE_QUADS {
                            return Err(MeshRecipeRefusal::TooManyQuads {
                                count: u32::try_from(*count).unwrap_or(u32::MAX),
                            });
                        }
                    }
                    staged.push((states, recipe.replace, recipe.occludes, quads));
                }
                Ok(staged)
            })();
            match staged {
                Ok(staged) => {
                    for (states, replace, occludes, quads) in staged {
                        for state in states {
                            if replace {
                                replaced.insert(state, mod_id);
                            }
                            builder.add(state, replace, occludes, &quads);
                        }
                    }
                }
                Err(refusal) => log::warn!(
                    "Horizon: mod {mod_id}'s section mesh recipes refused whole: {refusal}"
                ),
            }
            start = end;
        }
        builder.finish()
    }
}
