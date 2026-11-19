//! Winning definitions for the synchronized registries that read world packs.
use super::{PACK_REGISTRIES, json_to_nbt};
use serde_json::Value;
use std::collections::BTreeMap;
use sunrise_core::identifier::Identifier;
use sunrise_world::loot::PackOverlay;

/// Validated once for the world session. A false condition suppresses the
/// winning entry, including any prepared or lower-priority definition.
#[derive(Default)]
pub struct PackRegistryDefinitions {
    entries: BTreeMap<&'static str, BTreeMap<(String, String), Option<Value>>>,
}

impl PackRegistryDefinitions {
    /// RegistryDataLoader's collected errors: try every winning definition,
    /// then refuse the entire load and report each failure by registry and id.
    pub fn load(
        overlay: &PackOverlay<'_>,
        read_errors: &BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let mut definitions = Self::default();
        let mut errors = BTreeMap::<_, BTreeMap<_, _>>::new();
        for &registry in PACK_REGISTRIES {
            let kind = registry.trim_start_matches("minecraft:");
            for (key, error) in read_errors {
                if let Some(id) = key.strip_prefix(kind).and_then(|id| id.strip_prefix('/')) {
                    errors
                        .entry(registry)
                        .or_default()
                        .insert(id.to_owned(), error.clone());
                }
            }
            for (id, files) in overlay.kind(kind) {
                if errors
                    .get(registry)
                    .is_some_and(|errors| errors.contains_key(id))
                {
                    continue;
                }
                let decode = || -> Result<_, String> {
                    let id = Identifier::parse(id).map_err(|error| error.to_string())?;
                    let key = (id.namespace().to_owned(), id.path().to_owned());
                    let value: Value = serde_json::from_slice(PackOverlay::top(files))
                        .map_err(|error| error.to_string())?;
                    let file = format!("data/{}/{kind}/{}.json", id.namespace(), id.path());
                    if !overlay.conditions.allows_file_isolated(&value, &file)? {
                        return Ok((key, None));
                    }
                    decode_definition(registry, &id.to_string(), &value)?;
                    Ok((key, Some(value)))
                };
                match decode() {
                    Ok((key, value)) => {
                        definitions
                            .entries
                            .entry(registry)
                            .or_default()
                            .insert(key, value);
                    }
                    Err(error) => {
                        errors
                            .entry(registry)
                            .or_default()
                            .insert(id.to_owned(), error);
                    }
                }
            }
        }
        if errors.is_empty() {
            return Ok(definitions);
        }
        let mut report = String::from("failed to load world registry definitions:");
        for (registry, entries) in errors {
            report.push_str(&format!("\n{registry}:"));
            for (id, error) in entries {
                report.push_str(&format!("\n  {id}: {error}"));
            }
        }
        Err(report)
    }

    pub(super) fn discard_replaced(
        &self,
        registry: &str,
        entries: &mut Vec<((String, String), Value, bool)>,
    ) {
        if let Some(pack) = self.entries.get(registry) {
            entries.retain(|(key, _, _)| !pack.contains_key(key));
        }
    }

    pub(super) fn append(
        &self,
        registry: &str,
        entries: &mut Vec<((String, String), Value, bool)>,
    ) {
        if let Some(pack) = self.entries.get(registry) {
            entries.extend(
                pack.iter()
                    .filter_map(|(key, value)| Some((key.clone(), value.clone()?, true))),
            );
        }
    }
}

fn decode_definition(registry: &str, id: &str, value: &Value) -> Result<(), String> {
    if !value.is_object() {
        return Err("expected a registry definition object".into());
    }
    match registry {
        "minecraft:dialog" => {
            crate::dialog::Dialog::parse(&json_to_nbt(value)).ok_or("invalid dialog definition")?;
        }
        "minecraft:dimension_type" => {
            use sunrise_world::dimension_type::DimensionFacts;
            let base = DimensionFacts::vanilla(id).unwrap_or_else(DimensionFacts::overworld);
            DimensionFacts::patched(base, id, value)?;
        }
        "minecraft:enchantment" => {
            sunrise_items::enchant::decode_definition(id, value)
                .map_err(|error| error.to_string())?;
        }
        _ => unreachable!("only selected registry families are decoded"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bad_winning_definition_is_reported_in_one_error() {
        let files = [
            (
                "dialog/test:broken".into(),
                vec![
                    br#"{"type":"notice","title":"Lower"}"#.to_vec(),
                    b"{".to_vec(),
                ],
            ),
            (
                "dialog/test:condition_false".into(),
                vec![br#"{"neoforge:conditions":[{"type":"neoforge:never"}]}"#.to_vec()],
            ),
            (
                "dimension_type/test:missing_fields".into(),
                vec![b"{}".to_vec()],
            ),
            ("enchantment/test:not_read".into(), vec![b"{}".to_vec()]),
        ];
        let io = BTreeMap::from([("enchantment/test:not_read".into(), "read failed".into())]);
        let Err(error) = PackRegistryDefinitions::load(&PackOverlay::new(&files), &io) else {
            panic!("world registry load accepted invalid entries");
        };
        assert!(error.contains("\nminecraft:dialog:\n  test:broken: EOF"));
        assert!(
            error.contains(
                "\nminecraft:dimension_type:\n  test:missing_fields: invalid level height"
            )
        );
        assert!(error.contains("\nminecraft:enchantment:\n  test:not_read: read failed"));
        assert!(!error.contains("condition_false"));
    }
}
