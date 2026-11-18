#[cfg(test)]
mod tests {
    use crate::game::native_entity_refs::tests::with_game;
    use mod_api::{BlockEntityPart, BlockEntitySaveAdditionalEvent, Hooks, LevelHost, ModEntry};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI32, Ordering};
    use sunrise_client::{
        chunk::Chunk,
        containers::{BlockEntity, ModBlockEntity, ModBlockEntityFields},
    };
    use sunrise_core::nbt::{Compound, Tag};

    struct Counter(Arc<AtomicI32>);
    #[mod_api::sunrise_part]
    impl mod_api::BlockPart for Counter {}
    #[mod_api::sunrise_part]
    impl BlockEntityPart for Counter {
        fn block_entity_save_additional(
            &self,
            _: &dyn LevelHost,
            event: &mut BlockEntitySaveAdditionalEvent<'_>,
        ) {
            assert_eq!(event.tag.get(&"count".into()), Some(&Tag::Int(7)));
            event.tag.insert(
                "count".into(),
                Tag::Int(self.0.fetch_add(1, Ordering::Relaxed)),
            );
            event
                .tag
                .insert("id".into(), Tag::String("test:wrong".into()));
        }
    }
    struct Mod(Arc<AtomicI32>);
    #[mod_api::__subscriptions]
    impl Hooks for Mod {
        fn register(
            &self,
            registrar: &mut dyn mod_api::Registrar,
            _: &mod_api::RegistrationContext,
        ) {
            let counter = registrar.block_entity_type("counter").build();
            registrar
                .block_entity_part(&counter, Counter(self.0.clone()))
                .unwrap();
            registrar.block_entity_type("silent").build();
        }
    }

    #[test]
    #[ignore = "daily"]
    fn command_and_unload_capture_the_live_payload_without_mutating_storage() {
        let count = Arc::new(AtomicI32::new(11));
        mod_platform::testing::with_mods(
            vec![ModEntry::new("test", Box::new(Mod(count.clone())))].into(),
            || {
                with_game(|game| {
                    mod_api::__private::resolve_block_entity_types(
                        ["test:counter", "test:silent"],
                        |_| Some(()),
                    )
                    .unwrap();
                    game.world
                        .chunks
                        .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
                    let pos = (1, 64, 1);
                    let silent = (2, 64, 1);
                    for (pos, kind) in [(pos, "test:counter"), (silent, "test:silent")] {
                        game.world.block_entities.insert(
                            pos,
                            BlockEntity::Mod(Box::new(ModBlockEntity::new(
                                kind.into(),
                                ModBlockEntityFields {
                                    nbt: Compound::from([("count".into(), Tag::Int(7))]),
                                    ..Default::default()
                                },
                            ))),
                        );
                    }
                    let tag = game.block_entity_nbt([1, 64, 1]).unwrap();
                    assert_eq!(tag.get("count"), Some(&Tag::Int(11)));
                    assert_eq!(tag.get("id"), Some(&Tag::String("test:counter".into())));

                    let detached = game.world.chunks.remove(&(0, 0)).unwrap();
                    let capture = game.with_entities([&detached]);
                    let fields = |entity: &BlockEntity| {
                        let BlockEntity::Mod(entity) = entity else {
                            panic!("expected a mod block entity")
                        };
                        entity.nbt.get(&"count".into()).cloned()
                    };
                    let snapshots = &capture[0].block_entities;
                    assert_eq!(
                        fields(&snapshots.iter().find(|(p, _)| *p == pos).unwrap().1),
                        Some(Tag::Int(12))
                    );
                    assert_eq!(
                        fields(&snapshots.iter().find(|(p, _)| *p == silent).unwrap().1),
                        Some(Tag::Int(7))
                    );
                    assert_eq!(
                        fields(game.world.block_entities.get(&pos).unwrap()),
                        Some(Tag::Int(7))
                    );
                    assert_eq!(count.load(Ordering::Relaxed), 13);
                })
            },
        );
    }
}
