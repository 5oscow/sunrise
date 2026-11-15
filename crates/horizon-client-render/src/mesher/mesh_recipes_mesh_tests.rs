//! The section meshing design's recipe cases that need the real mesher and prepared resources
//! (`HORIZON_TEST_RESOURCES`), like the other mesher geometry tests.
use super::super::{BiomeZoom, LeafCullingMode, MeshScratch, MeshTables, Mesher};
use super::*;
use crate::{
    assets::{Assets, RenderAssets},
    biome::Biomes,
    blocks::BlockFacts,
    chunk::{Chunk, MIN_Y},
    models,
};

struct Fixture {
    assets: Assets,
    biomes: Biomes,
    render: RenderAssets,
    visibility: horizon_world::block_render::FaceVisibility,
    tables: MeshTables,
}

fn fixture() -> Fixture {
    let root = std::path::PathBuf::from(
        std::env::var_os("HORIZON_TEST_RESOURCES").expect("HORIZON_TEST_RESOURCES"),
    );
    let source = horizon_assets::resource_source::ResourceRoot::directory(root.clone());
    let assets = Assets::load(&root).unwrap();
    let facts = BlockFacts::load(&root, assets.registry).unwrap();
    let biomes = Biomes::load(&source).unwrap();
    let render = assets.reload(&source).unwrap();
    let visibility = models::load_visibility(&root, assets.registry).unwrap();
    let tables = MeshTables::new(&assets, &render, &facts, visibility.clone(), None).unwrap();
    Fixture {
        assets,
        biomes,
        render,
        visibility,
        tables,
    }
}

fn state(f: &Fixture, name: &str) -> u32 {
    f.assets.registry.default_state(name).unwrap().0
}

const BASE: i32 = MIN_Y + 4;

fn mesh(f: &Fixture, chunk: &Chunk, recipes: Option<&MeshRecipes>) -> SectionMesh {
    let region = [[Some(chunk); 3]; 3];
    let mut mesher = Mesher {
        nether_shading: false,
        shader_pack_lighting: false,
        separate_ao: false,
        shader_pack: false,
        assets: &f.assets,
        render: &f.render,
        tables: &f.tables,
        biomes: &f.biomes,
        visibility: f.visibility.clone(),
        zoom: BiomeZoom::new(0),
        blend_radius: 0,
        smooth_lighting: true,
        cutout_leaves: true,
        opaque_ice: false,
        translucent_layers: &[],
        leaf_culling: LeafCullingMode::Off,
        hidden_states: &[],
    };
    let mut scratch = MeshScratch::default();
    mesher.mesh_planned(&region, 0, SectionMesh::default(), &mut scratch, None, recipes)
}

fn quads(mesh: &SectionMesh) -> usize {
    (mesh.solid.len() + mesh.translucent.len()) / 4
}

fn stone(f: &Fixture, cells: &[[i32; 3]]) -> Chunk {
    let mut chunk = Chunk::empty(0, 0);
    for &[x, y, z] in cells {
        chunk.set(x, BASE + y, z, state(f, "minecraft:stone"));
    }
    chunk
}

fn top(f: &Fixture, cull: Option<u8>) -> ResolvedQuad {
    let layer = f.render.sprite_layers["minecraft:block/gold_block"] as u16;
    ResolvedQuad::new(
        [[0.0, 1.01, 0.0], [0.0, 1.01, 1.0], [1.0, 1.01, 1.0], [1.0, 1.01, 0.0]],
        [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
        0xFFFF_FFFF,
        layer,
        cull,
        false,
    )
}

fn table(f: &Fixture, edit: impl FnOnce(&mut MeshRecipesBuilder, u32)) -> MeshRecipes {
    let mut builder = MeshRecipesBuilder::new(f.assets.registry.len());
    edit(&mut builder, state(f, "minecraft:stone"));
    builder.finish().unwrap()
}

#[test]
#[ignore = "daily"]
fn recipes_replace_append_and_cull_through_the_mesher() {
    let f = fixture();
    let one = stone(&f, &[[8, 0, 8]]);
    assert_eq!(quads(&mesh(&f, &one, None)), 6);
    let empty = table(&f, |b, s| b.add(s, true, None, &[]));
    assert_eq!(quads(&mesh(&f, &one, Some(&empty))), 0, "no retained original six");
    let append = table(&f, |b, s| b.add(s, false, None, &[top(&f, None)]));
    assert_eq!(quads(&mesh(&f, &one, Some(&append))), 7);
    let two = stone(&f, &[[8, 0, 8], [9, 0, 8]]);
    assert_eq!(quads(&mesh(&f, &two, None)), 10);
    let other = table(&f, |b, _| b.add(0, false, None, &[]));
    assert_eq!(quads(&mesh(&f, &two, Some(&other))), 10, "a table for other states");
    // Stacked: the lower cube's up face is culled, so its quad culled by up goes too.
    let stacked = stone(&f, &[[8, 0, 8], [8, 1, 8]]);
    let culled = table(&f, |b, s| b.add(s, false, None, &[top(&f, Some(1))]));
    assert_eq!(quads(&mesh(&f, &stacked, Some(&culled))), 10 + 1);
    // A replacement that declares no occlusion opens its neighbours' faces.
    let open = table(&f, |b, s| b.add(s, true, None, &[top(&f, None)]));
    assert_eq!(quads(&mesh(&f, &two, Some(&open))), 2);
}

#[test]
#[ignore = "daily"]
fn without_recipes_meshes_are_byte_identical() {
    let f = fixture();
    let mut chunk = Chunk::empty(0, 0);
    let names = ["minecraft:stone", "minecraft:glass", "minecraft:water", "minecraft:oak_leaves"];
    for x in 0..16 {
        for z in 0..16 {
            for y in 0..6 {
                if let Some(name) = names.get((x * 7 + z * 3 + y * 5) as usize % 6) {
                    chunk.set(x, BASE + y, z, state(&f, name));
                }
            }
        }
    }
    let before = mesh(&f, &chunk, None);
    // A table naming only a state the section does not hold meshes the same bytes.
    let unused = table(&f, |b, _| b.add(0, false, None, &[]));
    let with = mesh(&f, &chunk, Some(&unused));
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&before.solid),
        bytemuck::cast_slice::<_, u8>(&with.solid)
    );
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&before.translucent),
        bytemuck::cast_slice::<_, u8>(&with.translucent)
    );
    assert_eq!(before.visibility, with.visibility);
    assert!(quads(&before) > 100);
}
