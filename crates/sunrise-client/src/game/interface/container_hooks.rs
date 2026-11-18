//! The open menu screen as `sunrise_hooks::containers` offers it (docs/layers.md, "Container
//! screens"): the engine's slot clicks and queries, the `Container` view over them, and the
//! dispatch of input and drawing to the crates that registered hooks, at the screen events' own
//! moments. With none registered nothing here builds a view.
use super::{
    key_codes::glfw_button,
    mount_menu,
    stations::{self, Kind as StationKind},
};
use crate::game::{Game, Screen};
use std::cell::RefCell;
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
    containers::{self as api, Button, Click, Container, Panel, Role, Slot, SlotId, Stack},
    hud,
};
use winit::{event::MouseButton, keyboard::KeyCode};

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
    panel: Panel,
    /// Every stack this view handed out, by its `Stack::id`.
    seen: RefCell<Vec<ItemStack>>,
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
    Station(StationKind),
    Mount,
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
        Local::Station(StationKind::Merchant) => "minecraft:merchant".into(),
        Local::Station(kind) => super::lan_stations::menu_of(kind)?.0.into(),
        Local::Mount => super::native_menus::MOUNT_MENU.into(),
    };
    Some(id)
}

/// What tells two stacks apart that don't merge: the item, its damage and its components.
fn kind_of(stack: &ItemStack) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (&*stack.item, stack.damage, stack.data()).hash(&mut hasher);
    hasher.finish()
}

/// A mouse button by the screen events' numbering.
fn button_of(button: MouseButton) -> Button {
    match glfw_button(button) {
        0 => Button::Left,
        1 => Button::Right,
        2 => Button::Middle,
        n => Button::Other(u16::try_from(n).unwrap_or(u16::MAX)),
    }
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
        let mut seen = self.seen.borrow_mut();
        seen.push(stack.clone());
        Stack::new(
            stack.item.to_string(),
            stack.count,
            self.game().max_stack(stack),
            kind_of(stack),
            seen.len() as u64 - 1,
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
                Slot::new(SlotId(i as u32), game.container_role(i), x, y)
            })
            .collect()
    }
    fn stack(&self, slot: SlotId) -> Option<Stack> {
        let stack = self.game().slot_view(slot.0 as usize)?;
        Some(self.stack_of(&stack))
    }
    fn carried(&self) -> Option<Stack> {
        self.game().carried.as_ref().map(|s| self.stack_of(s))
    }
    fn name(&self, slot: SlotId) -> Option<String> {
        let game = self.game();
        let stack = game.slot_view(slot.0 as usize)?;
        Some(hover_name(&game.pack.names, &stack))
    }
    fn takes_one(&mut self, slot: SlotId, of: &Stack) -> bool {
        let Some(stack) = usize::try_from(of.id)
            .ok()
            .and_then(|i| self.seen.borrow().get(i).cloned())
        else {
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
    fn panel(&self) -> Panel {
        self.panel
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
    /// The menu type id of the open menu screen: the player's own, any container, workstation,
    /// trade, mount or mod menu; not creative's tabs, which have no menu type.
    fn container_menu_id(&self) -> Option<String> {
        if self.screen != Screen::Inventory {
            return None;
        }
        if self.mount_active() {
            return local_menu_id(Local::Mount);
        }
        if self.remote.is_some() {
            return Some(self.remote_menu_type().unwrap_or("inventory").to_owned());
        }
        if let Some(station) = &self.station {
            return local_menu_id(Local::Station(station.kind));
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
    /// What a game slot is for in the open menu.
    pub(in crate::game) fn container_role(&self, slot: usize) -> Role {
        match slot {
            0..9 => Role::Hotbar,
            9..36 => Role::Inventory,
            100..109 => Role::Input,
            200 | stations::RESULT => Role::Result,
            500..=504 | mount_menu::SADDLE | mount_menu::ARMOR => Role::Equipment,
            s if (stations::INPUT..stations::RESULT).contains(&s) => Role::Input,
            s if (mount_menu::CHEST..mount_menu::CHEST + 15).contains(&s) => Role::Storage,
            300..500 => match self.container_kind() {
                Some(BlockEntity::Furnace(_)) if slot == 302 => Role::Result,
                Some(BlockEntity::Crafter(_)) if slot == 309 => Role::Result,
                Some(
                    BlockEntity::Furnace(_) | BlockEntity::Brewing(_) | BlockEntity::Crafter(_),
                ) => Role::Input,
                _ => Role::Storage,
            },
            _ => Role::Other,
        }
    }
    /// The panel's corner, which slot positions are from, and its size, as `batch`'s frame lays
    /// the screen out.
    fn container_frame(&self, batch: &HudBatch) -> ((f32, f32), Panel) {
        let (origin, width, height) = match self.built_container(batch) {
            Some(layout) => (layout.origin, layout.image[2], layout.image[3]),
            None => (
                self.inventory_origin_unbuilt(batch),
                self.native_screen_width().unwrap_or(176.0),
                self.screen_height(),
            ),
        };
        // titleLabelY and inventoryLabelY (imageHeight - 94).
        (origin, Panel::new(width, height, 6.0, height - 94.0))
    }
    /// The view for input, when a hook is registered and the screen is a menu.
    fn open_container<'a>(&'a mut self) -> Option<Open<'a, 'g>> {
        if !api::engine::any() {
            return None;
        }
        let menu = self.container_menu_id()?;
        #[cfg(test)]
        BUILT.with(|n| n.set(n.get() + 1));
        let (origin, panel) = match self.renderer.as_ref() {
            Some(r) => self.container_frame(&HudBatch::new(
                [r.config.width, r.config.height],
                self.settings.gui_scale_inputs(),
            )),
            None => (
                (0.0, 0.0),
                Panel::new(176.0, self.screen_height(), 6.0, 72.0),
            ),
        };
        Some(Open {
            game: Backing::Owned(self),
            menu,
            origin,
            panel,
            seen: RefCell::default(),
        })
    }
    fn container_target(&self) -> Option<SlotId> {
        self.inventory_target().0.map(|s| SlotId(s as u32))
    }
    /// A mouse button down or up on a menu screen, once the mods' pre hooks let it through: true
    /// when a hook took it. A press a hook took makes its release the hooks' alone.
    pub(in crate::game) fn container_mouse(&mut self, button: MouseButton, pressed: bool) -> bool {
        let pressed_by_hooks = !pressed && self.container_buttons.remove(&button);
        if !api::engine::any() {
            return pressed_by_hooks;
        }
        let slot = self.container_target();
        let Some(mut open) = self.open_container() else {
            return pressed_by_hooks;
        };
        if !pressed {
            return api::engine::release(&mut open, slot, button_of(button)) || pressed_by_hooks;
        }
        let taken = api::engine::press(&mut open, slot, button_of(button));
        if taken {
            self.container_buttons.insert(button);
        }
        taken
    }
    /// The release of a button whose press a hook took, with no menu screen open any more: it
    /// stays the hooks'.
    pub(in crate::game) fn container_release_elsewhere(&mut self, button: MouseButton) -> bool {
        self.container_buttons.remove(&button)
    }
    /// The cursor moved with a button held, once the mods' pre hook let it through: true when a
    /// hook took it. A drag taken from vanilla's own press ends its quick craft drag.
    pub(in crate::game) fn container_drag(&mut self) -> bool {
        if !api::engine::any() {
            return false;
        }
        let Some(button) = self
            .drag_button
            .filter(|b| self.physical_buttons.contains(b))
        else {
            return false;
        };
        let slot = self.container_target();
        let Some(mut open) = self.open_container() else {
            return false;
        };
        let taken = api::engine::drag(&mut open, slot, button_of(button));
        if taken && !self.container_buttons.contains(&button) {
            self.inv_drag = None;
            self.tweaks.end_drag();
            self.inv_clicks.skip_release();
        }
        taken
    }
    /// The wheel over a menu screen, once the mods' pre hook let it through: true when a hook
    /// took it.
    pub(in crate::game) fn container_wheel(&mut self, steps: f32) -> bool {
        if !api::engine::any() {
            return false;
        }
        let slot = self.container_target();
        let Some(mut open) = self.open_container() else {
            return false;
        };
        api::engine::wheel(&mut open, slot, steps)
    }
    /// A key press on a menu screen, once the mods' pre hook let it through, unless a text field
    /// has the focus: true when a hook took it.
    pub(in crate::game) fn container_key(&mut self, key: KeyCode) -> bool {
        if !api::engine::any() || self.pad_typing() {
            return false;
        }
        let Some(name) = sunrise_client::keybinds::key_name(key) else {
            return false;
        };
        let slot = self.container_target();
        let Some(mut open) = self.open_container() else {
            return false;
        };
        api::engine::key(&mut open, slot, name)
    }
    /// What the hooks draw over the open menu screen, at its foreground phase.
    pub(in crate::game) fn draw_container_hooks(&self, batch: &mut HudBatch) {
        if !api::engine::any() {
            return;
        }
        let Some(menu) = self.container_menu_id() else {
            return;
        };
        #[cfg(test)]
        BUILT.with(|n| n.set(n.get() + 1));
        let (origin, panel) = self.container_frame(batch);
        let open = Open {
            game: Backing::Shared(self),
            menu,
            origin,
            panel,
            seen: RefCell::default(),
        };
        let mut canvas = Canvas {
            batch,
            atlas: &self.gui.atlas,
            origin,
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

    /// Takes every press, drag and key.
    struct Take;
    impl ContainerHooks for Take {
        fn press(&self, _: &mut dyn Container, _: Option<SlotId>, _: Button) -> bool {
            true
        }
        fn drag(&self, _: &mut dyn Container, _: Option<SlotId>, _: Button) -> bool {
            true
        }
        fn key(&self, _: &mut dyn Container, _: Option<SlotId>, _: &str) -> bool {
            true
        }
    }
    static TAKE: Take = Take;

    /// Picks up slot 9 on every press.
    struct PickNine;
    impl ContainerHooks for PickNine {
        fn press(&self, c: &mut dyn Container, _: Option<SlotId>, _: Button) -> bool {
            c.click(SlotId(9), Click::Pick { right: false })
        }
    }
    static PICK_NINE: PickNine = PickNine;

    /// Takes nothing.
    struct Pass;
    impl ContainerHooks for Pass {}
    static PASS: Pass = Pass;

    #[test]
    fn a_local_container_has_vanillas_menu_id() {
        let chest = |rows| Local::Chest {
            rows: Some(rows),
            shulker: false,
            size: rows * 9,
        };
        assert_eq!(
            local_menu_id(chest(3)).as_deref(),
            Some("minecraft:generic_9x3")
        );
        assert_eq!(
            local_menu_id(chest(6)).as_deref(),
            Some("minecraft:generic_9x6")
        );
        let shulker = Local::Chest {
            rows: None,
            shulker: true,
            size: 27,
        };
        assert_eq!(
            local_menu_id(shulker).as_deref(),
            Some("minecraft:shulker_box")
        );
        assert_eq!(
            local_menu_id(Local::Furnace(FurnaceKind::Smoker)).as_deref(),
            Some("minecraft:smoker")
        );
        assert_eq!(
            local_menu_id(Local::Station(StationKind::Anvil)).as_deref(),
            Some("minecraft:anvil")
        );
        assert_eq!(
            local_menu_id(Local::Station(StationKind::Merchant)).as_deref(),
            Some("minecraft:merchant")
        );
        assert_eq!(
            local_menu_id(Local::Mount).as_deref(),
            Some("sunrise:mount_inventory")
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
        assert_ne!(
            kind_of(&a),
            kind_of(&ItemStack::new("minecraft:enchanted_book", 1))
        );
    }

    #[test]
    #[ignore = "daily"]
    fn a_hooks_click_goes_through_the_menu() {
        let mut resources = None;
        let mut game = server_game(&mut resources);
        game.screen = Screen::Inventory;
        let stone = ItemStack::new("minecraft:stone", 5);
        game.inventory[9] = Some(stone.clone());
        with_registrants(&[&PICK_NINE], || {
            assert!(game.container_mouse(MouseButton::Left, true))
        });
        assert_eq!(game.carried, Some(stone));
        assert!(game.inventory[9].is_none());
    }

    #[test]
    #[ignore = "daily"]
    fn a_taken_press_keeps_its_release_from_vanilla() {
        let mut resources = None;
        let mut game = server_game(&mut resources);
        game.screen = Screen::Inventory;
        with_registrants(&[&TAKE], || {
            assert!(game.container_mouse(MouseButton::Left, true));
            // Take takes no release: it is kept because the press was taken.
            assert!(game.container_mouse(MouseButton::Left, false));
            assert!(!game.container_mouse(MouseButton::Right, false));
        });
        with_registrants(&[&PASS], || {
            assert!(!game.container_mouse(MouseButton::Left, true));
            assert!(!game.container_mouse(MouseButton::Left, false));
        });
        with_registrants(&[&TAKE], || {
            assert!(game.container_mouse(MouseButton::Left, true));
            game.screen = Screen::Playing;
            assert!(game.container_release_elsewhere(MouseButton::Left));
            assert!(!game.container_release_elsewhere(MouseButton::Left));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_key_is_not_dispatched_while_the_recipe_search_is_focused() {
        let mut resources = None;
        let mut game = server_game(&mut resources);
        game.screen = Screen::Inventory;
        with_registrants(&[&TAKE], || assert!(game.container_key(KeyCode::KeyR)));
        game.level.recipe_book.open[sunrise_client::recipe_book::BookType::Crafting.index()] = true;
        game.ensure_recipe_search();
        game.recipe_focus_search();
        assert!(game.recipe_search_focused());
        BUILT.with(|n| n.set(0));
        with_registrants(&[&TAKE], || assert!(!game.container_key(KeyCode::KeyR)));
        assert_eq!(BUILT.with(|n| n.get()), 0);
    }

    #[test]
    #[ignore = "daily"]
    fn no_view_is_built_without_a_registrant_or_a_held_button() {
        let mut resources = None;
        let mut game = server_game(&mut resources);
        game.screen = Screen::Inventory;
        BUILT.with(|n| n.set(0));
        with_registrants(&[], || {
            assert!(!game.container_mouse(MouseButton::Left, true))
        });
        assert_eq!(BUILT.with(|n| n.get()), 0);
        // No button is held: a pointer move is no drag, however a press went.
        with_registrants(&[&TAKE], || assert!(!game.container_drag()));
        assert_eq!(BUILT.with(|n| n.get()), 0);
        with_registrants(&[&TAKE], || {
            assert!(game.container_mouse(MouseButton::Left, true))
        });
        assert_eq!(BUILT.with(|n| n.get()), 1);
    }
}
