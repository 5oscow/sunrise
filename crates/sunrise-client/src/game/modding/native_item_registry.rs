//! Item registry numbers shared by API metadata reads and the game's scatter seeds.
use super::Game;

impl Game<'_> {
    pub(super) fn item_registry_id(&self, key: mod_api::ItemKey) -> Option<u32> {
        self.item_number(mod_api::__private::item_key_id(key)?)
    }

    pub(in crate::game) fn item_number(&self, item: &str) -> Option<u32> {
        if let Some(remote) = &self.remote {
            return remote
                .builtin
                .get("minecraft:item")?
                .id(item)?
                .try_into()
                .ok();
        }
        // The prepared registry report includes mod items after vanilla, in content-plan order.
        self.pack.item_renderer.item_registry_id(item)
    }
}

#[cfg(test)]
mod tests {
    use super::super::native_entity_refs::tests::with_client_game;
    use mod_api::{Hooks, Identifier, ModEntry};
    use std::sync::Arc;
    use sunrise_protocol::registry::Builtin;

    struct Probe;
    #[mod_api::__subscriptions]
    impl Hooks for Probe {}

    #[test]
    #[ignore = "daily"]
    fn item_numbers_follow_the_connected_registry_for_api_and_scatter() {
        mod_platform::testing::with_mods(
            vec![ModEntry::new("numbers", Box::new(Probe))].into(),
            || {
                with_client_game(|game| {
                    let (pipe, _server) = sunrise_protocol::connection::Pipe::pair();
                    game.connect_local(pipe, "Steve").unwrap();
                    let stone = game
                        .item_key(&Identifier::parse("minecraft:stone").unwrap())
                        .unwrap();
                    let report = Builtin::parse(
                        r#"{"minecraft:item":{"entries":{
                    "minecraft:air":{"protocol_id":0},
                    "minecraft:stone":{"protocol_id":1},
                    "test:meal":{"protocol_id":2}
                }}}"#,
                    )
                    .unwrap();
                    game.remote.as_mut().unwrap().builtin = Arc::new(report);
                    assert_eq!(stone.registry_id(game), Some(1));
                    assert_eq!(game.item_number("minecraft:stone"), Some(1));
                    assert_eq!(game.item_number("test:meal"), Some(2));
                    assert_eq!(game.item_number("test:missing"), None);
                    let report = Builtin::parse(
                        r#"{"minecraft:item":{"entries":{
                    "minecraft:air":{"protocol_id":0},
                    "test:meal":{"protocol_id":1},
                    "minecraft:stone":{"protocol_id":2}
                }}}"#,
                    )
                    .unwrap();
                    game.remote.as_mut().unwrap().builtin = Arc::new(report);
                    assert_eq!(stone.registry_id(game), Some(2));
                    assert_eq!(game.item_number("test:meal"), Some(1));
                })
            },
        );
    }
}
