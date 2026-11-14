//! Load-phase declarative profession registration.
pub trait Hooks {
    /// Declare villager professions once per mod load, before world admission.
    /// The engine resolves every reference atomically with the owning resource tables.
    /// Vanilla replacement and overlapping custom acquisition POIs are refused.
    /// The public NeoForge 26.3 Javadoc was unreachable; no event parity is claimed.
    #[hook(
        signature = "pub(in crate::game) fn native_register_villager_professions( context: &mod_api::RegistrationContext, professions: &mut dyn VillagerProfessions, )",
        side = "server",
        contract = "collect",
        out = "professions",
        context = "registration"
    )]
    fn register_villager_professions();
}
