//! `section_mesh_recipes` through the generated host/guest wire against the same listener called
//! natively: the recipes, a malformed one too, come back as the native listener gives them, so
//! the game checks and resolves identical tables on native and wasm.
use super::*;
use mod_api::{MeshQuad, MeshRecipeRefusal, MeshRecipeTarget, StateMeshRecipe};

fn recipes() -> Vec<StateMeshRecipe> {
    let quad = MeshQuad::new(
        [[0.0, 1.001, 0.0], [0.0, 1.001, 1.0], [1.0, 1.001, 1.0], [1.0, 1.001, 0.0]],
        [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [0.25, 0.0]],
        "minecraft:block/gold_block",
    );
    let mut nan = quad.clone();
    nan.positions[1][2] = f32::NAN;
    vec![
        StateMeshRecipe::append(
            MeshRecipeTarget::Block("minecraft:stone".into()),
            vec![quad.clone().culled_by(1).tinted(0x80FF_2040)],
        ),
        StateMeshRecipe::replace(MeshRecipeTarget::State(mod_api::BlockStateId(9)), vec![])
            .occluding(0b00_0011),
        StateMeshRecipe::append(
            MeshRecipeTarget::State(mod_api::BlockStateId(3)),
            vec![quad.translucent(), nan],
        ),
    ]
}

pub(super) struct SectionMeshRecipesHooks;
impl SectionMeshRecipesHooks {
    pub(super) fn section_mesh_recipes(
        &self,
        _: &dyn mod_api::ClientHost,
        event: &mut mod_api::SectionMeshRecipesEvent<'_>,
    ) {
        assert!(event.recipes.is_empty(), "the guest starts from its own batch");
        event.recipes.extend(recipes());
    }
}

#[test]
fn wire_loopback_section_mesh_recipes_match_native() {
    mod_api::__private::initialize().unwrap();
    let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
    struct Client;
    impl mod_api::Host for Client {}
    mod_api::impl_host_areas!(Client);
    impl mod_api::ClientHost for Client {}
    mod_api::impl_client_host_areas!(Client);
    install_counter();
    let module = WasmMod {
        group: 0,
        subscribed: &[],
        listeners: &[],
        corrupt: false,
    };
    let mut answers = Vec::new();
    module.section_mesh_recipes(
        &Client,
        &mut mod_api::SectionMeshRecipesEvent {
            recipes: &mut answers,
        },
    );
    let native = recipes();
    // NaN is not equal to itself: compare the bits.
    assert_eq!(format!("{answers:?}"), format!("{native:?}"));
    let checks: Vec<_> = answers.iter().map(StateMeshRecipe::check).collect();
    assert_eq!(
        checks,
        [Ok(()), Ok(()), Err(MeshRecipeRefusal::NotFinite { quad: 1 })]
    );
}
