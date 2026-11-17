//! Resolve predicate/item-modifier tags once, without entering codec-owned payloads.
use super::*;
impl super::super::LootData {
    /// Edit and validate private table replacements before the one publication.
    /// A refused replacement leaves this snapshot and its retained codecs intact.
    pub fn apply_native_table_loads(
        &mut self,
        context: &crate::native_load_context::LoadContext,
    ) -> io::Result<()> {
        let mut ids: Vec<_> = self.tables.ids().collect();
        ids.sort();
        let mut tables = HashMap::new();
        for id in ids {
            let mut table = self.tables[id].clone();
            super::super::native_loot_table_load(context, id, &mut table);
            self.holder_sets
                .walk(&format!("native loot_table {id}"), &mut table, &self.tags)?;
            table_fields(&mut table, &self.tags, &self.types);
            tables.insert(id.to_owned(), table);
        }
        self.types.validate(
            tables.iter().map(|(id, value)| (id.as_str(), value)),
            &self.predicates,
            &self.modifiers,
        )?;
        self.tables = Arc::new(tables.into_iter().collect());
        self.refresh_copy_requirements();
        Ok(())
    }
    pub(in crate::loot) fn prepare_types(&mut self) -> io::Result<()> {
        let mut active = HashSet::new();
        let mut done = HashSet::new();
        for (registry, id) in self
            .tags
            .keys()
            .filter(|(registry, _)| matches!(registry.as_str(), "predicate" | "item_modifier"))
        {
            check_tag(&self.tags, registry, id, &mut active, &mut done)?;
        }
        let tags = &self.tags;
        let types = &self.types;
        let mut tables = HashMap::new();
        for id in self.tables.ids() {
            let mut table = self.tables[id].clone();
            table_fields(&mut table, tags, types);
            tables.insert(id.to_owned(), table);
        }
        self.tables = Arc::new(tables.into_iter().collect());
        for value in Arc::make_mut(&mut self.predicates).values_mut() {
            typed(value, false, tags, types);
        }
        for value in self.modifiers.values_mut() {
            typed(value, true, tags, types);
        }
        self.validate_types()?;
        self.refresh_copy_requirements();
        Ok(())
    }
}
type Tags = HashMap<(String, String), crate::tag_layers::TagLayers>;
fn check_tag(
    tags: &Tags,
    registry: &str,
    id: &str,
    active: &mut HashSet<(String, String)>,
    done: &mut HashSet<(String, String)>,
) -> io::Result<()> {
    let key = (registry.to_owned(), id.to_owned());
    if done.contains(&key) {
        return Ok(());
    }
    if !active.insert(key.clone()) {
        return Err(invalid(
            &format!("tags/{registry} {id}"),
            id,
            "reference cycle",
        ));
    }
    if let Some(layers) = tags.get(&key) {
        for tag in layers.tags() {
            check_tag(tags, registry, tag, active, done)?;
        }
    }
    active.remove(&key);
    done.insert(key);
    Ok(())
}
fn table_fields(value: &mut Value, tags: &Tags, types: &Types) {
    fields(value, tags, types);
    for pool in value
        .get_mut("pools")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        fields(pool, tags, types);
        for entry in pool
            .get_mut("entries")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            container(entry, tags, types);
        }
    }
}
fn container(value: &mut Value, tags: &Tags, types: &Types) {
    fields(value, tags, types);
    for child in value
        .get_mut("children")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        container(child, tags, types);
    }
    if value["type"] == "minecraft:loot_table" && value["value"].is_object() {
        table_fields(&mut value["value"], tags, types);
    }
}
fn fields(value: &mut Value, tags: &Tags, types: &Types) {
    if let Some(condition) = value.get_mut("condition") {
        typed(condition, false, tags, types);
    }
    if let Some(modifier) = value.get_mut("modifier") {
        typed(modifier, true, tags, types);
    }
}
fn typed(value: &mut Value, function: bool, tags: &Tags, types: &Types) {
    if let Some(id) = value.as_str() {
        if !id.contains(':') {
            *value = Value::String(format!("minecraft:{id}"));
        }
        return;
    }
    if let Some(id) = value.get("type").and_then(Value::as_str)
        && !id.contains(':')
    {
        value["type"] = Value::String(format!("minecraft:{id}"));
    }
    if function && let Some(values) = value.as_array_mut() {
        for value in values {
            typed(value, true, tags, types);
        }
        return;
    }
    let kind = value["type"].as_str().unwrap_or("");
    let conditional = types.conditional(kind);
    match (function, kind) {
        (true, "minecraft:sequence" | "sequence") => list(value, "functions", true, tags, types),
        (false, "minecraft:all_of" | "all_of" | "minecraft:any_of" | "any_of") => {
            list(value, "terms", false, tags, types)
        }
        (false, "minecraft:inverted" | "inverted") => typed(&mut value["term"], false, tags, types),
        (true, "minecraft:filtered" | "filtered") => {
            for field in ["on_pass", "on_fail"] {
                if let Some(value) = value.get_mut(field) {
                    typed(value, true, tags, types);
                }
            }
        }
        _ => {}
    }
    if function
        && conditional
        && let Some(condition) = value.get_mut("condition")
    {
        typed(condition, false, tags, types);
    }
}
fn list(value: &mut Value, field: &str, function: bool, tags: &Tags, types: &Types) {
    if let Some(tag) = value[field].as_str().and_then(|id| id.strip_prefix('#')) {
        let registry = if function {
            "item_modifier"
        } else {
            "predicate"
        };
        let ids = super::super::loot_tags::members(tags, registry, tag);
        value[field] = Value::Array(ids.into_iter().map(Value::String).collect());
    }
    for term in value
        .get_mut(field)
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        typed(term, function, tags, types);
    }
}
