use super::*;
use crate::game::native_entity_refs::tests::{with_client_game, with_game};
use mod_api::{BuildCreativeModeTabContentsEvent, Hooks, HostSide, ModEntry, prelude::ItemsHost};
use std::sync::Mutex;

#[derive(Default)]
struct Seen {
    events: Vec<(LogicalSide, CreativeTabPermissions, String)>,
    token: Option<CreativeTabEdit>,
}

struct Edit(Arc<Mutex<Seen>>);

fn stack(id: &str, count: u32) -> mod_api::ItemStack {
    mod_api::ItemStack::new(mod_api::__private::item_key(id).unwrap(), count).unwrap()
}

#[mod_api::__subscriptions]
impl Hooks for Edit {
    fn build_creative_mode_tab_contents(
        &self,
        host: HostSide,
        event: &mut BuildCreativeModeTabContentsEvent<'_>,
    ) {
        let (host, side): (&mut dyn mod_api::Host, _) = match host {
            HostSide::Client(host) => (host, LogicalSide::Client),
            HostSide::Server(host) => (host, LogicalSide::Server),
        };
        {
            let mut seen = self.0.lock().unwrap();
            seen.events
                .push((side, event.permissions, event.tab.to_owned()));
            seen.token = Some(event.edit);
        }
        if event.tab != "test:a" {
            return;
        }
        let previous = host.creative_tabs(event.permissions).unwrap();
        let b = &previous.tab("test:b").unwrap().display_items;
        match event.permissions {
            CreativeTabPermissions::Normal => assert!(b.is_empty()),
            CreativeTabPermissions::Operator => assert_eq!(b, &[stack("minecraft:sand", 1)]),
        }
        let other = match event.permissions {
            CreativeTabPermissions::Normal => CreativeTabPermissions::Operator,
            CreativeTabPermissions::Operator => CreativeTabPermissions::Normal,
        };
        assert!(matches!(
            host.creative_tabs(other),
            Err(CreativeTabReadError::Reentrant)
        ));
        assert_eq!(
            event.edit.insert(
                host,
                CreativeTabPlacement::Last,
                stack("minecraft:dirt", 2),
                CreativeTabVisibility::Both
            ),
            Err(CreativeTabEditError::Count),
        );
        assert_eq!(
            event.edit.insert(
                host,
                CreativeTabPlacement::Before(stack("minecraft:stone", 42)),
                stack("minecraft:dirt", 1),
                CreativeTabVisibility::Both
            ),
            Err(CreativeTabEditError::MissingAnchor(CreativeTabList::Search)),
        );
        // The disabled barrier still anchors the insertion during the event.
        event
            .edit
            .insert(
                host,
                CreativeTabPlacement::Before(stack("minecraft:barrier", 2)),
                stack("minecraft:torch", 1),
                CreativeTabVisibility::Search,
            )
            .unwrap();
    }
}

#[test]
#[ignore = "daily"]
fn creative_catalog_builds_lazily_and_edits_before_filtering_on_both_sides() {
    use crate::game::{execution_role::ExecutionRole, native_client_session::tests::logged_client};
    use sunrise_client::creative::Creative;
    let row = |item: &str, visibility: &str| {
        serde_json::json!({
            "kind":"stack", "snbt":format!(r#"{{id:"{item}",count:1}}"#), "visibility":visibility,
        })
    };
    let both = "PARENT_AND_SEARCH_TABS";
    let catalog = serde_json::json!({
        "schema":4, "item_features":{"minecraft:barrier":["test:disabled"]}, "potion_features":{},
        "tabs":[
            {"id":"test:a","type":"CATEGORY","build":[
                row("minecraft:stone", "PARENT_TAB_ONLY"), row("minecraft:stick", "SEARCH_TAB_ONLY"), row("minecraft:barrier", both)
            ]},
            {"id":"minecraft:hotbar","type":"HOTBAR","build":[]},
            {"id":"test:b","type":"CATEGORY","build":[row("minecraft:sand", both)],"operator_build":[row("minecraft:glass", both)]},
            {"id":"minecraft:search","type":"SEARCH","build":[]},
            {"id":"minecraft:inventory","type":"INVENTORY","build":[]},
        ]
    });
    let seen = Arc::new(Mutex::new(Seen::default()));
    let setups = vec![ModEntry::new("tab_edit", Box::new(Edit(seen.clone())))].into();
    let check = |game: &mut Game<'_>, side| {
        let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
        game.creative_screen = Creative::from_json(&catalog).unwrap();
        assert!(!game.creative_screen.has_contents());
        assert!(seen.lock().unwrap().events.is_empty());
        let flags = |game: &mut Game<'_>, ids: [&str; 2]| {
            let flags = ids.map(str::to_owned).into();
            if let Some(remote) = &mut game.remote {
                remote.features = flags;
            } else {
                game.level.enabled_features = flags;
            }
        };
        flags(game, ["test:alpha", "test:beta"]);
        let normal = ItemsHost::creative_tabs(game, CreativeTabPermissions::Normal).unwrap();
        let a = normal.tab("test:a").unwrap();
        assert_eq!(
            a.display_items,
            [stack("minecraft:dirt", 1), stack("minecraft:stone", 1)]
        );
        assert_eq!(
            a.search_items,
            [stack("minecraft:stick", 1), stack("minecraft:torch", 1)]
        );
        assert!(!a.contains(&stack("minecraft:dirt", 1)));
        assert!(a.contains(&stack("minecraft:torch", 8)));
        flags(game, ["test:beta", "test:alpha"]);
        let host: &mut dyn mod_api::Host = game;
        let cached = host.creative_tabs(CreativeTabPermissions::Normal).unwrap();
        assert!(Arc::ptr_eq(&normal, &cached));
        let old = seen.lock().unwrap().token.unwrap();
        assert_eq!(
            old.entries(host, CreativeTabList::Search),
            Err(CreativeTabEditError::Stale)
        );
        let operator = host
            .creative_tabs(CreativeTabPermissions::Operator)
            .unwrap();
        assert!(!Arc::ptr_eq(&normal, &operator));
        assert_eq!(
            operator.tab("minecraft:search").unwrap().display_items,
            [
                stack("minecraft:stick", 1),
                stack("minecraft:torch", 1),
                stack("minecraft:glass", 1),
            ]
        );
        assert_eq!(
            normal.tab("test:b").unwrap().display_items,
            [stack("minecraft:sand", 1)]
        );
        let actual = std::mem::take(&mut seen.lock().unwrap().events);
        let expected: Vec<_> = [
            CreativeTabPermissions::Normal,
            CreativeTabPermissions::Operator,
        ]
        .into_iter()
        .flat_map(|permissions| {
            [
                "test:a",
                "test:b",
                "minecraft:hotbar",
                "minecraft:search",
                "minecraft:inventory",
            ]
            .map(|tab| (side, permissions, tab.to_owned()))
        })
        .collect();
        assert_eq!(actual, expected);

        // A replacement provider drops the last build without invalidating an Arc
        // the caller kept. It must not dispatch until the next read.
        if let Some(remote) = &game.remote {
            let replacement = Arc::new((*remote.registries).clone());
            game.set_registries(replacement);
        } else {
            game.reset_creative_contents();
        }
        assert!(seen.lock().unwrap().events.is_empty());
        let fresh = ItemsHost::creative_tabs(game, CreativeTabPermissions::Normal).unwrap();
        assert!(!Arc::ptr_eq(&normal, &fresh));
        assert_eq!(*normal, *fresh);
        assert_eq!(seen.lock().unwrap().events.len(), 5);
        seen.lock().unwrap().events.clear();
    };
    mod_platform::testing::with_mods(setups, || {
        with_game(|game| {
            game.options.execution_role = ExecutionRole::DedicatedServer;
            check(game, LogicalSide::Server);
        });
        with_client_game(|game| {
            let _server = logged_client(game);
            game.native_lifecycle
                .begin_open(mod_api::Authority::RemoteClient);
            check(game, LogicalSide::Client);
        });
    });
}
