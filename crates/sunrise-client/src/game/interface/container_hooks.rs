//! The open container screen as `sunrise_hooks::containers` offers it (docs/layers.md, "Container
//! screens"): the engine's slot clicks and queries the inventory extras use, the `Container` view
//! over them, and the dispatch of input and drawing to the crates that registered hooks. With none
//! registered nothing here builds a view.
use crate::game::{Game, Screen};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use sunrise_client::{
    containers::{BlockEntity, FurnaceKind},
    extras_hud::argb_color,
    hud::{HudAtlas, HudBatch},
    items::ItemStack,
    menu::Input,
    stations::hover_name,
};
use sunrise_hooks::{
    containers::{self as api, Click, Container, Panel, Role, Slot, SlotId, Stack},
    hud,
};
use winit::keyboard::KeyCode;

// Views built since the test began: none without a registrant.
#[cfg(test)]
thread_local! {
    pub(in crate::game) static BUILT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The game a view reads, and writes through when it was built for input.
enum Backing<'a, 'g> {
    Shared(&'a Game<'g>),
    Owned(&'a mut Game<'g>),
}

/// The open screen for one call. Drawing's view is read-only: its clicks and rule queries do
/// nothing.
pub(in crate::game) struct Open<'a, 'g> {
    game: Backing<'a, 'g>,
    menu: String,
    origin: (f32, f32),
    /// The stacks handed out by what tells them apart, for the queries that take one back.
    seen: RefCell<HashMap<u64, ItemStack>>,
}

/// The slot ids' roles, from the game's numbering.
fn role(slot: usize) -> Role {
    match slot {
        0..9 => Role::Hotbar,
        9..36 => Role::Inventory,
        100..109 => Role::Input,
        200 => Role::Result,
        300..500 => Role::Storage,
        500..=504 => Role::Equipment,
        _ => Role::Other,
    }
}

/// What a world's own open container is, for its menu id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum Local {
    Inventory,
    Crafting,
    /// A chest-like block: `rows` when it is a generic 9-wide menu, the container's `size`.
    Chest {
        rows: Option<usize>,
        shulker: bool,
        size: usize,
    },
    Furnace(FurnaceKind),
    Brewing,
    Crafter,
}

/// Vanilla's menu type id for a world's own container: what a server would send for the same.
pub(in crate::game) fn local_menu_id(local: Local) -> Option<String> {
    let id = match local {
        Local::Inventory => "inventory".into(),
        Local::Crafting => "minecraft:crafting".into(),
        Local::Chest {
            rows: Some(rows), ..
        } => format!("minecraft:generic_9x{rows}"),
        Local::Chest { shulker: true, .. } => "minecraft:shulker_box".into(),
        Local::Chest { size: 5, .. } => "minecraft:hopper".into(),
        Local::Chest { size: 9, .. } => "minecraft:generic_3x3".into(),
        Local::Chest { .. } => return None,
        Local::Furnace(FurnaceKind::Furnace) => "minecraft:furnace".into(),
        Local::Furnace(FurnaceKind::BlastFurnace) => "minecraft:blast_furnace".into(),
        Local::Furnace(FurnaceKind::Smoker) => "minecraft:smoker".into(),
        Local::Brewing => "minecraft:brewing_stand".into(),
        Local::Crafter => "minecraft:crafter_3x3".into(),
    };
    Some(id)
}

/// What tells two stacks apart that don't merge: the item, its damage and its components.
fn kind_of(stack: &ItemStack) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (&*stack.item, stack.damage).hash(&mut hasher);
    match stack.data() {
        // Enchantments compare as a multiset, so they are sorted before they are printed.
        Some(data) if data.enchantments.len() > 1 || data.stored_enchantments.len() > 1 => {
            let mut data = data.clone();
            data.enchantments.sort();
            data.stored_enchantments.sort();
            format!("{:?}", Some(&data))
        }
        data => format!("{data:?}"),
    }
    .hash(&mut hasher);
    hasher.finish()
}

impl<'g> Open<'_, 'g> {
    fn game(&self) -> &Game<'g> {
        match &self.game {
            Backing::Shared(game) => game,
            Backing::Owned(game) => game,
        }
    }
    fn game_mut(&mut self) -> Option<&mut Game<'g>> {
        match &mut self.game {
            Backing::Shared(_) => None,
            Backing::Owned(game) => Some(game),
        }
    }
    fn stack_of(&self, stack: &ItemStack) -> Stack {
        let kind = kind_of(stack);
        self.seen.borrow_mut().entry(kind).or_insert_with(|| stack.clone());
        Stack::new(
            stack.item.to_string(),
            stack.count,
            self.game().max_stack(stack),
            kind,
        )
    }
}

impl Container for Open<'_, '_> {
    fn menu(&self) -> &str {
        &self.menu
    }
    fn slots(&self) -> Vec<Slot> {
        let game = self.game();
        game.slots()
            .into_iter()
            .map(|i| {
                let (x, y) = game.slot_position(i);
                Slot::new(SlotId(i as u32), role(i), x, y)
            })
            .collect()
    }
    fn stack(&mut self, slot: SlotId) -> Option<Stack> {
        let stack = self.game().slot_view(slot.0 as usize)?;
        Some(self.stack_of(&stack))
    }
    fn carried(&self) -> Option<Stack> {
        self.game().carried.as_ref().map(|s| self.stack_of(s))
    }
    fn takes_one(&mut self, slot: SlotId, of: &Stack) -> bool {
        let Some(stack) = self.seen.borrow().get(&of.kind).cloned() else {
            return false;
        };
        self.game_mut()
            .is_some_and(|g| g.tweak_takes_one(slot.0 as usize, &stack))
    }
    fn landing(&mut self, slot: SlotId) -> Vec<SlotId> {
        self.game_mut()
            .map(|g| g.tweak_landing(slot.0 as usize))
            .unwrap_or_default()
            .into_iter()
            .map(|i| SlotId(i as u32))
            .collect()
    }
    fn click(&mut self, slot: SlotId, click: Click) -> bool {
        let slot = slot.0 as usize;
        let Some(game) = self.game_mut() else {
            return false;
        };
        match click {
            Click::Pick { right } => game.tweak_click(slot, i32::from(right), Input::Pickup),
            Click::QuickMove => game.tweak_click(slot, 0, Input::QuickMove),
            Click::Put { one } => game.tweak_put(slot, one),
        }
    }
    fn spread(&self) -> usize {
        self.game()
            .inv_drag
            .as_ref()
            .map_or(0, |(_, slots)| slots.len())
    }
    fn cursor(&self) -> (f32, f32) {
        let (x, y) = self.game().cursor;
        (x - self.origin.0, y - self.origin.1)
    }
    fn shift(&self) -> bool {
        self.game().has_shift_down()
    }
    fn order(&mut self, slot: SlotId) -> (u32, String) {
        let game = self.game();
        let Some(stack) = game.slot_view(slot.0 as usize) else {
            return (u32::MAX, String::new());
        };
        let order = game.creative_screen.order_of(&stack);
        (
            order.and_then(|o| u32::try_from(o).ok()).unwrap_or(u32::MAX),
            hover_name(&game.pack.names, &stack),
        )
    }
    fn panel(&self) -> Panel {
        let game = self.game();
        let height = game.screen_height();
        // titleLabelY and inventoryLabelY (imageHeight - 94).
        Panel::new(176.0, height, 6.0, height - 94.0)
    }
}

/// What a hook draws with, from the panel's corner.
struct Canvas<'a> {
    batch: &'a mut HudBatch,
    atlas: &'a HudAtlas,
    origin: (f32, f32),
}

impl hud::Canvas for Canvas<'_> {
    fn size(&self) -> (f32, f32) {
        (self.batch.width, self.batch.height)
    }
    fn tr(&self, key: &str) -> String {
        sunrise_client::lang::get(key)
    }
    fn text_width(&self, text: &str) -> f32 {
        HudBatch::text_width(self.atlas, text)
    }
    fn text(&mut self, x: f32, y: f32, text: &str, argb: u32, shadow: bool) {
        let (ox, oy) = self.origin;
        self.batch
            .text(self.atlas, text, ox + x, oy + y, argb_color(argb), shadow);
    }
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, argb: u32) {
        let (ox, oy) = self.origin;
        self.batch
            .fill(self.atlas, ox + x, oy + y, w, h, argb_color(argb));
    }
    fn place(&mut self, _: hud::Corner, _: f32, _: f32) -> Option<(f32, f32)> {
        None
    }
}

impl<'g> Game<'g> {
    /// The menu type id of the open screen when container hooks act on it: the player's own
    /// screen and the containers vanilla has a menu type for, not creative's tabs, stations,
    /// mounts or a mod's screen.
    fn container_menu_id(&self) -> Option<String> {
        if self.screen != Screen::Inventory
            || self.station.is_some()
            || self.mount_active()
            || self.native_screen_shown()
        {
            return None;
        }
        if self.remote.is_some() {
            return Some(self.remote_menu_type().unwrap_or("inventory").to_owned());
        }
        local_menu_id(match self.container_kind() {
            None if self.craft_size == 3 => Local::Crafting,
            None => Local::Inventory,
            Some(BlockEntity::Chest(_)) => Local::Chest {
                rows: self.chest_rows(),
                shulker: self.container_block_is_shulker(),
                size: self.open_container_size(),
            },
            Some(BlockEntity::Furnace(f)) => Local::Furnace(f.kind),
            Some(BlockEntity::Brewing(_)) => Local::Brewing,
            Some(BlockEntity::Crafter(_)) => Local::Crafter,
            Some(_) => return None,
        })
    }
    /// The panel's corner, as the screen is drawn now.
    fn container_origin(&self) -> (f32, f32) {
        self.renderer
            .as_ref()
            .map(|r| {
                let batch = HudBatch::new(
                    [r.config.width, r.config.height],
                    self.settings.gui_scale_inputs(),
                );
                self.inventory_origin(&batch)
            })
            .unwrap_or_default()
    }
    /// The view for input, when a hook is registered and acts on this screen.
    fn open_container<'a>(&'a mut self) -> Option<Open<'a, 'g>> {
        if !api::engine::any() {
            return None;
        }
        let menu = self.container_menu_id()?;
        #[cfg(test)]
        BUILT.with(|n| n.set(n.get() + 1));
        let origin = self.container_origin();
        Some(Open {
            game: Backing::Owned(self),
            menu,
            origin,
            seen: RefCell::default(),
        })
    }
    /// A taken press or drag ends vanilla's quick craft drag.
    fn container_took_drag(&mut self) {
        self.inv_drag = None;
        self.tweaks.end_drag();
        self.inv_clicks.skip_release();
    }
    /// A press on the panel: true when a hook took it.
    pub(in crate::game) fn container_press(&mut self, slot: Option<usize>, right: bool) -> bool {
        if !api::engine::any() {
            return false;
        }
        self.inv_held = true;
        let Some(mut open) = self.open_container() else {
            return false;
        };
        let taken = api::engine::press(&mut open, slot.map(|s| SlotId(s as u32)), right);
        if taken {
            self.container_took_drag();
        }
        taken
    }
    /// The cursor moved with a button held: true when a hook took it.
    pub(in crate::game) fn container_drag(&mut self) -> bool {
        if !api::engine::any() || !self.inv_held {
            return false;
        }
        let slot = self.inventory_target().0;
        let Some(mut open) = self.open_container() else {
            return false;
        };
        let taken = api::engine::drag(&mut open, slot.map(|s| SlotId(s as u32)));
        if taken {
            self.container_took_drag();
        }
        taken
    }
    pub(in crate::game) fn container_release(&mut self) {
        self.inv_held = false;
        if let Some(mut open) = self.open_container() {
            api::engine::release(&mut open);
        }
    }
    /// The wheel over the panel: true when a hook took it.
    pub(in crate::game) fn container_wheel(&mut self, steps: f32) -> bool {
        if !api::engine::any() {
            return false;
        }
        let slot = self.inventory_target().0;
        let Some(mut open) = self.open_container() else {
            return false;
        };
        api::engine::wheel(&mut open, slot.map(|s| SlotId(s as u32)), steps)
    }
    /// A key or mouse button down on the screen, unless a text field has the focus.
    pub(in crate::game) fn container_key(&mut self, key: &str) -> bool {
        if !api::engine::any() || self.recipe_search_focused() {
            return false;
        }
        let slot = self.inventory_target().0;
        let Some(mut open) = self.open_container() else {
            return false;
        };
        api::engine::key(&mut open, slot.map(|s| SlotId(s as u32)), key)
    }
    /// A mouse button down on the screen, by the name the key mappings use.
    pub(in crate::game) fn container_mouse(&mut self, button: winit::event::MouseButton) -> bool {
        api::engine::any()
            && self.container_key(&sunrise_client::keybinds::mouse_name(button))
    }
    /// A key press, by the name the key mappings use.
    pub(in crate::game) fn container_key_code(&mut self, code: KeyCode) -> bool {
        sunrise_client::keybinds::key_name(code).is_some_and(|name| self.container_key(name))
    }
    /// What the hooks draw over the panel whose corner is (`ox`, `oy`).
    pub(in crate::game) fn draw_container_hooks(&self, batch: &mut HudBatch, ox: f32, oy: f32) {
        if !api::engine::any() {
            return;
        }
        let Some(menu) = self.container_menu_id() else {
            return;
        };
        #[cfg(test)]
        BUILT.with(|n| n.set(n.get() + 1));
        let open = Open {
            game: Backing::Shared(self),
            menu,
            origin: (ox, oy),
            seen: RefCell::default(),
        };
        let mut canvas = Canvas {
            batch,
            atlas: &self.gui.atlas,
            origin: (ox, oy),
        };
        api::engine::draw(&open, &mut canvas);
    }
}

impl Game<'_> {
    /// One ordinary click on a game slot. False when nothing took it.
    pub(in crate::game) fn tweak_click(&mut self, slot: usize, button: i32, input: Input) -> bool {
        if self.remote.is_some() {
            return self.remote_menu_ops(&[(Some(slot), button, input)]);
        }
        if !self.click_open_inventory(Some(slot), button, input) {
            return false;
        }
        if let Some(key) = self.container {
            self.container_changed(key);
        }
        true
    }
    /// Puts the hand's stack (`right`: one of it) in `slot`, as a one-slot
    /// drag (`QUICK_CRAFT`) and not a click: a server that has the hand empty
    /// (the stack it was to come from just burned as fuel) ignores a drag, but
    /// a click would pick up what's in `slot`.
    pub(in crate::game) fn tweak_put(&mut self, slot: usize, right: bool) -> bool {
        let kind = i32::from(right) << 2;
        let ops = [
            (None, kind, Input::QuickCraft),
            (Some(slot), kind | 1, Input::QuickCraft),
            (None, kind | 2, Input::QuickCraft),
        ];
        if self.remote.is_some() {
            return self.remote_menu_ops(&ops);
        }
        if !self.menu_ops(&ops) {
            return false;
        }
        if let Some(key) = self.container {
            self.container_changed(key);
        }
        true
    }
    /// `slot` is a result slot of the open menu.
    pub(in crate::game) fn tweak_result(&mut self, slot: usize) -> bool {
        let mut result = false;
        self.menu_session(|menu, _, to_menu| {
            result = to_menu(slot).is_some_and(|i| menu.is_result(i));
            Some(false)
        });
        result
    }
    /// `slot` takes one more of `stack`, by the open menu's rules.
    pub(in crate::game) fn tweak_takes_one(&mut self, slot: usize, stack: &ItemStack) -> bool {
        let mut takes = false;
        self.menu_session(|menu, env, to_menu| {
            takes = to_menu(slot).is_some_and(|i| menu.takes_one(env, i, stack));
            Some(false)
        });
        takes
    }
    /// Where vanilla's quick move would put `slot`'s items, tried on a copy
    /// of the menu that's dropped (`Some(false)` keeps nothing).
    pub(in crate::game) fn tweak_landing(&mut self, slot: usize) -> Vec<usize> {
        let slots = self.slots();
        let mut landing = Vec::new();
        self.menu_session(|menu, env, to_menu| {
            let from = to_menu(slot)?;
            landing = menu
                .quick_move_landing(env, from)
                .into_iter()
                .filter_map(|i| slots.iter().copied().find(|&g| to_menu(g) == Some(i)))
                .collect();
            Some(false)
        });
        landing
    }
    /// The slots across from `slot`: a container's or crafting grid's face
    /// the inventory (hotbar first, from the right), the inventory's face
    /// them; without one, hotbar and main inventory trade.
    pub(in crate::game) fn other_side(&self, slot: usize) -> Vec<usize> {
        let table = self.craft_size == 3;
        let own: Vec<usize> = self
            .slots()
            .into_iter()
            .filter(|s| (300..500).contains(s) || (table && (100..109).contains(s)))
            .collect();
        let player = || (0..9).rev().chain((9..36).rev());
        match slot {
            100..109 | 300..500 => player().collect(),
            0..36 if !own.is_empty() => own,
            0..9 => (9..36).collect(),
            9..36 => (0..9).collect(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::game_tests::tests::server_game;
    use sunrise_hooks::containers::{ContainerHooks, engine::with_registrants};

    /// Takes every press.
    struct Take;
    impl ContainerHooks for Take {
        fn press(&self, _: &mut dyn Container, _: Option<SlotId>, _: bool) -> bool {
            true
        }
    }
    static TAKE: Take = Take;

    #[test]
    fn a_local_chest_and_a_servers_have_the_same_menu() {
        let chest = |rows| Local::Chest {
            rows: Some(rows),
            shulker: false,
            size: rows * 9,
        };
        assert_eq!(local_menu_id(chest(3)).as_deref(), Some("minecraft:generic_9x3"));
        assert_eq!(local_menu_id(chest(6)).as_deref(), Some("minecraft:generic_9x6"));
        let shulker = Local::Chest {
            rows: None,
            shulker: true,
            size: 27,
        };
        assert_eq!(local_menu_id(shulker).as_deref(), Some("minecraft:shulker_box"));
        assert_eq!(
            local_menu_id(Local::Furnace(FurnaceKind::Smoker)).as_deref(),
            Some("minecraft:smoker")
        );
    }

    #[test]
    fn stacks_that_merge_have_the_same_kind() {
        let book = |order: [&str; 2]| {
            let mut stack = ItemStack::new("minecraft:enchanted_book", 1);
            stack.data_mut().stored_enchantments =
                order.iter().map(|id| ((*id).into(), 1)).collect();
            stack
        };
        let (a, b) = (
            book(["minecraft:mending", "minecraft:unbreaking"]),
            book(["minecraft:unbreaking", "minecraft:mending"]),
        );
        assert_eq!(a, b);
        assert_eq!(kind_of(&a), kind_of(&b));
        assert_ne!(kind_of(&a), kind_of(&ItemStack::new("minecraft:enchanted_book", 1)));
    }

    #[test]
    #[ignore = "daily"]
    fn a_hooks_clicks_go_out_as_container_clicks_and_a_taken_press_suppresses_vanillas() {
        let mut resources = None;
        let mut game = server_game(&mut resources);
        game.screen = Screen::Inventory;
        // A press on a slot that a hook takes sends nothing of vanilla's.
        game.container_press(Some(9), false);
        with_registrants(&[&TAKE], || assert!(game.container_press(Some(9), false)));
        assert!(game.carried.is_none());
        assert!(game.inv_drag.is_none());
    }

    #[test]
    #[ignore = "daily"]
    fn a_key_is_not_dispatched_while_the_recipe_search_is_focused() {
        let mut resources = None;
        let mut game = server_game(&mut resources);
        game.screen = Screen::Inventory;
        BUILT.with(|n| n.set(0));
        if game.recipe_search_focused() {
            with_registrants(&[&TAKE], || assert!(!game.container_key("key.keyboard.r")));
            assert_eq!(BUILT.with(|n| n.get()), 0);
        }
    }

    #[test]
    #[ignore = "daily"]
    fn no_view_is_built_without_a_registrant_and_a_taken_press_ends_the_drag() {
        let mut resources = None;
        let mut game = server_game(&mut resources);
        game.screen = Screen::Inventory;
        BUILT.with(|n| n.set(0));
        with_registrants(&[], || assert!(!game.container_press(None, false)));
        assert_eq!(BUILT.with(|n| n.get()), 0);
        game.inv_drag = Some((0, vec![9]));
        with_registrants(&[&TAKE], || assert!(game.container_press(None, false)));
        assert_eq!(BUILT.with(|n| n.get()), 1);
        assert!(game.inv_drag.is_none());
    }
}
