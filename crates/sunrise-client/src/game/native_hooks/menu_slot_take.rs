use super::*;

impl Game<'_> {
    /// The authoritative, normalized take notice for a registered mod menu.
    #[inline(never)]
    pub(in crate::game) fn native_menu_slot_take(
        &mut self,
        call: mod_api::__private::MenuSlotTakeCall<'_>,
    ) {
        self.fire_menu_slot_take(call);
    }
}
