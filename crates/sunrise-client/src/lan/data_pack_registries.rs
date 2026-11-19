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
        text_registries: &crate::command::Registries,
        sound_exists: impl Fn(&str) -> bool,
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
                    let value = decode_definition(
                        registry,
                        &id.to_string(),
                        value,
                        text_registries,
                        &sound_exists,
                    )?;
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

fn decode_definition(
    registry: &str,
    id: &str,
    mut value: Value,
    text_registries: &crate::command::Registries,
    sound_exists: &dyn Fn(&str) -> bool,
) -> Result<Value, String> {
    let decode_text = |field: &str, value: &Value| {
        crate::text_grammar::build(value, text_registries, false)
            .ok_or_else(|| format!("{field}: invalid JSON text component"))
    };
    match registry {
        "minecraft:dialog" => {
            crate::dialog::Dialog::parse(&json_to_nbt(&value))
                .ok_or("invalid dialog definition")?;
        }
        "minecraft:dimension_type" => {
            value = sunrise_world::dimension_type::DimensionFacts::decode_pack_definition(value)?;
        }
        "minecraft:enchantment" => {
            value = sunrise_items::enchant::decode_pack_definition(value)?;
            value["description"] = decode_text("description", &value["description"])?;
        }
        "minecraft:painting_variant" => {
            let painting = crate::entities::PaintingVariant::decode(id, &value)?;
            for (field, text) in [("title", painting.title), ("author", painting.author)] {
                if let Some(text) = text {
                    if text.is_null() {
                        value
                            .as_object_mut()
                            .expect("decoded painting object")
                            .remove(field);
                    } else {
                        value[field] = decode_text(field, &text)?;
                    }
                }
            }
        }
        "minecraft:instrument" => {
            let instrument = sunrise_items::items::Instrument::decode(&value)?;
            if value["sound_event"].is_string() && !sound_exists(&instrument.sound) {
                return Err(format!("sound_event: unknown holder {}", instrument.sound));
            }
            value["description"] = decode_text("description", &value["description"])?;
        }
        "minecraft:banner_pattern" => {
            crate::banner::Pattern::decode(id, &value)?;
        }
        _ => unreachable!("only selected registry families are decoded"),
    }
    Ok(value)
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
                "dimension_type/test:wrong_scale".into(),
                vec![br#"{"min_y":0,"height":64,"logical_height":64,"has_skylight":true,"has_ceiling":false,"has_ender_dragon_fight":false,"infiniburn":["minecraft:netherrack"],"coordinate_scale":"one","ambient_light":0,"monster_spawn_light_level":0,"monster_spawn_block_light_limit":0}"#.to_vec()],
            ),
            ("enchantment/test:not_read".into(), vec![b"{}".to_vec()]),
            ("enchantment/test:wrong_level".into(), vec![br#"{"max_level":"seven"}"#.to_vec()]),
            ("painting_variant/test:numeric_style".into(), vec![br#"{"asset_id":"test:mural","width":1,"height":1,"title":{"text":"No","bold":1}}"#.to_vec()]),
            (
                "painting_variant/test:bad".into(),
                vec![br#"{"asset_id":"test:mural","width":17,"height":1}"#.to_vec()],
            ),
            (
                "instrument/test:bad".into(),
                vec![br#"{"sound_event":"minecraft:item.goat_horn.sound.0","use_duration":-1,"range":128,"description":"Broken"}"#.to_vec()],
            ),
            (
                "banner_pattern/test:bad".into(),
                vec![br#"{"asset_id":"test:banner","translation_key":7}"#.to_vec()],
            ),
        ];
        let io = BTreeMap::from([("enchantment/test:not_read".into(), "read failed".into())]);
        let Err(error) = PackRegistryDefinitions::load(
            &PackOverlay::new(&files),
            &io,
            &Default::default(),
            |_| true,
        ) else {
            panic!("world registry load accepted invalid entries");
        };
        assert!(error.contains("\nminecraft:dialog:\n  test:broken: EOF"));
        assert!(error.contains(
            "\nminecraft:dimension_type:\n  test:wrong_scale: coordinate_scale must be a number"
        ));
        assert!(error.contains("\nminecraft:enchantment:\n  test:not_read: read failed\n  test:wrong_level: max_level must be an integer"));
        assert!(error.contains("  test:numeric_style: title: invalid JSON text component"));
        assert!(error.contains(
            "\nminecraft:painting_variant:\n  test:bad: width must be an integer in 1..=16"
        ));
        assert!(
            error
                .contains("\nminecraft:instrument:\n  test:bad: use_duration must be non-negative")
        );
        assert!(
            error.contains("\nminecraft:banner_pattern:\n  test:bad: translation_key is required")
        );
        assert!(!error.contains("condition_false"));
    }

    #[test]
    fn admitted_wire_text_drops_json_null_styles() {
        let files = [(
            "instrument/test:horn".into(),
            vec![
                br#"{
            "sound_event":{"sound_id":"test:horn"}, "use_duration":2, "range":64,
            "description":{"text":"Horn","bold":null,"italic":true}
        }"#
                .to_vec(),
            ],
        )];
        let loaded = PackRegistryDefinitions::load(
            &PackOverlay::new(&files),
            &BTreeMap::new(),
            &Default::default(),
            |_| false,
        )
        .unwrap();
        let mut entries = Vec::new();
        loaded.append("minecraft:instrument", &mut entries);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].1["description"],
            serde_json::json!({"text":"Horn","italic":true})
        );
    }
}
