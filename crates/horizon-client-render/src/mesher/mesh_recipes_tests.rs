//! The section meshing design's recipe rules that need no resources: composition, occlusion, the
//! solid rows they open, the section budget, and the API's whole-recipe check.
use super::super::{Padded, pi};
use super::*;

fn quad(cull: Option<u8>) -> ResolvedQuad {
    ResolvedQuad::new(
        [[0.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 0.0]],
        [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
        0xFFFF_FFFF,
        3,
        cull,
        false,
    )
}

#[test]
fn no_recipe_is_no_table() {
    assert!(MeshRecipesBuilder::new(8).finish().is_none());
}

#[test]
fn recipes_compose_by_state() {
    let mut builder = MeshRecipesBuilder::new(8);
    builder.add(2, false, None, &[quad(None)]);
    builder.add(2, true, Some(0b11_0011), &[quad(Some(1))]);
    builder.add(2, false, Some(0b10_0001), &[]);
    builder.add(5, false, None, &[quad(None), quad(None)]);
    let table = builder.finish().unwrap();
    let two = table.get(2).unwrap();
    assert!(two.replaces(), "one replacement drops the model");
    assert_eq!(two.quads(), 2, "every mod's quads draw");
    assert_eq!(two.occlusion(), Some(0b10_0001), "declarations intersect");
    let five = table.get(5).unwrap();
    assert!(!five.replaces());
    assert_eq!(five.occlusion(), None, "an append keeps the block's own occlusion");
    assert!(table.get(0).is_none());
    assert!(table.get(99).is_none(), "a state past the table has none");
}

#[test]
fn a_replacement_occludes_only_what_it_declares() {
    let mut builder = MeshRecipesBuilder::new(4);
    builder.add(1, true, None, &[]);
    builder.add(2, true, Some(63), &[]);
    let table = builder.finish().unwrap();
    assert_eq!(table.get(1).unwrap().occlusion(), Some(0));
    assert_eq!(table.get(2).unwrap().occlusion(), Some(63));
    let mut pad = Padded {
        blocks: vec![0; P * P * P],
        light: Vec::new(),
    };
    pad.blocks[pi(3, 0, 0)] = 1;
    pad.blocks[pi(4, 0, 0)] = 2;
    pad.blocks[pi(16, 0, 0)] = 1;
    let mut rows = vec![u32::MAX; P * P];
    open_recipes(&table, &pad, &mut rows);
    let row = rows[B as usize * P + B as usize];
    assert_eq!(row & 1 << (3 + B), 0, "nothing declared: no longer solid");
    assert_ne!(row & 1 << (4 + B), 0, "every face declared: stays as it was");
    assert_eq!(row & 1 << (16 + B), 0, "the border cell too");
}

#[test]
fn the_section_budget_counts_every_added_quad() {
    let mut builder = MeshRecipesBuilder::new(4);
    builder.add(1, false, None, &[quad(None), quad(None)]);
    let table = builder.finish().unwrap();
    let mut pad = Padded {
        blocks: vec![1; P * P * P],
        light: Vec::new(),
    };
    assert_eq!(added_quads(&table, &pad), 2 * 4096, "only the section's own cells");
    pad.blocks.fill(0);
    assert_eq!(added_quads(&table, &pad), 0);
}

#[test]
fn the_api_checks_recipes_whole() {
    use mod_api::{MeshQuad, MeshRecipeRefusal as R, MeshRecipeTarget as T, StateMeshRecipe};
    let good = || {
        MeshQuad::new(
            [[0.0, 0.0, 0.0], [0.0, 2.0, 0.0], [-1.0, 2.0, 0.0], [-1.0, 0.0, 0.0]],
            [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            "minecraft:block/stone",
        )
    };
    let target = || T::Block("minecraft:stone".into());
    assert_eq!(StateMeshRecipe::append(target(), vec![good()]).check(), Ok(()));
    let mut nan = good();
    nan.positions[2][1] = f32::NAN;
    assert_eq!(
        StateMeshRecipe::append(target(), vec![good(), nan]).check(),
        Err(R::NotFinite { quad: 1 })
    );
    let mut far = good();
    far.positions[0][0] = 2.5;
    assert_eq!(
        StateMeshRecipe::replace(target(), vec![far]).check(),
        Err(R::OutOfRange { quad: 0 })
    );
    assert_eq!(
        StateMeshRecipe::append(target(), vec![good().culled_by(6)]).check(),
        Err(R::FaceMask)
    );
    assert_eq!(
        StateMeshRecipe::replace(target(), vec![]).occluding(64).check(),
        Err(R::FaceMask)
    );
    let many = vec![good(); mod_api::MAX_RECIPE_QUADS + 1];
    assert_eq!(
        StateMeshRecipe::append(target(), many).check(),
        Err(R::TooManyQuads { count: 257 })
    );
}
