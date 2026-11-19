//! A native mod whose hook calls a Host method its load phase refuses, twice (#3123). The probe builds
//! it as the shipped route builds a mod, and fires the hook under the Setup phase.
use mod_api::{Flow, Hooks, PotionBrewPreEvent, ServerHost};
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn potion_brew_pre(
        &self,
        host: &mut dyn ServerHost,
        event: &mut PotionBrewPreEvent<'_>,
    ) -> Flow {
        // Undone by the game when this listener is refused.
        event.cancelled = true;
        let first = catch_unwind(AssertUnwindSafe(|| {
            host.entity(1);
        }))
        .is_err();
        // The mod's own catch does not undo the refusal: every later Host call refuses too.
        let second = catch_unwind(AssertUnwindSafe(|| {
            host.entity(2);
        }))
        .is_err();
        mod_api::content::crash::set_detail(
            "phase probe",
            format!("first {first} latched {second}"),
        );
        Flow::Cancel
    }
}
