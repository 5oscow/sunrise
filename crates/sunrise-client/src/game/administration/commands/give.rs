//! `/give`'s handing over of one stack.
use super::{Game, Target};
use sunrise_client::items::ItemStack;
use sunrise_core::random::RandomSource;

impl Game<'_> {
    /// `GiveCommand.giveItem`'s handing over of one stack to the acting player:
    /// `Inventory.add`, then what did not fit thrown from the eyes with no pickup delay and
    /// `owner` (the player's UUID) its only taker, or the pickup sound when it all went in.
    /// For a player in another level than the one in place nothing lands in the world: what
    /// did not fit is returned and no sound plays.
    pub(in crate::game) fn give_acting_stack(
        &mut self,
        stack: ItemStack,
        owner: Option<[i32; 4]>,
    ) -> Option<ItemStack> {
        let uuid = self.native_acting_id();
        self.journal_player(uuid);
        let left = self.add_stack_to_inventory(stack);
        if !self.acting_in_level_in_place() {
            return left;
        }
        // At the player who gets them.
        if let Some(rest) = left {
            let uuid = self.native_acting_id();
            self.after_commit(crate::game::transactions::After::GiveRest(
                uuid, rest, owner,
            ));
        } else {
            let uuid = self.native_acting_id();
            self.after_commit(crate::game::transactions::After::GivePickup(uuid));
        }
        None
    }
    pub(in crate::game) fn give_pickup(&mut self) {
        let p = self
            .target_position(Target::Player)
            .unwrap_or(self.source_position());
        // GiveCommand uses the receiving player's random, not the level's.
        let mut r = sunrise_client::random_source::entity();
        let pitch = ((r.next_float() - r.next_float()) * 0.7 + 1.0) * 2.0;
        self.audio
            .play("minecraft:entity.item.pickup", Some(p), 0.2, pitch);
    }
    pub(in crate::game) fn give_remainder(&mut self, rest: ItemStack, owner: Option<[i32; 4]>) {
        let p = self
            .target_position(Target::Player)
            .unwrap_or(self.source_position());
        // Inventory full: Player.createItemStackToDrop (thrown from
        // the eyes), then setNoPickUpDelay and setTarget(player).
        let eye = self.eye_position().unwrap_or([
            p[0],
            p[1] + self.target_eye_height(Target::Player),
            p[2],
        ]);
        self.throw_from_eyes(rest, eye, false, false);
        if let Some(item) = self.just_spawned_item() {
            item.thrower = None;
            item.pickup_delay = 0;
            item.target = owner;
        }
    }
}
