//! A native mod whose hook calls a Host method its load phase refuses, then one it permits (#3123). The probe builds
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
        // Positive control: the Setup phase permits this metadata call, so it answers before any
        // refusal and the later refusal of it can only be the latch.
        let allowed = catch_unwind(AssertUnwindSafe(|| {
            host.tags();
        }))
        .is_ok();
        let first = catch_unwind(AssertUnwindSafe(|| {
            host.entity(1);
        }))
        .is_err();
        // The mod's own catch does not undo the refusal: the permitted call now refuses too.
        let latched = catch_unwind(AssertUnwindSafe(|| {
            host.tags();
        }))
        .is_err();
        mod_api::content::crash::set_detail(
            "phase probe",
            format!("allowed {allowed} first {first} latched {latched}"),
        );
        Flow::Cancel
    }
}
