use super::*;
/// A kind's overlay files over what the resource root provided: the top
/// pack's file for each id replaces the rest (`SimpleJsonResourceReloadListener`
/// sees one resource per id).
pub(super) fn overlay_map(overlay: &PackOverlay, kind: &str, out: &mut HashMap<String, Value>) {
    for (id, files) in overlay.kind(kind) {
        let Ok(v) = serde_json::from_slice::<Value>(PackOverlay::top(files)) else {
            continue;
        };
        out.insert(id.to_owned(), v);
    }
    // Only a loot table's pool entries carry their own conditions (#1381).
    out.retain(|id, value| match kind {
        "loot_table" => overlay.conditions.admit_loot_table(value, id),
        _ => overlay.conditions.allows_id(value, kind, id),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conditions::LoadConditions;
    use serde_json::json;

    fn file(kind: &str, value: Value) -> (String, Vec<Vec<u8>>) {
        (
            format!("{kind}/demo:file"),
            vec![value.to_string().into_bytes()],
        )
    }

    /// Conditions that report an unknown condition kind as a load error of the
    /// folder it is evaluated in, so a nested key that got evaluated shows.
    fn strict() -> LoadConditions {
        LoadConditions::new(["create"])
            .with_diagnostics()
            .with_lookup(std::collections::BTreeMap::new(), |_, _| None)
    }

    fn read(kind: &str, value: Value) -> Value {
        let files = [file(kind, value)];
        let conditions = strict();
        let overlay = PackOverlay {
            files: &files,
            conditions: &conditions,
        };
        let mut out = HashMap::new();
        overlay_map(&overlay, kind, &mut out);
        // A nested key outside pool entries is never evaluated: its unknown
        // condition leaves no diagnostic.
        assert_eq!(conditions.check_resource_errors(kind), Ok(()), "{kind}");
        out.remove("demo:file").expect("the file loads")
    }

    /// #1381: a loot table's pool entries carry their own conditions: a failing
    /// entry is dropped and a passing one loses the key; other lists of the
    /// table (an entry's `children`, a pool's `functions`) keep it as data, as
    /// the issue's contract names loot pool entries only.
    #[test]
    fn a_loot_tables_pool_entries_are_filtered_and_nothing_else() {
        let unknown = json!([{"type": "absent:condition"}]);
        let create = json!([{"type": "neoforge:mod_loaded", "modid": "create"}]);
        let ic2 = json!([{"type": "neoforge:mod_loaded", "modid": "ic2"}]);
        let table = read(
            "loot_table",
            json!({"pools": [{
                "rolls": 1,
                "entries": [
                    {"type": "minecraft:item", "name": "create:zinc", "neoforge:conditions": create},
                    {"type": "minecraft:item", "name": "ic2:tin", "neoforge:conditions": ic2},
                    {"type": "minecraft:alternatives", "children": [
                        {"type": "minecraft:item", "name": "minecraft:stick", "neoforge:conditions": unknown}
                    ]}
                ],
                "functions": [{"function": "minecraft:set_count", "count": 2, "neoforge:conditions": unknown}]
            }]}),
        );
        assert_eq!(
            table,
            json!({"pools": [{
                "rolls": 1,
                "entries": [
                    {"type": "minecraft:item", "name": "create:zinc"},
                    {"type": "minecraft:alternatives", "children": [
                        {"type": "minecraft:item", "name": "minecraft:stick", "neoforge:conditions": unknown}
                    ]}
                ],
                "functions": [{"function": "minecraft:set_count", "count": 2, "neoforge:conditions": unknown}]
            }]})
        );
    }

    /// #1381: in a predicate, an item modifier, an enchantment or a recipe, a
    /// list element's `neoforge:conditions` is ordinary data: never evaluated,
    /// so even an unknown condition keeps the element, and the key stays.
    #[test]
    fn other_kinds_keep_a_list_elements_conditions_as_data() {
        let element =
            json!({"item": "ic2:tin", "neoforge:conditions": [{"type": "absent:condition"}]});
        for kind in ["predicate", "item_modifier", "enchantment", "recipe"] {
            let value = json!({"terms": [element.clone()], "extra": {"list": [element.clone()]}});
            assert_eq!(read(kind, value.clone()), value, "{kind}");
        }
    }

    /// The whole-file check still holds for every kind.
    #[test]
    fn a_files_own_failing_conditions_still_omit_it() {
        for kind in ["loot_table", "predicate", "recipe"] {
            let files = [file(
                kind,
                json!({"neoforge:conditions": [{"type": "neoforge:mod_loaded", "modid": "ic2"}]}),
            )];
            let conditions = LoadConditions::new(["create"]);
            let overlay = PackOverlay {
                files: &files,
                conditions: &conditions,
            };
            let mut out = HashMap::new();
            overlay_map(&overlay, kind, &mut out);
            assert!(out.is_empty(), "{kind}");
        }
    }
}
