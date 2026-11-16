//! Resolve predicate/item-modifier tags once, without entering codec-owned payloads.
use super::*;
impl super::super::LootData {
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
        let mut tables = HashMap::new();
        for id in self.tables.ids() {
            let mut table = self.tables[id].clone();
            table_fields(&mut table, tags);
            tables.insert(id.to_owned(), table);
        }
        self.tables = Arc::new(tables.into_iter().collect());
        for value in Arc::make_mut(&mut self.predicates).values_mut() {
            typed(value, false, tags);
        }
        for value in self.modifiers.values_mut() {
            typed(value, true, tags);
        }
        self.validate_types()
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
fn table_fields(value: &mut Value, tags: &Tags) {
    fields(value, tags);
    for pool in value
        .get_mut("pools")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        fields(pool, tags);
        for entry in pool
            .get_mut("entries")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            container(entry, tags);
        }
    }
}
fn container(value: &mut Value, tags: &Tags) {
    fields(value, tags);
    for child in value
        .get_mut("children")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        container(child, tags);
    }
    if value["type"] == "minecraft:loot_table" && value["value"].is_object() {
        table_fields(&mut value["value"], tags);
    }
}
fn fields(value: &mut Value, tags: &Tags) {
    if let Some(condition) = value.get_mut("condition") {
        typed(condition, false, tags);
    }
    if let Some(modifier) = value.get_mut("modifier") {
        typed(modifier, true, tags);
    }
}
fn typed(value: &mut Value, function: bool, tags: &Tags) {
    if function && let Some(values) = value.as_array_mut() {
        for value in values {
            typed(value, true, tags);
        }
        return;
    }
    let kind = value["type"].as_str().unwrap_or("");
    match (function, kind) {
        (true, "minecraft:sequence" | "sequence") => list(value, "functions", true, tags),
        (false, "minecraft:all_of" | "all_of" | "minecraft:any_of" | "any_of") => {
            list(value, "terms", false, tags)
        }
        (false, "minecraft:inverted" | "inverted") => typed(&mut value["term"], false, tags),
        (true, "minecraft:filtered" | "filtered") => {
            for field in ["on_pass", "on_fail"] {
                if let Some(value) = value.get_mut(field) {
                    typed(value, true, tags);
                }
            }
        }
        _ => {}
    }
    if function && let Some(condition) = value.get_mut("condition") {
        typed(condition, false, tags);
    }
}
fn list(value: &mut Value, field: &str, function: bool, tags: &Tags) {
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
        typed(term, function, tags);
    }
}
