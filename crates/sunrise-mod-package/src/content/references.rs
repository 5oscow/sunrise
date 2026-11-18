//! The references a content mod's files make to each other, which `Content::read` leaves for the
//! game to meet at load: a blockstate's models, a model's parent and textures, an item
//! definition's model, the ids a recipe, a tag or a loot table names in the mod's own namespaces,
//! and what a block needs to be playable (a blockstate, a loot table, a name).
//!
//! Ids outside the mod's namespaces (vanilla's blocks, items, templates and loot functions) are not
//! checked: their tables live in the game's prepared resources, which no package tool reads.
//! The game accepts every finding here and draws or drops the thing (the missing model, a recipe
//! or tag that does not load, a block that drops nothing); `errors` are those the game drops or
//! draws broken, `warnings` the legal ones that are almost certainly mistakes.
use super::{Content, FileReader};
use crate::manifest::{Manifest, Side};
use serde_json::Value;
use std::collections::BTreeSet;

/// What [`check_references`] found, each a sentence that names the file.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Findings {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// The most declared ids a message lists.
const LISTED: usize = 8;

/// Checks the references among `content`'s files; `Err` only when a file cannot be read.
/// A mod with code (`native`, `wasm`) also declares ids in code, so the ids it declares in files
/// are not taken for all of them; a mod with `jar` layers may find assets there.
pub fn check_references(content: &Content, manifest: &Manifest) -> Result<Findings, String> {
    let mut check = References {
        content,
        manifest,
        reader: content.reader()?,
        code: manifest.native.is_some() || manifest.wasm.is_some(),
        layered: !manifest.jar.is_empty(),
        blocks: content.blocks.iter().map(|b| b.id.as_str()).collect(),
        items: content
            .items
            .iter()
            .map(|i| i.id.as_str())
            .chain(
                content
                    .blocks
                    .iter()
                    .filter(|b| b.item)
                    .map(|b| b.id.as_str()),
            )
            .collect(),
        findings: Findings::default(),
    };
    for name in &content.files {
        let mut parts = name.splitn(3, '/');
        let (top, namespace, rest) = (parts.next(), parts.next(), parts.next());
        let (Some(top), Some(namespace), Some(rest)) = (top, namespace, rest) else {
            continue;
        };
        let Some(path) = rest.strip_suffix(".json") else {
            continue;
        };
        let owned = content.namespaces.iter().any(|n| n == namespace);
        match (top, path.split_once('/')) {
            ("assets", Some(("blockstates", _))) if owned => check.blockstate(name)?,
            ("assets", Some(("models", _))) if owned => check.model(name)?,
            ("assets", Some(("items", _))) if owned => check.item_definition(name)?,
            ("data", Some(("recipe", _))) if owned => check.recipe(name)?,
            ("data", Some(("loot_table", _))) if owned => check.loot_table(name)?,
            ("data", Some(("tags", tag))) => {
                if let Some((registry, _)) = tag.split_once('/') {
                    check.tag(name, registry)?;
                }
            }
            _ => {}
        }
    }
    check.declarations()?;
    check.side();
    // Four slots of one model naming one missing texture are one finding.
    for list in [&mut check.findings.errors, &mut check.findings.warnings] {
        let mut seen = BTreeSet::new();
        list.retain(|message| seen.insert(message.clone()));
    }
    Ok(check.findings)
}

struct References<'a> {
    content: &'a Content,
    manifest: &'a Manifest,
    reader: FileReader<'a>,
    code: bool,
    layered: bool,
    blocks: BTreeSet<&'a str>,
    items: BTreeSet<&'a str>,
    findings: Findings,
}

impl References<'_> {
    fn json(&mut self, name: &str) -> Result<Option<Value>, String> {
        let bytes = self.reader.read(name)?;
        match serde_json::from_slice(&bytes) {
            Ok(value) => Ok(Some(value)),
            Err(e) => {
                self.findings.errors.push(format!(
                    "{name}: not valid JSON, so the game drops it: {e}."
                ));
                Ok(None)
            }
        }
    }

    fn has(&self, file: &str) -> bool {
        self.content
            .files
            .binary_search_by(|f| f.as_str().cmp(file))
            .is_ok()
    }

    /// The path of `id` when it is in one of the mod's namespaces.
    fn own<'i>(&self, id: &'i str) -> Option<(&'i str, &'i str)> {
        let (namespace, path) = id.split_once(':')?;
        self.content
            .namespaces
            .iter()
            .any(|n| n == namespace)
            .then_some((namespace, path))
    }

    /// `id` as a file of this mod: `assets/<ns>/<folder>/<path><extension>`.
    /// Fails with the finding's text when the file is missing.
    fn asset(&self, id: &str, folder: &str, extension: &str, what: &str) -> Option<String> {
        if self.layered {
            return None;
        }
        let (namespace, path) = self.own(id)?;
        let file = format!("assets/{namespace}/{folder}/{path}{extension}");
        (!self.has(&file)).then(|| {
            format!("the {what} {id} is not a file of this mod (expected {file}). Add the file or fix the id")
        })
    }

    fn missing(&mut self, name: &str, id: &str, folder: &str, extension: &str, what: &str) {
        if let Some(problem) = self.asset(id, folder, extension, what) {
            let near = near_file(self, folder, extension, id);
            self.findings
                .errors
                .push(format!("{name}: {problem}{near}."));
        }
    }

    fn blockstate(&mut self, name: &str) -> Result<(), String> {
        if let Some(value) = self.json(name)? {
            for model in strings_under(&value, |key, _| key == "model") {
                self.missing(name, &model, "models", ".json", "model");
            }
        }
        Ok(())
    }

    fn model(&mut self, name: &str) -> Result<(), String> {
        let Some(Value::Object(model)) = self.json(name)? else {
            return Ok(());
        };
        if let Some(Value::String(parent)) = model.get("parent") {
            self.missing(name, parent, "models", ".json", "parent model");
        }
        if let Some(Value::Object(textures)) = model.get("textures") {
            for texture in textures.values() {
                let id = match texture {
                    Value::String(id) => id,
                    Value::Object(sprite) => match sprite.get("sprite") {
                        Some(Value::String(id)) => id,
                        _ => continue,
                    },
                    _ => continue,
                };
                if !id.starts_with('#') {
                    self.missing(name, id, "textures", ".png", "texture");
                }
            }
        }
        Ok(())
    }

    fn item_definition(&mut self, name: &str) -> Result<(), String> {
        if let Some(value) = self.json(name)? {
            let models = strings_under(&value, |key, parent| {
                key == "base" || (key == "model" && type_of(parent) == Some("minecraft:model"))
            });
            for model in models {
                self.missing(name, &model, "models", ".json", "model");
            }
        }
        Ok(())
    }

    fn recipe(&mut self, name: &str) -> Result<(), String> {
        let Some(value) = self.json(name)? else {
            return Ok(());
        };
        let mut ids = Vec::new();
        recipe_ids(&value, false, &mut ids);
        for id in ids {
            self.id_in_registry(name, &id, "item", "the ingredient or result");
        }
        Ok(())
    }

    fn loot_table(&mut self, name: &str) -> Result<(), String> {
        let Some(value) = self.json(name)? else {
            return Ok(());
        };
        let mut found = Vec::new();
        loot_ids(&value, &mut found);
        for (registry, id) in found {
            self.id_in_registry(name, &id, registry, "the loot entry");
        }
        Ok(())
    }

    fn tag(&mut self, name: &str, registry: &str) -> Result<(), String> {
        let Some(value) = self.json(name)? else {
            return Ok(());
        };
        let Some(Value::Array(values)) = value.get("values") else {
            return Ok(());
        };
        for entry in values {
            let id = match entry {
                Value::String(id) => id,
                Value::Object(object) => match object.get("id") {
                    Some(Value::String(id)) => id,
                    _ => continue,
                },
                _ => continue,
            };
            if let Some(tag) = id.strip_prefix('#') {
                if let Some((namespace, path)) = self.own(tag)
                    && !self.has(&format!("data/{namespace}/tags/{registry}/{path}.json"))
                {
                    self.findings.errors.push(format!(
                        "{name}: the tag {id} is not a tag file of this mod (expected data/{namespace}/tags/{registry}/{path}.json), so the tag does not load. Add the file or fix the id."
                    ));
                }
            } else {
                self.id_in_registry(name, id, registry, "the tag value");
            }
        }
        Ok(())
    }

    /// `id` of a vanilla-shaped registry the mod declares into: an error when it is in the mod's
    /// namespace and the mod does not declare it.
    fn id_in_registry(&mut self, name: &str, id: &str, registry: &str, what: &str) {
        if self.code || self.own(id).is_none() {
            return;
        }
        let known: Vec<&str> = match registry {
            "block" => self.blocks.iter().copied().collect(),
            "item" => self.items.iter().copied().collect(),
            "fluid" => self.content.fluids.iter().map(|f| f.id.as_str()).collect(),
            "entity_type" => self
                .content
                .entity_types
                .iter()
                .map(|t| t.id.as_str())
                .collect(),
            _ => return,
        };
        if known.contains(&id) {
            return;
        }
        let article = if registry == "item" || registry == "entity_type" {
            "an"
        } else {
            "a"
        };
        let near = nearest(id, &known)
            .map(|n| format!(" Did you mean {n}?"))
            .unwrap_or_default();
        self.findings.errors.push(format!(
            "{name}: {what} {id} is not {article} {} this mod declares ({}).{near} Fix the id.",
            registry.replace('_', " "),
            self.listed(&known),
        ));
    }

    fn listed(&self, known: &[&str]) -> String {
        if known.is_empty() {
            return "none declared".into();
        }
        let mut names: Vec<&str> = known
            .iter()
            .map(|id| {
                id.strip_prefix(self.manifest.id.as_str())
                    .and_then(|rest| rest.strip_prefix(':'))
                    .unwrap_or(id)
            })
            .collect();
        names.sort_unstable();
        let more = names.len().saturating_sub(LISTED);
        names.truncate(LISTED);
        let mut text = format!("declared: {}", names.join(", "));
        if more > 0 {
            text.push_str(&format!(", and {more} more"));
        }
        text
    }

    /// What each block and item declared in a file needs: a blockstate, an item definition, a
    /// loot table, a name.
    fn declarations(&mut self) -> Result<(), String> {
        let mut names = Vec::new();
        let content = self.content;
        for block in &content.blocks {
            let Some((namespace, path)) = self.own(&block.id) else {
                continue;
            };
            if !self.has(&format!("data/{namespace}/sunrise/block/{path}.json")) {
                continue;
            }
            let file = format!("data/{namespace}/sunrise/block/{path}.json");
            let invisible = block.properties.render_shape == Some(super::RenderShape::Invisible);
            let state = format!("assets/{namespace}/blockstates/{path}.json");
            if !invisible && !self.layered && !self.has(&state) {
                self.findings.errors.push(format!(
                    "{file}: the block {} has no model: expected {state}, which names its models. Add it, or declare render_shape INVISIBLE.",
                    block.id
                ));
            }
            if block.item {
                self.item_definition_exists(&file, &block.id, namespace, path);
            }
            let loot = format!("data/{namespace}/loot_table/blocks/{path}.json");
            if !self.has(&loot) {
                self.findings.warnings.push(format!(
                    "{file}: the block {} has no loot table, so it drops nothing when broken. Add {loot}.",
                    block.id
                ));
            }
            names.push((
                format!("block.{namespace}.{}", path.replace('/', ".")),
                file,
            ));
        }
        for item in &content.items {
            let Some((namespace, path)) = self.own(&item.id) else {
                continue;
            };
            let file = format!("data/{namespace}/sunrise/item/{path}.json");
            if !self.has(&file) {
                continue;
            }
            self.item_definition_exists(&file, &item.id, namespace, path);
            names.push((format!("item.{namespace}.{}", path.replace('/', ".")), file));
        }
        self.names(names)
    }

    fn item_definition_exists(&mut self, file: &str, id: &str, namespace: &str, path: &str) {
        let definition = format!("assets/{namespace}/items/{path}.json");
        if !self.layered && !self.has(&definition) {
            self.findings.warnings.push(format!(
                "{file}: the item {id} has no item definition, so it is drawn as the missing model. Add {definition}."
            ));
        }
    }

    /// Each `(language key, declaring file)` has a name in its namespace's `en_us.json`.
    fn names(&mut self, wanted: Vec<(String, String)>) -> Result<(), String> {
        let mut by_namespace: std::collections::BTreeMap<String, Option<Value>> =
            Default::default();
        for (key, file) in wanted {
            let namespace = file.split('/').nth(1).unwrap_or_default().to_owned();
            if !by_namespace.contains_key(&namespace) {
                let lang = format!("assets/{namespace}/lang/en_us.json");
                let value = if self.has(&lang) {
                    self.json(&lang)?
                } else {
                    None
                };
                by_namespace.insert(namespace.clone(), value);
            }
            let named = by_namespace[&namespace]
                .as_ref()
                .is_some_and(|lang| lang.get(&key).is_some());
            if !named && !self.layered {
                let lang = format!("assets/{namespace}/lang/en_us.json");
                let why = if self.has(&lang) {
                    format!("{lang} has no {key:?}")
                } else {
                    format!("there is no {lang}")
                };
                self.findings.warnings.push(format!(
                    "{file}: {why}, so players see the raw key {key}. Add the name."
                ));
            }
        }
        Ok(())
    }

    /// Registry content on a client-only package: joining a server compares the mods, and the
    /// package's side defaults to `client` when the manifest has no `side` line.
    fn side(&mut self) {
        let content = &self.content;
        let registry = !(content.blocks.is_empty()
            && content.items.is_empty()
            && content.effects.is_empty()
            && content.fluids.is_empty()
            && content.entity_types.is_empty()
            && content.creative_tabs.is_empty());
        if registry && self.manifest.side == Side::Client {
            self.findings.warnings.push(
                "sunrise-mod.toml: side is client (the default when the line is left out), but this mod adds blocks, items or other registry content that a server and its players must have too; set side = \"both\" or joining a server fails."
                    .into(),
            );
        }
    }
}

/// `", did you mean <id>"` for a missing file, from the files of the same folder.
fn near_file(check: &References<'_>, folder: &str, extension: &str, id: &str) -> String {
    let Some((namespace, _)) = check.own(id) else {
        return String::new();
    };
    let prefix = format!("assets/{namespace}/{folder}/");
    let known: Vec<String> = check
        .content
        .files
        .iter()
        .filter_map(|f| {
            let path = f.strip_prefix(&prefix)?.strip_suffix(extension)?;
            Some(format!("{namespace}:{path}"))
        })
        .collect();
    let known: Vec<&str> = known.iter().map(String::as_str).collect();
    nearest(id, &known)
        .map(|n| format!(" (did you mean {n}?)"))
        .unwrap_or_default()
}

/// The id among `known` within two edits of `id`, the closest first.
fn nearest<'k>(id: &str, known: &[&'k str]) -> Option<&'k str> {
    known
        .iter()
        .map(|k| (edits(id, k), *k))
        .filter(|(d, _)| *d <= 2)
        .min()
        .map(|(_, k)| k)
}

/// The number of single-character changes from `a` to `b` (Levenshtein).
fn edits(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(ca != *cb));
            diagonal = above;
        }
    }
    row[b.len()]
}

fn type_of(object: &Value) -> Option<&str> {
    object.get("type").and_then(Value::as_str)
}

/// Every string value under a key `wanted` picks, with the object holding the key.
fn strings_under(value: &Value, wanted: impl Fn(&str, &Value) -> bool) -> Vec<String> {
    fn walk(value: &Value, wanted: &dyn Fn(&str, &Value) -> bool, out: &mut Vec<String>) {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    match child {
                        Value::String(text) if wanted(key, value) => out.push(text.clone()),
                        _ => walk(child, wanted, out),
                    }
                }
            }
            Value::Array(list) => list.iter().for_each(|child| walk(child, wanted, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, &wanted, &mut out);
    out
}

/// The item ids a recipe names: under `result`, `key`, `ingredient(s)` and the like, wherever the
/// recipe kind puts them; data (`components`, `type`) is left alone.
fn recipe_ids(value: &Value, naming: bool, out: &mut Vec<String>) {
    const NAMING: [&str; 13] = [
        "result",
        "key",
        "ingredient",
        "ingredients",
        "item",
        "id",
        "base",
        "addition",
        "template",
        "input",
        "material",
        "items",
        "remainder",
    ];
    match value {
        Value::String(id) if naming && !id.starts_with('#') => out.push(id.clone()),
        Value::Array(list) => list.iter().for_each(|v| recipe_ids(v, naming, out)),
        Value::Object(object) => {
            for (key, child) in object {
                if matches!(key.as_str(), "components" | "type" | "category" | "group") {
                    continue;
                }
                recipe_ids(child, naming || NAMING.contains(&key.as_str()), out);
            }
        }
        _ => {}
    }
}

/// The `(registry, id)` a loot table names: an item entry's `name`, a state condition's `block`.
fn loot_ids(value: &Value, out: &mut Vec<(&'static str, String)>) {
    match value {
        Value::Object(object) => {
            let named = |key: &str| object.get(key).and_then(Value::as_str);
            match (type_of(value), named("name"), named("block")) {
                (Some("minecraft:item"), Some(name), _) => out.push(("item", name.to_owned())),
                (Some("minecraft:block_state_property"), _, Some(block)) => {
                    out.push(("block", block.to_owned()));
                }
                _ => {}
            }
            object.values().for_each(|child| loot_ids(child, out));
        }
        Value::Array(list) => list.iter().for_each(|child| loot_ids(child, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Package;

    const MANIFEST: &str = "id = \"gb\"\nname = \"gb\"\nversion = \"0.1.0\"\napi = \"0.2\"\nside = \"both\"\ncapabilities = [\"registries\"]\n";
    const GOOD: &[(&str, &str)] = &[
        (
            "data/gb/sunrise/block/slab.json",
            r#"{"template":"minecraft:smooth_stone_slab"}"#,
        ),
        (
            "data/gb/sunrise/item/shard.json",
            r#"{"template":"minecraft:diamond"}"#,
        ),
        (
            "assets/gb/blockstates/slab.json",
            r#"{"variants":{"":{"model":"gb:block/slab"}}}"#,
        ),
        (
            "assets/gb/models/block/slab.json",
            r##"{"parent":"minecraft:block/slab","textures":{"side":"gb:block/stone","top":"#side"}}"##,
        ),
        (
            "assets/gb/models/item/shard.json",
            r#"{"parent":"minecraft:item/generated","textures":{"layer0":"gb:item/shard"}}"#,
        ),
        (
            "assets/gb/items/slab.json",
            r#"{"model":{"type":"minecraft:model","model":"gb:block/slab"}}"#,
        ),
        (
            "assets/gb/items/shard.json",
            r#"{"model":{"type":"minecraft:model","model":"gb:item/shard"}}"#,
        ),
        ("assets/gb/textures/block/stone.png", "png"),
        ("assets/gb/textures/item/shard.png", "png"),
        (
            "assets/gb/lang/en_us.json",
            r#"{"block.gb.slab":"Slab","item.gb.shard":"Shard"}"#,
        ),
        (
            "data/gb/loot_table/blocks/slab.json",
            r#"{"type":"minecraft:block","pools":[{"entries":[{"type":"minecraft:item","name":"gb:slab"}]}]}"#,
        ),
        (
            "data/gb/recipe/slab.json",
            r#"{"type":"minecraft:crafting_shaped","pattern":["SS"],"key":{"S":"gb:shard"},"result":{"id":"gb:slab","count":6}}"#,
        ),
        (
            "data/gb/tags/item/things.json",
            r#"{"values":["gb:shard","gb:slab","minecraft:stick"]}"#,
        ),
    ];

    /// The good mod with `changes` applied: a file replaced, or removed when its text is empty.
    fn findings(manifest: &str, changes: &[(&str, &str)]) -> Findings {
        let dir = tempfile::tempdir().unwrap();
        let mut files: Vec<(&str, &str)> = GOOD.to_vec();
        for (name, text) in changes {
            files.retain(|(n, _)| n != name);
            if !text.is_empty() {
                files.push((name, text));
            }
        }
        std::fs::write(dir.path().join(crate::manifest::MANIFEST), manifest).unwrap();
        for (name, text) in files {
            let path = dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let package = Package::read(dir.path()).unwrap();
        let content = Content::read(dir.path(), &package.manifest).unwrap();
        check_references(&content, &package.manifest).unwrap()
    }

    fn errors(changes: &[(&str, &str)]) -> Vec<String> {
        findings(MANIFEST, changes).errors
    }

    #[test]
    fn a_consistent_mod_has_no_findings() {
        assert_eq!(findings(MANIFEST, &[]), Findings::default());
    }

    #[test]
    fn a_missing_texture_names_the_model_and_the_file() {
        let e = errors(&[("assets/gb/textures/block/stone.png", "")]);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].starts_with("assets/gb/models/block/slab.json: the texture gb:block/stone"));
        assert!(e[0].contains("expected assets/gb/textures/block/stone.png"));
        // Two slots naming it are one finding.
        let two = r#"{"textures":{"a":"gb:block/gone","b":"gb:block/gone"}}"#;
        assert_eq!(
            errors(&[("assets/gb/models/block/slab.json", two)]).len(),
            1
        );
    }

    #[test]
    fn a_missing_model_is_named_with_a_near_miss() {
        let e = errors(&[("assets/gb/models/block/slab.json", "")]);
        // The blockstate and the item definition both name it.
        assert_eq!(e.len(), 2, "{e:?}");
        assert!(e[0].contains("the model gb:block/slab is not a file"));
        let e = errors(&[(
            "assets/gb/blockstates/slab.json",
            r#"{"variants":{"":{"model":"gb:block/slabb"}}}"#,
        )]);
        assert!(e[0].contains("did you mean gb:block/slab?"), "{e:?}");
    }

    #[test]
    fn a_block_without_a_blockstate_has_no_model_unless_invisible() {
        let e = errors(&[("assets/gb/blockstates/slab.json", "")]);
        assert!(e[0].contains("the block gb:slab has no model"), "{e:?}");
        let invisible =
            r#"{"template":"minecraft:stone","properties":{"render_shape":"INVISIBLE"}}"#;
        let e = errors(&[
            ("assets/gb/blockstates/slab.json", ""),
            ("data/gb/sunrise/block/slab.json", invisible),
        ]);
        assert!(e.is_empty(), "{e:?}");
    }

    #[test]
    fn a_recipe_item_of_this_mod_must_be_declared() {
        let recipe = r#"{"type":"minecraft:crafting_shaped","pattern":["S"],"key":{"S":"gb:shrad"},"result":{"id":"minecraft:stick","count":1}}"#;
        let e = errors(&[("data/gb/recipe/slab.json", recipe)]);
        assert_eq!(e.len(), 1, "{e:?}");
        assert_eq!(
            e[0],
            "data/gb/recipe/slab.json: the ingredient or result gb:shrad is not an item this mod declares (declared: shard, slab). Did you mean gb:shard? Fix the id."
        );
    }

    #[test]
    fn a_tag_value_of_this_mod_must_be_declared() {
        let e = errors(&[(
            "data/gb/tags/item/things.json",
            r##"{"values":["gb:nope","#gb:other"]}"##,
        )]);
        assert_eq!(e.len(), 2, "{e:?}");
        assert!(e[0].contains("the tag value gb:nope is not an item"));
        assert!(e[1].contains("the tag #gb:other is not a tag file"));
        // A vanilla tag file lists this mod's blocks; a block tag checks blocks.
        let e = errors(&[(
            "data/minecraft/tags/block/slabs.json",
            r#"{"values":["gb:slab","gb:shard"]}"#,
        )]);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("gb:shard is not a block"));
    }

    #[test]
    fn a_loot_entry_of_this_mod_must_be_declared() {
        let table = r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"gb:slabs"}]}]}"#;
        let e = errors(&[("data/gb/loot_table/blocks/slab.json", table)]);
        assert!(
            e[0].contains("the loot entry gb:slabs is not an item"),
            "{e:?}"
        );
    }

    #[test]
    fn a_broken_json_file_is_an_error() {
        let e = errors(&[("data/gb/recipe/slab.json", "{")]);
        assert!(
            e[0].starts_with("data/gb/recipe/slab.json: not valid JSON"),
            "{e:?}"
        );
    }

    #[test]
    fn a_block_with_no_loot_table_or_name_is_a_warning() {
        let f = findings(
            MANIFEST,
            &[
                ("data/gb/loot_table/blocks/slab.json", ""),
                ("assets/gb/lang/en_us.json", r#"{"item.gb.shard":"Shard"}"#),
            ],
        );
        assert!(f.errors.is_empty(), "{f:?}");
        assert_eq!(f.warnings.len(), 2, "{f:?}");
        assert!(f.warnings[0].contains("has no loot table"));
        assert!(f.warnings[1].contains("has no \"block.gb.slab\""));
        let f = findings(MANIFEST, &[("assets/gb/lang/en_us.json", "")]);
        assert!(
            f.warnings
                .iter()
                .all(|w| w.contains("there is no assets/gb/lang/en_us.json"))
        );
        assert_eq!(f.warnings.len(), 2);
    }

    #[test]
    fn registry_content_on_a_client_package_is_a_warning() {
        let client = MANIFEST.replace("side = \"both\"\n", "");
        let f = findings(&client, &[]);
        assert!(f.errors.is_empty());
        assert_eq!(f.warnings.len(), 1, "{f:?}");
        assert!(f.warnings[0].contains("set side = \"both\""));
    }
}
