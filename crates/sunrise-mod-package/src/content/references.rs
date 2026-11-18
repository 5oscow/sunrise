//! The references a content mod's files make to each other, which `Content::read` leaves for the
//! game to meet at load: a blockstate's models, a model's parent and textures, an item
//! definition's model, the ids a recipe, a tag or a loot table names in the mod's own namespaces,
//! and what a block needs to be playable (a blockstate, a loot table, a name).
//!
//! Each file is read as the game reads it, and only the fields its format treats as references:
//! models, blockstates and item definitions through `sunrise-assets`' decoders (a byte-order mark
//! is skipped; a blockstate with a `neoforge:definition_type` and an item model type the decoder
//! does not know are a mod's own payload, left alone), the other files through the shapes the
//! recipe, tag and loot loaders walk (a recipe's ingredients and result, a tag's `values`, a loot
//! entry's `name` and a state condition's `block`), never the data beside them (components, custom
//! data, a recipe type's own fields). Comments are skipped as Gson's lenient reader skips them. A
//! file the load conditions (`neoforge:conditions`) may disable, a tag entry that is not
//! `required`, and a removal are not read: they cannot break a mod that does not have what they
//! name.
//!
//! Ids outside the mod's namespaces (vanilla's blocks, items, templates and loot functions) are
//! not checked: their tables live in the game's prepared resources, which no package tool reads.
//!
//! Severity is one rule. An **error** is a finding where the tool sees everything that decides
//! the answer and the game would certainly drop or draw the thing broken: an id of the mod's own
//! namespaces that nothing in the mod declares, in a mod with no code, no `[[jar]]` layer and no declared dependency (whose jar can supply files; the
//! namespace fence keeps every other mod out of them), a file the mod lacks when nothing else
//! could supply it, a file that is not JSON at all. A **warning** is a finding where another
//! source could supply the thing (a `[[jar]]` layer or a dependency's jar, an atlas source that renames a sprite or
//! walks a directory, a block whose template draws without a block model) or a legal mistake
//! (a block with no loot table or name); it says "could not find ... in this mod; if X supplies
//! it, ignore this". What the tool cannot judge at all is dropped, not warned: the ids a mod's
//! code declares, what the load conditions decide, a payload only a mod reads.
use super::{Content, FileReader, RenderShape};
use crate::manifest::{Manifest, Side};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use sunrise_assets::{
    blockstate::{BlockStateFile, VariantSet},
    item_model::ClientItem,
    model::{CuboidModel, TextureSlot},
};

/// What [`check_references`] found, each a sentence that names the file.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Findings {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// The most declared ids a message lists.
const LISTED: usize = 8;

/// A model file's parent and its sprites by slot, as the model decoder reads them.
struct Model {
    file: String,
    parent: Option<String>,
    textures: BTreeMap<String, String>,
    slots: BTreeSet<String>,
}

/// Checks the references among `content`'s files; `Err` only when a file cannot be read.
/// A mod with code (`native`, `wasm`) also declares ids in code, so the ids it declares in files
/// are not taken for all of them; a mod with `jar` layers may find assets and data there.
pub fn check_references(content: &Content, manifest: &Manifest) -> Result<Findings, String> {
    let atlas = |name: &String| {
        let mut parts = name.split('/');
        parts.next() == Some("assets") && parts.nth(1) == Some("atlases") && name.ends_with(".json")
    };
    let mut check = References {
        content,
        manifest,
        reader: content.reader()?,
        code: manifest.native.is_some() || manifest.wasm.is_some(),
        jar: !manifest.jar.is_empty() || !manifest.dependencies.is_empty(),
        atlas: content.files.iter().any(atlas),
        blocks: content.blocks.iter().map(|b| b.id.clone()).collect(),
        items: content
            .items
            .iter()
            .map(|i| i.id.clone())
            .chain(
                content
                    .blocks
                    .iter()
                    .filter(|b| b.item)
                    .map(|b| b.id.clone()),
            )
            .collect(),
        // A world fluid's flowing form is a fluid of its own (its block and bucket are in
        // `content.blocks` and `content.items`).
        fluids: content
            .fluids
            .iter()
            .flat_map(|f| [f.id.clone(), f.flowing_id()])
            .collect(),
        entity_types: content.entity_types.iter().map(|t| t.id.clone()).collect(),
        models: BTreeMap::new(),
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
            ("assets", Some(("models", model))) if owned => {
                check.read_model(name, namespace, model)?
            }
            ("assets", Some(("items", _))) if owned => check.item_definition(name)?,
            ("data", Some(("recipe", _))) if owned => check.recipe(name)?,
            ("data", Some(("loot_table", _))) if owned => check.loot_table(name)?,
            ("data", Some(("tags", tag))) => check.tag(name, tag)?,
            _ => {}
        }
    }
    check.check_models();
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
    /// A `[[jar]]` layer or a declared dependency's jar may supply files of the mod's namespaces.
    jar: bool,
    /// The mod has atlas sources, which can rename a sprite or take a whole directory.
    atlas: bool,
    blocks: BTreeSet<String>,
    items: BTreeSet<String>,
    fluids: BTreeSet<String>,
    entity_types: BTreeSet<String>,
    /// Every model file of the mod's namespaces, by id.
    models: BTreeMap<String, Model>,
    findings: Findings,
}

/// A file's text and value as the game's readers take it: without a byte-order mark, and, when
/// that does not parse, without comments.
fn parse(bytes: &[u8]) -> Result<(String, Value), serde_json::Error> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    match serde_json::from_str(text) {
        Ok(value) => Ok((text.to_owned(), value)),
        Err(error) => {
            let plain =
                String::from_utf8_lossy(&super::without_comments(text.as_bytes())).into_owned();
            serde_json::from_str(&plain)
                .map(|value| (plain, value))
                .map_err(|_| error)
        }
    }
}

/// Whether the file's own `neoforge:conditions` may disable it: the tool does not know which mods
/// and features the player has, so a file that carries them is not read.
fn conditional(value: &Value) -> bool {
    value.get("neoforge:conditions").is_some()
}

impl References<'_> {
    fn parsed(&mut self, name: &str) -> Result<Result<(String, Value), serde_json::Error>, String> {
        Ok(parse(&self.reader.read(name)?))
    }

    /// The text and value of a JSON file; an error finding when it is not JSON at all.
    fn json(&mut self, name: &str) -> Result<Option<(String, Value)>, String> {
        match self.parsed(name)? {
            Ok(json) => Ok(Some(json)),
            Err(e) => {
                self.findings.errors.push(format!(
                    "{name}: not valid JSON, so the game drops it: {e}."
                ));
                Ok(None)
            }
        }
    }

    /// A data file the load conditions do not disable (the tool cannot tell, so any file with
    /// `neoforge:conditions` is left alone).
    fn data(&mut self, name: &str) -> Result<Option<Value>, String> {
        Ok(self
            .json(name)?
            .map(|(_, value)| value)
            .filter(|value| !conditional(value)))
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

    /// What else could supply an asset the mod's files lack.
    fn supplier(&self, texture: bool) -> Option<String> {
        let mut others = Vec::new();
        if self.jar {
            others.push("a [[jar]] layer or a dependency's jar");
        }
        if texture && self.atlas {
            others.push("an atlas source (it renames a sprite or reads a directory)");
        }
        (!others.is_empty()).then(|| others.join(" or "))
    }

    /// `id` as a file of this mod, `assets/<ns>/<folder>/<path><extension>`: an error when the
    /// file is missing and nothing else could supply it, a warning when something could.
    fn missing(&mut self, name: &str, id: &str, folder: &str, extension: &str, what: &str) {
        let Some((namespace, path)) = self.own(id) else {
            return;
        };
        let file = format!("assets/{namespace}/{folder}/{path}{extension}");
        if self.has(&file) {
            return;
        }
        let near = near_file(self, folder, extension, id);
        match self.supplier(folder == "textures") {
            None => self.findings.errors.push(format!(
                "{name}: the {what} {id} is not a file of this mod (expected {file}). Add the file or fix the id{near}."
            )),
            Some(other) => self.findings.warnings.push(format!(
                "{name}: could not find the {what} {id} in this mod (expected {file}); if {other} supplies it, ignore this. Otherwise add the file or fix the id{near}."
            )),
        }
    }

    fn blockstate(&mut self, name: &str) -> Result<(), String> {
        let Some((text, _)) = self.json(name)? else {
            return Ok(());
        };
        // A file the decoder refuses, and a definition of a mod's own type, name no model here.
        let Ok(BlockStateFile::Vanilla(definition)) = BlockStateFile::from_json(&text) else {
            return Ok(());
        };
        let mut sets: Vec<&VariantSet> = Vec::new();
        sets.extend(definition.variants.iter().flatten().map(|(_, set)| set));
        sets.extend(definition.multipart.iter().flatten().map(|s| &s.variant));
        for set in sets {
            let models: Vec<&str> = match set {
                VariantSet::Single(variant) => vec![variant.model.as_str()],
                VariantSet::Weighted(list) => list.iter().map(|(v, _)| v.model.as_str()).collect(),
            };
            for model in models {
                self.missing(name, model, "models", ".json", "model");
            }
        }
        Ok(())
    }

    /// A model file's parent and sprites; [`References::check_models`] checks them once every model is
    /// known, so a slot a child model overrides is not asked of its parent.
    fn read_model(&mut self, name: &str, namespace: &str, path: &str) -> Result<(), String> {
        let Some((text, _)) = self.json(name)? else {
            return Ok(());
        };
        let Ok(model) = CuboidModel::from_json(&text) else {
            return Ok(());
        };
        let textures = model
            .textures
            .iter()
            .filter_map(|(slot, texture)| match texture {
                TextureSlot::Material(material) => Some((slot.clone(), material.sprite.clone())),
                TextureSlot::Reference(_) => None,
            })
            .collect();
        self.models.insert(
            format!("{namespace}:{path}"),
            Model {
                file: name.to_owned(),
                parent: model.parent.clone(),
                textures,
                slots: model.textures.keys().cloned().collect(),
            },
        );
        // Composite children bring parents of their own.
        for parent in model.referenced_models() {
            self.missing(name, &parent, "models", ".json", "parent model");
        }
        Ok(())
    }

    /// The sprites of each model, except a slot that a model built on it sets itself: that
    /// model's own sprite is the one drawn.
    fn check_models(&mut self) {
        let overridden = |id: &str, slot: &str| {
            self.models.iter().any(|(other, model)| {
                let mut at = model.parent.as_deref();
                let mut depth = 0;
                while let Some(parent) = at
                    && depth < 64
                {
                    if parent == id && model.slots.contains(slot) && other != id {
                        return true;
                    }
                    at = self.models.get(parent).and_then(|m| m.parent.as_deref());
                    depth += 1;
                }
                false
            })
        };
        let mut missing = Vec::new();
        for (id, model) in &self.models {
            for (slot, sprite) in &model.textures {
                if !overridden(id, slot) {
                    missing.push((model.file.clone(), sprite.clone()));
                }
            }
        }
        for (file, sprite) in missing {
            self.missing(&file, &sprite, "textures", ".png", "texture");
        }
    }

    fn item_definition(&mut self, name: &str) -> Result<(), String> {
        let Some((text, _)) = self.json(name)? else {
            return Ok(());
        };
        let Ok(item) = ClientItem::from_json(&text) else {
            return Ok(());
        };
        let mut models = Vec::new();
        item.model.dependencies(&mut models);
        for model in models {
            self.missing(name, &model, "models", ".json", "model");
        }
        Ok(())
    }

    fn recipe(&mut self, name: &str) -> Result<(), String> {
        let Some(recipe) = self.data(name)? else {
            return Ok(());
        };
        let mut ids = Vec::new();
        recipe_ids(&recipe, &mut ids);
        for id in ids {
            self.id_in_registry(name, &id, "item", "the ingredient or result");
        }
        Ok(())
    }

    fn loot_table(&mut self, name: &str) -> Result<(), String> {
        let Some(table) = self.data(name)? else {
            return Ok(());
        };
        let mut found = Vec::new();
        loot_table_ids(&table, &mut found);
        for (registry, id) in found {
            self.id_in_registry(name, &id, registry, "the loot entry");
        }
        Ok(())
    }

    /// A tag file, `tag` being its path under `tags/`: the registry (one folder, or two under
    /// `worldgen/`) and the tag's own path.
    fn tag(&mut self, name: &str, tag: &str) -> Result<(), String> {
        let split = match tag.strip_prefix("worldgen/") {
            Some(rest) => rest
                .split_once('/')
                .map(|(kind, path)| (format!("worldgen/{kind}"), path)),
            None => tag
                .split_once('/')
                .map(|(registry, path)| (registry.to_owned(), path)),
        };
        let Some((registry, _)) = split else {
            return Ok(());
        };
        let Some(value) = self.data(name)? else {
            return Ok(());
        };
        let Some(Value::Array(values)) = value.get("values") else {
            return Ok(());
        };
        for entry in values {
            let id = match entry {
                Value::String(id) => id,
                // `{"id": ..., "required": false}` is allowed to name nothing, and
                // `{"id": ..., "remove": true}` takes an entry out.
                Value::Object(object)
                    if object.get("required").and_then(Value::as_bool) != Some(false)
                        && object.get("remove").and_then(Value::as_bool) != Some(true) =>
                {
                    match object.get("id") {
                        Some(Value::String(id)) => id,
                        _ => continue,
                    }
                }
                _ => continue,
            };
            if let Some(tag) = id.strip_prefix('#') {
                self.tag_reference(name, id, tag, &registry);
            } else {
                self.id_in_registry(name, id, &registry, "the tag value");
            }
        }
        Ok(())
    }

    fn tag_reference(&mut self, name: &str, id: &str, tag: &str, registry: &str) {
        let Some((namespace, path)) = self.own(tag) else {
            return;
        };
        let file = format!("data/{namespace}/tags/{registry}/{path}.json");
        if self.has(&file) {
            return;
        }
        let other = match (self.jar, self.code) {
            (true, true) => Some("a [[jar]] layer, a dependency's jar or the mod's code"),
            (true, false) => Some("a [[jar]] layer or a dependency's jar"),
            (false, true) => Some("the mod's code"),
            (false, false) => None,
        };
        match other {
            None => self.findings.errors.push(format!(
                "{name}: the tag {id} is not a tag file of this mod (expected {file}), so the tag does not load. Add the file or fix the id."
            )),
            Some(other) => self.findings.warnings.push(format!(
                "{name}: could not find the tag {id} in this mod (expected {file}); if {other} supplies it, ignore this. Otherwise add the file or fix the id."
            )),
        }
    }

    /// `id` of a vanilla-shaped registry the mod declares into, when it is in the mod's namespace
    /// and the mod does not declare it: an error, or a warning when a `[[jar]]` layer could.
    /// A mod with code is not asked: its code declares ids no file shows.
    fn id_in_registry(&mut self, name: &str, id: &str, registry: &str, what: &str) {
        if self.code || self.own(id).is_none() {
            return;
        }
        let known: Vec<&str> = match registry {
            "block" => &self.blocks,
            "item" => &self.items,
            "fluid" => &self.fluids,
            "entity_type" => &self.entity_types,
            _ => return,
        }
        .iter()
        .map(String::as_str)
        .collect();
        if known.contains(&id) {
            return;
        }
        let article = if registry == "item" || registry == "entity_type" {
            "an"
        } else {
            "a"
        };
        let kind = registry.replace('_', " ");
        let near = nearest(id, &known)
            .map(|n| format!(" Did you mean {n}?"))
            .unwrap_or_default();
        let listed = self.listed(&known);
        if self.jar {
            self.findings.warnings.push(format!(
                "{name}: could not find {what} {id} among the {kind}s this mod declares ({listed}); if a [[jar]] layer or a dependency's jar supplies it, ignore this.{near} Otherwise fix the id."
            ));
        } else {
            self.findings.errors.push(format!(
                "{name}: {what} {id} is not {article} {kind} this mod declares ({listed}).{near} Fix the id."
            ));
        }
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
            let state = format!("assets/{namespace}/blockstates/{path}.json");
            // Only a block drawn from a model needs a blockstate. One that declares no render
            // shape takes its template's, which the tool cannot see: a chest or a sign is drawn
            // by its block entity.
            let shape = block.properties.render_shape;
            if shape != Some(RenderShape::Invisible)
                && shape != Some(RenderShape::EntityblockAnimated)
                && !self.has(&state)
            {
                match (shape, self.supplier(false)) {
                    (Some(_), None) => self.findings.errors.push(format!(
                        "{file}: the block {} has no model: expected {state}, which names its models. Add it, or declare render_shape INVISIBLE.",
                        block.id
                    )),
                    (shape, other) => {
                        let mut maybe = Vec::new();
                        if shape.is_none() {
                            maybe.push("its template draws it without a block model (a chest, a sign)".to_owned());
                        }
                        maybe.extend(other.map(|o| format!("{o} supplies it")));
                        self.findings.warnings.push(format!(
                            "{file}: could not find a blockstate for the block {} in this mod (expected {state}); if {}, ignore this. Otherwise add it, or declare render_shape INVISIBLE.",
                            block.id,
                            maybe.join(" or ")
                        ));
                    }
                }
            }
            if block.item {
                self.item_definition_exists(&file, &block.id, namespace, path);
            }
            let loot = format!("data/{namespace}/loot_table/blocks/{path}.json");
            if !self.jar && !self.has(&loot) {
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
        if !self.jar && !self.has(&definition) {
            self.findings.warnings.push(format!(
                "{file}: the item {id} has no item definition, so it is drawn as the missing model. Add {definition}."
            ));
        }
    }

    /// Each `(language key, declaring file)` has a name in its namespace's `en_us.json`.
    fn names(&mut self, wanted: Vec<(String, String)>) -> Result<(), String> {
        // `None`: no file. `Some(None)`: a file the game's lenient reader may take and this
        // one does not; no key is asked of it.
        let mut by_namespace: BTreeMap<String, Option<Option<Value>>> = Default::default();
        for (key, file) in wanted {
            let namespace = file.split('/').nth(1).unwrap_or_default().to_owned();
            let lang = format!("assets/{namespace}/lang/en_us.json");
            if !by_namespace.contains_key(&namespace) {
                let value = if self.has(&lang) {
                    Some(self.parsed(&lang)?.ok().map(|(_, value)| value))
                } else {
                    None
                };
                by_namespace.insert(namespace.clone(), value);
            }
            let why = match &by_namespace[&namespace] {
                Some(Some(lang)) if lang.get(&key).is_some() => continue,
                Some(None) => continue,
                Some(Some(_)) => format!("{lang} has no {key:?}"),
                None => format!("there is no {lang}"),
            };
            if !self.jar {
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

/// The item ids of a recipe, read as the recipe kinds the game knows read them: each kind's
/// ingredient fields and its `result`. A recipe of another kind (a mod's own, with fields only it
/// reads) and the data beside the ids (`components`, `group`, `category`) name nothing.
fn recipe_ids(recipe: &Value, out: &mut Vec<String>) {
    let Some(kind) = recipe.get("type").and_then(Value::as_str) else {
        return;
    };
    let (ingredients, keyed, result): (&[&str], bool, bool) =
        match kind.strip_prefix("minecraft:").unwrap_or(kind) {
            "crafting_shaped" => (&[], true, true),
            "crafting_shapeless" => (&["ingredients"], false, true),
            "smelting" | "blasting" | "smoking" | "campfire_cooking" | "stonecutting" => {
                (&["ingredient"], false, true)
            }
            "smithing_transform" => (&["template", "base", "addition"], false, true),
            "smithing_trim" => (&["template", "base", "addition"], false, false),
            "crafting_transmute" => (&["input", "material"], false, true),
            _ => return,
        };
    for field in ingredients {
        if let Some(ingredient) = recipe.get(field) {
            ingredient_ids(ingredient, out);
        }
    }
    if keyed && let Some(Value::Object(key)) = recipe.get("key") {
        key.values()
            .for_each(|ingredient| ingredient_ids(ingredient, out));
    }
    if result {
        match recipe.get("result") {
            Some(Value::String(id)) => out.push(id.clone()),
            Some(stack) => out.extend(stack.get("id").and_then(Value::as_str).map(str::to_owned)),
            None => {}
        }
    }
}

/// The item ids an ingredient names: an id, a list of ingredients, or an ingredient type's own
/// fields (`neoforge:compound`, `difference`, `components`...). A `#tag` and a type only a mod
/// reads name no id.
fn ingredient_ids(ingredient: &Value, out: &mut Vec<String>) {
    match ingredient {
        Value::String(id) if !id.starts_with('#') => out.push(id.clone()),
        Value::Array(list) => list.iter().for_each(|i| ingredient_ids(i, out)),
        Value::Object(object) => {
            let kind = object
                .get("type")
                .or_else(|| object.get("neoforge:ingredient_type"))
                .and_then(Value::as_str);
            let fields: &[&str] = match kind {
                Some("neoforge:compound" | "neoforge:intersection") => &["children", "ingredients"],
                Some("neoforge:difference") => &["base", "subtracted"],
                Some("neoforge:custom_display") => &["base"],
                Some("neoforge:components") => &["items"],
                // A brewing slot's item predicate wraps its ingredient.
                None => &["item"],
                Some(_) => &[],
            };
            for field in fields {
                if let Some(part) = object.get(*field) {
                    ingredient_ids(part, out);
                }
            }
        }
        _ => {}
    }
}

/// The `(registry, id)` a loot table names: an item entry's `name`, a state condition's `block`,
/// through the table's pools (a pool the load conditions may disable is left out), their entries
/// and their conditions. A function's work (`set_components`, `set_custom_data`) names no id.
fn loot_table_ids(table: &Value, out: &mut Vec<(&'static str, String)>) {
    list(table.get("functions"), |function| {
        conditions(function.get("conditions"), out);
    });
    list(table.get("pools"), |pool| {
        if conditional(pool) {
            return;
        }
        conditions(pool.get("conditions"), out);
        list(pool.get("functions"), |f| {
            conditions(f.get("conditions"), out)
        });
        entries(pool.get("entries"), out);
    });
}

/// `each` of a list, or of the one value a field holds in its place.
fn list<'v>(value: Option<&'v Value>, mut each: impl FnMut(&'v Value)) {
    match value {
        Some(Value::Array(values)) => values.iter().for_each(each),
        Some(value @ Value::Object(_)) => each(value),
        _ => {}
    }
}

fn entries(value: Option<&Value>, out: &mut Vec<(&'static str, String)>) {
    list(value, |entry| {
        conditions(entry.get("conditions"), out);
        list(entry.get("functions"), |f| {
            conditions(f.get("conditions"), out)
        });
        let kind = type_of(entry).map(|t| t.strip_prefix("minecraft:").unwrap_or(t));
        match (kind, entry.get("name").and_then(Value::as_str)) {
            (Some("item"), Some(name)) => out.push(("item", name.to_owned())),
            (Some("alternatives" | "group" | "sequence"), _) => {
                entries(entry.get("children"), out);
            }
            _ => {}
        }
    });
}

fn conditions(value: Option<&Value>, out: &mut Vec<(&'static str, String)>) {
    list(value, |condition| {
        let kind = type_of(condition).map(|t| t.strip_prefix("minecraft:").unwrap_or(t));
        match kind {
            Some("block_state_property") => {
                if let Some(block) = condition.get("block").and_then(Value::as_str) {
                    out.push(("block", block.to_owned()));
                }
            }
            Some("inverted") => conditions(condition.get("term"), out),
            Some("all_of" | "any_of") => conditions(condition.get("terms"), out),
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Package;

    const MANIFEST: &str = "id = \"gb\"\nname = \"gb\"\nversion = \"0.1.0\"\napi = \"0.2\"\nside = \"both\"\ncapabilities = [\"registries\"]\n";
    fn jar() -> String {
        format!(
            "[[jar]]\nid = \"original\"\nversion = \"1\"\nsha512 = \"{}\"\n",
            "1".repeat(128)
        )
    }
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

    /// No error and no warning.
    fn clean(manifest: &str, changes: &[(&str, &str)]) {
        assert_eq!(findings(manifest, changes), Findings::default());
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
    fn a_block_without_a_blockstate_has_no_model_unless_it_draws_none() {
        // Declaring MODEL says the block is drawn from one: nothing else could supply it.
        let model = r#"{"template":"minecraft:stone","properties":{"render_shape":"MODEL"}}"#;
        let e = errors(&[
            ("assets/gb/blockstates/slab.json", ""),
            ("data/gb/sunrise/block/slab.json", model),
        ]);
        assert!(e[0].contains("the block gb:slab has no model"), "{e:?}");
        // The template's own shape is not known: a warning.
        let f = findings(MANIFEST, &[("assets/gb/blockstates/slab.json", "")]);
        assert!(f.errors.is_empty(), "{f:?}");
        assert_eq!(f.warnings.len(), 1, "{f:?}");
        assert!(f.warnings[0].contains("could not find a blockstate for the block gb:slab"));
    }

    #[test]
    fn a_block_drawn_by_nothing_or_by_its_block_entity_needs_no_blockstate() {
        for shape in ["INVISIBLE", "ENTITYBLOCK_ANIMATED"] {
            let block = format!(
                r#"{{"template":"minecraft:stone","properties":{{"render_shape":"{shape}"}}}}"#
            );
            clean(
                MANIFEST,
                &[
                    ("assets/gb/blockstates/slab.json", ""),
                    ("data/gb/sunrise/block/slab.json", &block),
                ],
            );
        }
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
        // Through a group's children, and a state condition's block.
        let nested = r#"{"pools":[{"entries":[{"type":"minecraft:alternatives","children":[{"type":"minecraft:item","name":"gb:slab","conditions":[{"type":"minecraft:block_state_property","block":"gb:slabs"}]}]}]}]}"#;
        let e = errors(&[("data/gb/loot_table/blocks/slab.json", nested)]);
        assert!(e[0].contains("gb:slabs is not a block"), "{e:?}");
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

    // One test per finding of the review of #2893: a valid mod the first version failed.

    /// 1. An optional entry may name nothing, and a removal names what it takes out.
    #[test]
    fn optional_and_removed_tag_entries_name_nothing() {
        let tag = r##"{"values":["gb:shard",{"id":"gb:later","required":false},{"id":"#gb:nothing","required":false},{"id":"gb:gone","remove":true}],"remove":["gb:gone","#gb:gone"]}"##;
        clean(MANIFEST, &[("data/gb/tags/item/things.json", tag)]);
        // A required entry still must exist.
        let required = r#"{"values":[{"id":"gb:later","required":true}]}"#;
        let e = errors(&[("data/gb/tags/item/things.json", required)]);
        assert!(e[0].contains("gb:later is not an item"), "{e:?}");
    }

    /// 2. A tag's registry can be two folders, as `worldgen/biome`.
    #[test]
    fn a_worldgen_tag_is_found_under_its_registry_path() {
        let cold = r##"{"values":["#gb:warm","minecraft:plains"]}"##;
        clean(
            MANIFEST,
            &[
                ("data/gb/tags/worldgen/biome/cold.json", cold),
                ("data/gb/tags/worldgen/biome/warm.json", r#"{"values":[]}"#),
            ],
        );
        let e = errors(&[("data/gb/tags/worldgen/biome/cold.json", cold)]);
        assert!(
            e[0].contains("expected data/gb/tags/worldgen/biome/warm.json"),
            "{e:?}"
        );
    }

    /// 3. A `[[jar]]` layer may supply what the files lack: a warning that says so.
    #[test]
    fn a_jar_layer_may_supply_what_the_files_lack() {
        let manifest = format!("{MANIFEST}{}", jar());
        let tag = r##"{"values":["#gb:from_jar","gb:from_jar"]}"##;
        let f = findings(
            &manifest,
            &[
                ("data/gb/tags/item/things.json", tag),
                ("assets/gb/textures/block/stone.png", ""),
                ("assets/gb/blockstates/slab.json", ""),
            ],
        );
        assert!(f.errors.is_empty(), "{f:?}");
        assert_eq!(f.warnings.len(), 4, "{f:?}");
        assert!(f.warnings.iter().all(|w| w.contains("[[jar]] layer")));
        assert!(
            f.warnings
                .iter()
                .any(|w| w.contains("could not find the tag #gb:from_jar"))
        );
        // A declared dependency's jar can do the same; the tool does not fetch it.
        let dependent = format!("{MANIFEST}[dependencies]\nchem_lib = \"1\"\n");
        let f = findings(&dependent, &[("data/gb/tags/item/things.json", tag)]);
        assert!(f.errors.is_empty(), "{f:?}");
        assert_eq!(f.warnings.len(), 2, "{f:?}");
        // The same mod without a jar breaks.
        let e = errors(&[("data/gb/tags/item/things.json", tag)]);
        assert_eq!(e.len(), 2, "{e:?}");
    }

    /// 4. A world fluid's flowing form is a fluid, its bucket an item, its block a block.
    #[test]
    fn a_world_fluid_brings_its_generated_ids() {
        clean(
            MANIFEST,
            &[
                ("data/gb/sunrise/fluid/oil.json", r#"{"world":{}}"#),
                (
                    "assets/gb/lang/en_us.json",
                    r#"{"block.gb.slab":"Slab","item.gb.shard":"Shard"}"#,
                ),
                (
                    "data/minecraft/tags/fluid/water.json",
                    r#"{"values":["gb:oil","gb:flowing_oil"]}"#,
                ),
                (
                    "data/minecraft/tags/block/mineable/shovel.json",
                    r#"{"values":["gb:oil"]}"#,
                ),
                (
                    "data/gb/recipe/bucket.json",
                    r#"{"type":"minecraft:stonecutting","ingredient":"gb:oil_bucket","result":{"id":"minecraft:bucket","count":1}}"#,
                ),
            ],
        );
        let e = errors(&[
            ("data/gb/sunrise/fluid/oil.json", r#"{"world":{}}"#),
            (
                "data/minecraft/tags/fluid/water.json",
                r#"{"values":["gb:flowing_gas"]}"#,
            ),
        ]);
        assert!(e[0].contains("gb:flowing_gas is not a fluid"), "{e:?}");
    }
    /// 5. An atlas source may rename a sprite or take a directory; a child model sets its own sprite.
    #[test]
    fn atlas_sources_and_model_overrides_may_supply_a_texture() {
        let atlas =
            r#"{"sources":[{"type":"minecraft:directory","source":"stone","prefix":"block/"}]}"#;
        let f = findings(
            MANIFEST,
            &[
                ("assets/gb/atlases/blocks.json", atlas),
                ("assets/gb/textures/block/stone.png", ""),
            ],
        );
        assert!(f.errors.is_empty(), "{f:?}");
        assert_eq!(f.warnings.len(), 1, "{f:?}");
        assert!(f.warnings[0].contains("could not find the texture gb:block/stone"));
        assert!(f.warnings[0].contains("atlas source"));
        clean(
            MANIFEST,
            &[
                (
                    "assets/gb/models/block/base.json",
                    r#"{"textures":{"all":"gb:block/placeholder"}}"#,
                ),
                (
                    "assets/gb/models/block/slab.json",
                    r#"{"parent":"gb:block/base","textures":{"all":"gb:block/stone"}}"#,
                ),
            ],
        );
        // A slab model that leaves the slot alone still needs the parent's sprite.
        let e = errors(&[
            (
                "assets/gb/models/block/base.json",
                r#"{"textures":{"all":"gb:block/placeholder"}}"#,
            ),
            (
                "assets/gb/models/block/slab.json",
                r#"{"parent":"gb:block/base"}"#,
            ),
        ]);
        assert!(e[0].contains("the texture gb:block/placeholder"), "{e:?}");
    }

    /// 7. Payloads only a mod or the stack reads name no ids.
    #[test]
    fn opaque_payloads_are_not_scanned_for_ids() {
        clean(
            MANIFEST,
            &[
                (
                    "data/gb/recipe/machine.json",
                    r#"{"type":"gb:machine","extra":{"input":"gb:nope","items":["gb:nope"]},"result":{"id":"gb:slab"}}"#,
                ),
                (
                    "data/gb/recipe/slab.json",
                    r#"{"type":"minecraft:crafting_shapeless","ingredients":["gb:shard",{"type":"neoforge:components","items":"gb:shard","components":{"minecraft:custom_data":{"id":"gb:nope"}}}],"result":{"id":"gb:slab","components":{"minecraft:custom_data":{"item":"gb:nope","id":"gb:nope"}}}}"#,
                ),
                (
                    "data/gb/loot_table/blocks/slab.json",
                    r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"gb:slab","functions":[{"function":"minecraft:set_custom_data","tag":{"type":"minecraft:item","name":"gb:nope","block":"gb:nope"}}]}]}]}"#,
                ),
                (
                    "assets/gb/blockstates/slab.json",
                    r#"{"neoforge:definition_type":"gb:custom","model":"gb:block/gone","config":{"model":"gb:block/gone"}}"#,
                ),
                (
                    "assets/gb/items/slab.json",
                    r#"{"model":{"type":"gb:custom","model":"gb:item/gone"}}"#,
                ),
            ],
        );
    }

    /// 8. A file the load conditions may disable is not read.
    #[test]
    fn files_the_load_conditions_may_disable_are_not_read() {
        let when = r#""neoforge:conditions":[{"type":"neoforge:mod_loaded","modid":"other"}]"#;
        clean(
            MANIFEST,
            &[
                (
                    "data/gb/recipe/other.json",
                    &format!(
                        r#"{{{when},"type":"minecraft:smelting","ingredient":"gb:nope","result":{{"id":"minecraft:stick"}}}}"#
                    ),
                ),
                (
                    "data/gb/tags/item/other.json",
                    &format!(r##"{{{when},"values":["gb:nope","#gb:nope"]}}"##),
                ),
                (
                    "data/gb/loot_table/blocks/slab.json",
                    &format!(
                        r#"{{"pools":[{{{when},"entries":[{{"type":"minecraft:item","name":"gb:nope"}}]}},{{"entries":[{{"type":"minecraft:item","name":"gb:slab"}}]}}]}}"#
                    ),
                ),
            ],
        );
    }

    /// 9. The game reads a byte-order mark and comments.
    #[test]
    fn a_byte_order_mark_and_comments_are_read_as_the_game_reads_them() {
        let recipe = "\u{feff}{\n// a note\n\"type\":\"minecraft:crafting_shaped\", /* the key */\n\"pattern\":[\"SS\"],\"key\":{\"S\":\"gb:shard\"},\"result\":{\"id\":\"gb:slab\"}}";
        clean(MANIFEST, &[("data/gb/recipe/slab.json", recipe)]);
        let model = "\u{feff}{\"parent\":\"minecraft:block/cube_all\",\"textures\":{\"all\":\"gb:block/stone\"}}\n// trailing";
        clean(MANIFEST, &[("assets/gb/models/block/slab.json", model)]);
        // A comment hides nothing: the id after it is still checked.
        let e = errors(&[(
            "data/gb/recipe/slab.json",
            "{\"type\":\"minecraft:smelting\",\"ingredient\":\"gb:nope\", // note\n\"result\":{\"id\":\"gb:slab\"}}",
        )]);
        assert!(e[0].contains("gb:nope is not an item"), "{e:?}");
    }
}
