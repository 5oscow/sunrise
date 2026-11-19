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
            value =
                sunrise_world::dimension_type::DimensionFacts::decode_pack_definition(id, &value)?;
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
            (
                "dimension_type/test:empty_bed_rule".into(),
                vec![br#"{"min_y":0,"height":64,"logical_height":64,"has_skylight":true,"has_ceiling":false,"has_ender_dragon_fight":false,"infiniburn":["minecraft:netherrack"],"coordinate_scale":1,"ambient_light":0,"monster_spawn_light_level":0,"monster_spawn_block_light_limit":0,"attributes":{"minecraft:gameplay/bed_rule":{}}}"#.to_vec()],
            ),
            ("enchantment/test:not_read".into(), vec![b"{}".to_vec()]),
            ("enchantment/test:wrong_level".into(), vec![br#"{"max_level":"seven"}"#.to_vec()]),
            ("painting_variant/test:numeric_style".into(), vec![br#"{"asset_id":"test:mural","width":1,"height":1,"title":{"text":"No","bold":1}}"#.to_vec()]),
            ("painting_variant/test:numeric_object".into(), vec![br#"{"asset_id":"test:mural","width":1,"height":1,"author":{"object":7,"sprite":"test:icon"}}"#.to_vec()]),
            ("painting_variant/test:legacy_click".into(), vec![br#"{"asset_id":"test:mural","width":1,"height":1,"title":{"text":"Label","hover_event":{"action":"show_text","value":{"text":"Click","click_event":{"action":"run_command","value":"/help"}}}}}"#.to_vec()]),
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
        for (registry, defs) in [
            ("dialog", &[("broken", "")][..]),
            (
                "dimension_type",
                &[
                    ("wrong_scale", "coordinate_scale"),
                    ("empty_bed_rule", "bed_rule"),
                ],
            ),
            (
                "enchantment",
                &[("not_read", "read failed"), ("wrong_level", "max_level")],
            ),
            (
                "painting_variant",
                &[
                    ("numeric_style", "title"),
                    ("numeric_object", "author"),
                    ("legacy_click", "title"),
                    ("bad", "width"),
                ],
            ),
            ("instrument", &[("bad", "use_duration")]),
            ("banner_pattern", &[("bad", "translation_key")]),
        ] {
            let header = format!("\nminecraft:{registry}:\n");
            let group = error
                .split_once(&header)
                .unwrap()
                .1
                .split("\nminecraft:")
                .next()
                .unwrap();
            for (id, field) in defs {
                let entry = format!("  test:{id}:");
                assert!(
                    group
                        .lines()
                        .any(|line| line.starts_with(&entry) && line.contains(field)),
                    "{error}"
                );
            }
        }
        assert!(!error.contains("condition_false"));
    }
}
