//! Load-scoped registration and per-candidate condition lookup.
use super::Game;
use mod_api::__private::game::RegistrationContextNew;
use std::{collections::BTreeMap, sync::Arc};
pub(super) type ConditionTable =
    Arc<BTreeMap<mod_api::Identifier, (String, Arc<dyn mod_api::LoadCondition>)>>;
impl Game<'_> {
    pub(super) fn begin_condition_reload(
        &mut self,
        stack: &super::functions::PackStack,
        resources: &std::path::Path,
    ) -> Result<
        super::server_reload::PreparedServerReload,
        mod_api::__private::reload::ReloadStageError,
    > {
        use mod_api::__private::reload::ReloadStageError;
        if self.mods.load_condition_candidate.is_none() {
            let inventory = horizon_mod_package::enabled::EnabledMods::read(resources)
                .map_err(|error| ReloadStageError::DataSource(error.to_string()))?;
            self.mods.load_condition_candidate = Some(
                conditions_for_mods(&self.mods, &inventory)
                    .map_err(ReloadStageError::DataSource)?,
            );
        }
        self.prepare_server_reload(stack, resources)
    }
    pub(super) fn prepare_pack_condition_reload(
        &mut self,
        stack: &super::functions::PackStack,
    ) -> Result<
        super::server_reload::PreparedServerReload,
        mod_api::__private::reload::ReloadStageError,
    > {
        self.mods.load_condition_candidate = None;
        self.begin_condition_reload(stack, &self.options.resources.clone())
    }

    pub(super) fn stage_condition_inventory(
        &mut self,
    ) -> horizon_client::Result<Option<horizon_assets::resource_source::ResourceRoot>> {
        self.protect_condition_root();
        self.mods.load_condition_candidate = None;
        let inventory = horizon_mod_package::enabled::EnabledMods::read(&self.options.resources)?;
        self.mods.load_condition_candidate = Some(conditions_for_mods(&self.mods, &inventory)?);
        self.stage_enabled_inventory()
    }

    pub(super) fn protect_condition_root(&mut self) {
        self.mods.active_condition_root = Some(horizon_client::mod_content::retain_root(
            &self.options.resources,
        ));
    }

    pub(super) fn condition_root_needs_reload(&self) -> bool {
        self.mods
            .load_condition_candidate
            .as_ref()
            .is_some_and(|view| view.has_registered())
            && (!self.mods.selected.is_empty()
                || !self.mods.dev.is_empty()
                || !self.mods.native.packages().is_empty())
    }

    /// The conditions of a load from `root` over the candidate pack `files`.
    pub(super) fn registered_load_conditions(
        &self,
        inventory: &horizon_mod_package::enabled::EnabledMods,
        root: &std::path::Path,
        files: &[(String, Vec<Vec<u8>>)],
    ) -> Result<horizon_world::conditions::LoadConditions, String> {
        // `ICondition.IContext`: the world's feature flags and every registry's
        // entries as this candidate holds them. Tags join later, once this load has bound them.
        let registries = candidate_registries(root, files);
        Ok(conditions_for_mods(&self.mods, inventory)?
            .with_features(self.level.enabled_features.iter().map(String::as_str))
            .with_registries(
                registries
                    .iter()
                    .map(|(registry, ids)| (registry.as_str(), ids.iter().map(String::as_str))),
            ))
    }
}

/// Every registry's entries as the candidate load holds them, which `neoforge:registered` reads:
/// the built-in ones the prepared root exports (`command-registries.json`, never what an earlier
/// load added), and the entries the candidate's pack `files` add to a registry that takes them
/// (vanilla's, or a datapack registry a mod declared). So an id a pack adds counts on its first
/// load, and one a pack no longer has stops counting on the next.
fn candidate_registries(
    root: &std::path::Path,
    files: &[(String, Vec<Vec<u8>>)],
) -> BTreeMap<String, std::collections::BTreeSet<String>> {
    let export: serde_json::Value = std::fs::read(root.join("command-registries.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let mut registries: BTreeMap<String, std::collections::BTreeSet<String>> =
        export["by_registry"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(registry, entry)| {
                let ids = entry["ids"].as_array().into_iter().flatten();
                let ids = ids.filter_map(serde_json::Value::as_str).map(str::to_owned);
                (registry.clone(), ids.collect())
            })
            .collect();
    for declaration in mod_api::__private::datapack_registry_declarations().iter() {
        registries.entry(declaration.key.clone()).or_default();
    }
    // A pack file's key is `<folder>/<namespace>:<path>`: `worldgen/biome/test:dunes` for
    // `minecraft:worldgen/biome`, `mymod/spell/test:fire` for a mod's `mymod:spell`.
    let folder = |registry: &str| match registry.split_once(':') {
        Some(("minecraft", path)) => path.to_owned(),
        Some((namespace, path)) => format!("{namespace}/{path}"),
        None => registry.to_owned(),
    };
    let by_folder: BTreeMap<String, String> = registries
        .keys()
        .map(|registry| (folder(registry), registry.clone()))
        .collect();
    for (key, _) in files {
        let Some(slash) = key.find(':').and_then(|colon| key[..colon].rfind('/')) else {
            continue;
        };
        if let Some(registry) = by_folder.get(&key[..slash]) {
            let id = key[slash + 1..].to_owned();
            registries
                .get_mut(registry)
                .expect("named above")
                .insert(id);
        }
    }
    registries
}

pub(super) fn conditions_for_mods(
    sources: &super::mods::ModsState,
    inventory: &horizon_mod_package::enabled::EnabledMods,
) -> Result<horizon_world::conditions::LoadConditions, String> {
    if let Some(candidate) = &sources.load_condition_candidate {
        return Ok(candidate.with_mod_inventory(inventory.server_ids()));
    }
    let table = sources
        .load_conditions
        .get_or_init(|| {
            let mut registrar = mod_api::LoadConditionDeclarations::default();
            Game::native_register_load_conditions(
                &mod_api::RegistrationContext::new(),
                &mut registrar,
            );
            registrar
                .finish()
                .map(Arc::new)
                .map_err(|error| error.message)
        })
        .clone()?;
    let conditions = horizon_world::conditions::LoadConditions::new(inventory.server_ids());
    if table.is_empty() {
        return Ok(conditions);
    }
    let mut enabled: std::collections::BTreeSet<_> =
        table.values().map(|(owner, _)| owner.clone()).collect();
    enabled.retain(|owner| sources.code_owner_enabled(owner));
    let owned = table
        .iter()
        .map(|(name, (owner, _))| (name.namespace().to_owned(), owner.clone()))
        .collect();
    Ok(conditions.with_lookup(owned, move |name, args| {
        let id = mod_api::Identifier::parse(name).ok()?;
        let (owner, handler) = table.get(&id)?;
        if !enabled.contains(owner) {
            return Some(Ok(false));
        }
        let mut arguments = args.as_object().cloned().unwrap_or_default();
        arguments.remove("type");
        Some(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                handler.test(&mod_api::serde_json::Value::Object(arguments))
            }))
            .map_err(|_| format!("condition {name} (mod {owner}): handler panicked"))
            .and_then(|answer| {
                answer.map_err(|error| format!("condition {name} (mod {owner}): {}", error.message))
            }),
        )
    }))
}

#[cfg(test)]
#[path = "load_conditions/tests.rs"]
mod tests;

pub(super) fn content_root_with_conditions(
    root: &std::path::Path,
    jars: &std::path::Path,
    mods: &[horizon_client::mod_content::ContentMod],
    cache: &std::path::Path,
    inventory: &horizon_mod_package::enabled::EnabledMods,
    sources: &super::mods::ModsState,
) -> horizon_client::Result<std::path::PathBuf> {
    let conditions = conditions_for_mods(sources, inventory)?;
    horizon_client::mod_content::resource_root_with_conditions(
        root,
        jars,
        mods,
        cache,
        inventory,
        &conditions,
    )
}
