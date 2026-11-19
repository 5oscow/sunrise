//! Winning definitions for the synchronized registries that read world packs.
use super::{PACK_REGISTRIES, json_to_nbt};
use serde_json::Value;
use std::collections::BTreeMap;
use sunrise_core::{
    identifier::Identifier,
    nbt::{Compound, Tag},
};
use sunrise_world::loot::PackOverlay;

/// Validated once for the world session. A false condition suppresses the
/// winning entry, including any prepared or lower-priority definition.
#[derive(Default)]
pub struct PackRegistryDefinitions {
    entries: BTreeMap<&'static str, BTreeMap<(String, String), Option<Definition>>>,
}

/// Enchantment holder-set binding still consumes JSON. Other definitions need
/// no further binding here and can keep their codec's exact NBT numeric types.
enum Definition {
    Encoded(Tag),
    Enchantment(Value),
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

    pub(super) fn append_enchantments(&self, entries: &mut Vec<((String, String), Value, bool)>) {
        if let Some(pack) = self.entries.get("minecraft:enchantment") {
            entries.extend(pack.iter().filter_map(|(key, value)| match value {
                Some(Definition::Enchantment(value)) => Some((key.clone(), value.clone(), true)),
                _ => None,
            }));
        }
    }

    pub(super) fn append_encoded(
        &self,
        registry: &str,
        entries: &mut Vec<((String, String), Tag, bool)>,
    ) {
        if let Some(pack) = self.entries.get(registry) {
            entries.extend(pack.iter().filter_map(|(key, value)| match value {
                Some(Definition::Encoded(value)) => Some((key.clone(), value.clone(), true)),
                _ => None,
            }));
        }
    }
}

fn decode_definition(
    registry: &str,
    id: &str,
    value: Value,
    text_registries: &crate::command::Registries,
    sound_exists: &dyn Fn(&str) -> bool,
) -> Result<Definition, String> {
    let decode_text = |field: &str, value: &Value| {
        crate::text_grammar::build(value, text_registries, false)
            .ok_or_else(|| format!("{field}: invalid JSON text component"))
    };
    let string = |value: &str| Tag::String(value.into());
    let asset_id = |asset: &str| {
        // The render readers store the default namespace as a path only.
        if asset.contains(':') {
            string(asset)
        } else {
            string(&format!("minecraft:{asset}"))
        }
    };
    let encoded = match registry {
        "minecraft:dialog" => {
            use crate::dialog::{AfterAction, Dialog, Kind};
            let source = json_to_nbt(&value);
            let dialog = Dialog::decode_pack(&source, &value)
                .map_err(|field| format!("invalid dialog field {field}"))?;
            let mut fields = Compound::from([(
                "title".into(),
                json_to_nbt(&decode_text("title", &value["title"])?),
            )]);
            if dialog.external_title.is_some() && !value["external_title"].is_null() {
                fields.insert(
                    "external_title".into(),
                    json_to_nbt(&decode_text("external_title", &value["external_title"])?),
                );
            }
            for (name, enabled) in [
                ("pause", dialog.pause),
                ("can_close_with_escape", dialog.can_close_with_escape),
            ] {
                if !enabled {
                    fields.insert(name.into(), Tag::Byte(0));
                }
            }
            match dialog.after_action {
                AfterAction::Close => {}
                action => {
                    fields.insert(
                        "after_action".into(),
                        string(if matches!(action, AfterAction::None) {
                            "none"
                        } else {
                            "wait_for_response"
                        }),
                    );
                }
            }
            // Nested codecs remain #3067. Keep only the fields the dialog reader consumed.
            let (kind, children, exit, columns, width): (_, &[&str], _, _, _) = match dialog.kind {
                Kind::Notice(_) => ("notice", &["action"], false, None, None),
                Kind::Confirmation { .. } => ("confirmation", &["yes", "no"], false, None, None),
                Kind::MultiAction { exit, columns, .. } => (
                    "multi_action",
                    &["actions"],
                    exit.is_some(),
                    Some(columns),
                    None,
                ),
                Kind::ServerLinks {
                    exit,
                    columns,
                    button_width,
                } => (
                    "server_links",
                    &[],
                    exit.is_some(),
                    Some(columns),
                    Some(button_width),
                ),
                Kind::DialogList {
                    dialogs,
                    exit,
                    columns,
                    button_width,
                    ..
                } => {
                    fields.insert("dialogs".into(), dialogs);
                    (
                        "dialog_list",
                        &[],
                        exit.is_some(),
                        Some(columns),
                        Some(button_width),
                    )
                }
            };
            fields.insert("type".into(), string(&format!("minecraft:{kind}")));
            for (name, value) in [
                (
                    "columns",
                    columns.map(|n| n.get() as i32).filter(|n| *n != 2),
                ),
                ("button_width", width.filter(|n| *n != 150)),
            ] {
                if let Some(value) = value {
                    fields.insert(name.into(), Tag::Int(value));
                }
            }
            let keys = children
                .iter()
                .copied()
                .chain((!dialog.body.is_empty()).then_some("body"))
                .chain((!dialog.inputs.is_empty()).then_some("inputs"))
                .chain(exit.then_some("exit_action"));
            for name in keys {
                if let Some(value) = source.get(name) {
                    fields.insert(name.into(), value.clone());
                }
            }
            Tag::Compound(fields)
        }
        "minecraft:dimension_type" => {
            sunrise_world::dimension_type::DimensionFacts::decode_pack_definition(id, &value)?
        }
        "minecraft:enchantment" => {
            let mut value = sunrise_items::enchant::decode_pack_definition(value)?;
            value["description"] = decode_text("description", &value["description"])?;
            return Ok(Definition::Enchantment(value));
        }
        "minecraft:painting_variant" => {
            let painting = crate::entities::PaintingVariant::decode(id, &value)?;
            let mut fields = Compound::from([
                ("asset_id".into(), asset_id(&painting.asset)),
                ("width".into(), Tag::Int(painting.width)),
                ("height".into(), Tag::Int(painting.height)),
            ]);
            for (field, text) in [("title", painting.title), ("author", painting.author)] {
                if let Some(text) = text.filter(|value| !value.is_null()) {
                    fields.insert(field.into(), json_to_nbt(&decode_text(field, &text)?));
                }
            }
            Tag::Compound(fields)
        }
        "minecraft:instrument" => {
            let instrument = sunrise_items::items::Instrument::decode(&value)?;
            let float = |v: f32| Tag::Float(v.to_bits());
            let sound = match instrument.inline_sound_range {
                None if !sound_exists(&instrument.sound) => {
                    return Err(format!("sound_event: unknown holder {}", instrument.sound));
                }
                None => string(&instrument.sound),
                Some(fixed) => {
                    let mut fields =
                        Compound::from([("sound_id".into(), string(&instrument.sound))]);
                    if let Some(range) = fixed {
                        fields.insert("range".into(), float(range));
                    }
                    Tag::Compound(fields)
                }
            };
            let mut fields = Compound::from([
                ("sound_event".into(), sound),
                ("use_duration".into(), float(instrument.use_duration)),
                ("range".into(), float(instrument.range)),
                (
                    "description".into(),
                    json_to_nbt(&decode_text("description", &value["description"])?),
                ),
            ]);
            if instrument.durability_damage != 0 {
                fields.insert(
                    "durability_damage".into(),
                    Tag::Int(instrument.durability_damage),
                );
            }
            Tag::Compound(fields)
        }
        "minecraft:banner_pattern" => {
            let pattern = crate::banner::Pattern::decode(id, &value)?;
            Tag::compound([
                ("asset_id", asset_id(&pattern.asset)),
                ("translation_key", string(&pattern.translation_key)),
            ])
        }
        // WorldClock.DIRECT_CODEC is a unit map codec: extra keys are permitted.
        "minecraft:world_clock" => {
            if !value.is_object() {
                return Err("not a map".into());
            }
            json_to_nbt(&value)
        }
        _ => unreachable!("only selected registry families are decoded"),
    };
    Ok(Definition::Encoded(encoded))
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
                "dialog/test:negative_columns".into(),
                vec![br#"{"type":"multi_action","title":"Buttons","actions":[{"label":"OK"}],"columns":-1}"#.to_vec()],
            ),
            (
                "dialog/test:wrong_columns".into(),
                vec![br#"{"type":"dialog_list","title":"Buttons","dialogs":[],"columns":true}"#.to_vec()],
            ),
            (
                "dialog/test:wide_button".into(),
                vec![br#"{"type":"server_links","title":"Links","button_width":1025}"#.to_vec()],
            ),
            (
                "dialog/test:wrong_pause".into(),
                vec![br#"{"type":"notice","title":"Pause","pause":1}"#.to_vec()],
            ),
            (
                "dialog/test:wrong_action".into(),
                vec![br#"{"type":"notice","title":"Action","after_action":"stay"}"#.to_vec()],
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
                "dimension_type/test:unknown_attribute".into(),
                vec![br#"{"min_y":0,"height":64,"logical_height":64,"has_skylight":true,"has_ceiling":false,"has_ender_dragon_fight":false,"infiniburn":["minecraft:netherrack"],"coordinate_scale":1,"ambient_light":0,"monster_spawn_light_level":0,"monster_spawn_block_light_limit":0,"attributes":{"minecraft:visual/not_an_attribute":1}}"#.to_vec()],
            ),
            (
                "dimension_type/test:empty_bed_rule".into(),
                vec![br#"{"min_y":0,"height":64,"logical_height":64,"has_skylight":true,"has_ceiling":false,"has_ender_dragon_fight":false,"infiniburn":["minecraft:netherrack"],"coordinate_scale":1,"ambient_light":0,"monster_spawn_light_level":0,"monster_spawn_block_light_limit":0,"attributes":{"minecraft:gameplay/bed_rule":{}}}"#.to_vec()],
            ),
            ("enchantment/test:not_read".into(), vec![b"{}".to_vec()]),
            ("enchantment/test:wrong_level".into(), vec![br#"{"max_level":"seven"}"#.to_vec()]),
            ("painting_variant/test:numeric_style".into(), vec![br#"{"asset_id":"test:mural","width":1,"height":1,"title":{"text":"No","bold":1}}"#.to_vec()]),
            ("painting_variant/test:numeric_object".into(), vec![br#"{"asset_id":"test:mural","width":1,"height":1,"author":{"object":7,"sprite":"test:icon"}}"#.to_vec()]),
            ("painting_variant/test:wrong_atlas".into(), vec![br#"{"asset_id":"test:mural","width":1,"height":1,"author":{"object":"atlas","atlas":7,"sprite":"test:icon","player":"Steve"}}"#.to_vec()]),
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
            (
                "dialog",
                &[
                    ("broken", ""),
                    ("negative_columns", "columns"),
                    ("wrong_columns", "columns"),
                    ("wide_button", "button_width"),
                    ("wrong_pause", "pause"),
                    ("wrong_action", "after_action"),
                ][..],
            ),
            (
                "dimension_type",
                &[
                    ("wrong_scale", "coordinate_scale"),
                    ("empty_bed_rule", "bed_rule"),
                    ("unknown_attribute", "minecraft:visual/not_an_attribute"),
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
                    ("wrong_atlas", "author"),
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
