//! Resume a detached authoritative menu at each positive mod-slot take.
use super::*;
use crate::game::api_values::TryIntoApi;
use crate::game::decided_action::Epoch;
use crate::game::transactions::decision::{ActionHost, GameHost, Identity, Reply, Side};
use sunrise_client::menu::{Asked, Env, Kind, Menu, MenuStep, TakeEvent};

impl Game<'_> {
    pub(super) fn menu_take_listened(menu: &Menu) -> bool {
        matches!(menu.kind(), Kind::Mod(_))
            && mod_api::__private::has_part_hook(mod_api::HookId::MenuSlotTake)
            && menu.registry_id().is_some_and(|id| {
                mod_api::__private::menu_listens(mod_api::HookId::MenuSlotTake, id)
            })
    }

    /// The pickup decision, transfer, each notice and continuation share the outer transaction.
    /// No environment borrowed from Game survives a step, and no click is replayed.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn guest_menu_click_taken(
        &mut self,
        gi: usize,
        menu: &mut Menu,
        click: crate::game::lan_pickup::Click,
        asked: Asked,
        prepare: &dyn Fn(&mut Menu, &Env),
        mut publish: impl FnMut(&mut Game<'_>, &mut Menu, &Vec<usize>),
        mut refresh: impl FnMut(&Game<'_>, &mut Menu) -> bool,
    ) -> Option<Vec<usize>> {
        let conn = self.lan.as_ref()?.hub.guests.get(gi)?.conn;
        let backup = menu.clone();
        let seal = self.transaction_open();
        self.journal_guest_click(conn);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.guest_menu_action_settled(
                gi,
                menu,
                Some(click),
                Some(prepare),
                |game, menu, rules, answer| {
                    let (mut step, before) =
                        game.guest_menu_op_with(gi, menu, rules, answer, |menu, env| {
                            prepare(menu, env);
                            menu.menu_slot_take_hook = true;
                            let before = (0..menu.slot_count())
                                .map(|i| menu.slot(i).cloned())
                                .collect::<Vec<_>>();
                            (
                                menu.click_steps_asked(env, click.0, click.1, click.2, asked),
                                before,
                            )
                        })?;
                    loop {
                        let changed = (0..menu.slot_count())
                            .filter(|&i| menu.slot(i) != before[i].as_ref())
                            .collect::<Vec<_>>();
                        publish(game, menu, &changed);
                        match step {
                            MenuStep::Done(Ok(())) => return Some(changed),
                            MenuStep::Done(Err(_)) => return None,
                            MenuStep::Take { event, resume } => {
                                if !game.menu_take_at(conn, menu, &event) || !refresh(game, menu) {
                                    return None;
                                }
                                step = game.guest_menu_op_with(
                                    gi,
                                    menu,
                                    rules,
                                    answer,
                                    |menu, env| resume(menu, env),
                                )?;
                            }
                        }
                    }
                },
            )
        }))
        .ok()
        .flatten();
        self.transaction_close(&seal, result.is_some());
        if result.is_none() {
            *menu = backup;
            log::warn!("menu_slot_take: refused a failed or stale click; nothing changed");
        }
        menu.menu_slot_take_hook = false;
        result
    }

    /// Snapshot a mod-owned source before a separately handled quick move. Player inventory,
    /// the built-in crafting grid and its result retain their existing take contracts.
    pub(super) fn menu_quick_take_before(
        &self,
        kind: Option<&'static sunrise_client::containers::ModMenuType>,
        slot: usize,
        pos: (i32, i32, i32),
    ) -> Option<(Menu, TakeEvent)> {
        let kind = kind?;
        if slot >= kind.slots
            || kind.crafting.is_some_and(|crafting| {
                crafting.result == slot || crafting.grid_slots().contains(&slot)
            })
        {
            return None;
        }
        let mut menu = Menu::new(Kind::Mod(kind));
        menu.set_registry_id(Some(kind.id));
        if !Self::menu_take_listened(&menu) {
            return None;
        }
        let Some(BlockEntity::Mod(entity)) = self.world.block_entities.get(&pos) else {
            return None;
        };
        let taken = entity.items.get(slot)?.clone()?;
        menu.set_block_pos(Some([pos.0, pos.1, pos.2]));
        Some((
            menu,
            TakeEvent {
                slot,
                taken,
                quick_craft_amount: 0,
            },
        ))
    }

    /// The handled move has committed its remainder and destination. Describe its positive
    /// removed portion through the same take notice, inside the move's outer transaction.
    pub(super) fn menu_quick_take_after(
        &mut self,
        conn: u32,
        pos: (i32, i32, i32),
        mut menu: Menu,
        mut event: TakeEvent,
    ) -> bool {
        let Some(BlockEntity::Mod(entity)) = self.world.block_entities.get(&pos) else {
            return false;
        };
        let left = entity
            .items
            .get(event.slot)
            .and_then(Option::as_ref)
            .filter(|left| left.same_item_same_components(&event.taken))
            .map_or(0, |left| left.count);
        let amount = event.taken.count.saturating_sub(left);
        if amount == 0 {
            return true;
        }
        event.taken.count = amount;
        event.quick_craft_amount = amount;
        let result = menu.mod_crafting().map(|crafting| crafting.result);
        for slot in 0..menu.slot_count() - 36 {
            if Some(slot) != result {
                menu.load_slot(slot, entity.items.get(slot).cloned().flatten());
            }
        }
        self.menu_take_at(conn, &menu, &event)
    }

    /// Fire with the live intermediate storage published. Refuse rather than resume with
    /// a cursor/inventory that differs from the continuation's frozen preconditions.
    pub(super) fn menu_take_at(&mut self, conn: u32, menu: &Menu, event: &TakeEvent) -> bool {
        let Some(actor) = self.guest_actor(conn) else {
            return false;
        };
        let Some(route) = self.guest_route(conn) else {
            return false;
        };
        let Some(guest) = self.lan.as_ref().and_then(|host| host.hub.guest(conn)) else {
            return false;
        };
        let Some(open) = guest.menu.as_ref() else {
            return false;
        };
        let Some(pos) = open.pos else {
            return false;
        };
        let Some(admitted) =
            crate::game::native_quick_move::Admitted::of(self, conn, pos, open.size, open.window)
        else {
            return false;
        };
        let generation = self.world.block_entities.generation(&pos);
        let inventory = guest.inventory.clone();
        let armor = guest.armor.clone();
        let offhand = guest.offhand.clone();
        let carried = guest.carried.clone();
        let position = guest.position;
        let links = open.links.clone();
        let uuid = guest.uuid;
        let Some(id) = menu.registry_id().map(str::to_owned) else {
            return false;
        };
        let Ok(stack) = event.taken.try_into_api() else {
            return false;
        };
        let env = self.menu_env();
        let (slot, view) = menu.take_slot_context(&env, event);
        let copy = |slots: &[Option<ItemStack>]| {
            slots
                .iter()
                .map(|stack| stack.as_ref().map(TryIntoApi::try_into_api).transpose())
                .collect::<std::result::Result<Vec<_>, _>>()
        };
        let (Ok(main), Ok(grid), Ok(result)) =
            (copy(view.slots), copy(view.grid), copy(view.result))
        else {
            return false;
        };
        let container = view.container;
        let context = mod_api::MenuSlotContext::new(Some(uuid), view.pos, vec![main, grid, result]);
        let fresh = move |game: &Game<'_>| {
            admitted.holds(game)
                && game.world.block_entities.generation(&pos) == generation
                && game
                    .lan
                    .as_ref()
                    .and_then(|host| host.hub.guest(conn))
                    .is_some_and(|guest| {
                        guest.uuid == uuid
                            && guest.inventory == inventory
                            && guest.armor == armor
                            && guest.offhand == offhand
                            && guest.carried == carried
                            && guest.position == position
                            && guest.menu.as_ref().is_some_and(|open| open.links == links)
                    })
        };
        let fire = |game: &mut Game<'_>| {
            let failures = game.wasm_dispatch_failures.get();
            let fired = game
                .as_guest(conn, |game| {
                    game.native_menu_slot_take(mod_api::__private::MenuSlotTakeCall {
                        menu: &id,
                        slot,
                        container,
                        stack: &stack,
                        quick_craft_amount: event.quick_craft_amount,
                        context: &context,
                    });
                })
                .is_some();
            if fired && game.wasm_dispatch_failures.get() == failures {
                Reply::Allow
            } else {
                Reply::Failed
            }
        };
        let identity = Identity {
            side: Side::Server,
            world: actor.epoch(),
            actor,
            target: actor,
        };
        let inherited = GameHost {
            game: &mut *self,
            identity,
        }
        .menu_busy(route);
        if inherited {
            // The trusted continuation is already inside the pickup's action. Do not admit a
            // nested action on the same menu; failure aborts its enclosing transaction instead.
            matches!(fire(self), Reply::Allow) && fresh(self)
        } else {
            self.decided_action(
                "menu_slot_take",
                route,
                actor,
                Epoch::Off,
                fresh,
                fire,
                |_| {},
            )
        }
    }
}
