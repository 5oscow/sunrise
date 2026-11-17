//! Load-phase data-file condition registration.
pub trait Hooks {
    /// Register named conditions once at mod load, before server data admission.
    /// Each handler is a raw-JSON handler: it reads the condition's JSON value.
    /// Names may use any namespace; duplicate ids refuse mod load and name both
    /// registering mods. Native and wasm handlers share the same route. Any
    /// namespace with a registered handler is owned for unknown-id handling,
    /// whoever registered it; an unknown condition there refuses data reload.
    /// Foreign unknown ids omit their file with one ERROR-level log entry,
    /// even inside `or(true, unknown)`; every combinator branch is visited.
    #[hook(
        side = "server",
        version = 2,
        contract = "collect",
        out = "conditions",
        context = "registration",
        neoforge = "RegisterEvent"
    )]
    fn register_load_conditions();
}
