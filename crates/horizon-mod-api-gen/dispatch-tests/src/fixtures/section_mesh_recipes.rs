mod stubs {
    #[derive(Debug, PartialEq)]
    pub(super) struct StateMeshRecipe(pub(super) &'static str);
}

/// The generated dispatch of `section_mesh_recipes` hands each mod an empty batch and names that
/// mod on every recipe it pushed, in mod order, so the game can refuse one mod's recipes whole.
#[test]
fn each_mesh_recipe_names_the_mod_that_pushed_it() {
    struct Recipes(&'static [&'static str]);
    #[mod_api::__subscriptions]
    impl Hooks for Recipes {
        fn section_mesh_recipes(&self, _: &dyn ClientHost, event: &mut SectionMeshRecipesEvent<'_>) {
            assert!(event.recipes.is_empty(), "each mod starts from its own batch");
            event.recipes.extend(self.0.iter().map(|r| StateMeshRecipe(r)));
        }
    }
    install(
        vec![
            ModEntry::new("bus", Box::new(Recipes(&["bus:cable", "bus:part"]))),
            ModEntry::new("quiet", Box::new(Recipes(&[]))),
            ModEntry::new("frame", Box::new(Recipes(&["frame:edge"]))),
        ]
        .into(),
    );
    let mut recipes = Vec::new();
    dispatch_section_mesh_recipes(
        &TestHost,
        calls::SectionMeshRecipesCall {
            recipes: &mut recipes,
        },
    );
    let entry = |id, mod_id| Answered {
        answer: StateMeshRecipe(id),
        mod_id,
    };
    assert_eq!(
        recipes,
        [entry("bus:cable", "bus"), entry("bus:part", "bus"), entry("frame:edge", "frame")]
    );
}
