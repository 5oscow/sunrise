//! What a mod declares of the game's own code (`[[native.mixin]]` targets, `[[native.loop]]` loops and its `internals`), as entries of the list of what it uses: one function reads them from a build's build data, for the packer to record at the pinned build and for the fit check to compare at the installed one.
use std::path::Path;
use sunrise_mod_package::mod_surface::uses::{Change, Entry, Misfit};
use sunrise_mod_package::{Manifest, build_data, internals};
use sunrise_platform::mod_build::api_gen;

/// What every such entry's id starts with; no surface item's id does.
pub const PREFIX: &str = "game:";

/// The `[native]` table whose rows name game code no build data describes, so a mod with one works on the build it was made for only; `None` for a mod a list can judge.
pub fn unjudged(manifest: &Manifest) -> Option<&'static str> {
    let native = manifest.native.as_ref()?;
    [
        (native.module.is_empty(), "[[native.module]]"),
        (native.bridge.is_empty(), "[[native.bridge]]"),
        (native.state.is_empty(), "[[native.state]]"),
    ]
    .into_iter()
    .find_map(|(empty, table)| (!empty).then_some(table))
}

/// Whether `manifest` declares game code [`entries`] lists.
pub fn declared(manifest: &Manifest) -> bool {
    !manifest.internals.is_empty()
        || manifest
            .native
            .as_ref()
            .is_some_and(|native| !native.mixin.is_empty() || !native.r#loop.is_empty())
}

/// Each piece of game code `manifest` declares, by id, with its hash in the build whose build data is in `dir`: `None` when that build has no such item. A mixin's target is hashed by signature and body, since a replacement written for an old body could undo what the game changed; an internal by its signature; a loop by what the game publishes of it.
pub fn entries(manifest: &Manifest, dir: &Path) -> Result<Vec<(String, Option<String>)>, String> {
    let read = |file: &str| {
        let path = dir.join(file);
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
    };
    let mut entries = Vec::new();
    let native = manifest.native.as_ref();
    if let Some(rows) = native
        .map(|native| &native.mixin)
        .filter(|rows| !rows.is_empty())
    {
        let file = api_gen::functions::FILE;
        let (_, index): (_, api_gen::functions::FunctionIndex) =
            build_data::read(file, &read(file)?)?;
        for row in rows {
            let function = index.functions.get(&api_gen::functions::key(&row.target));
            entries.push((
                format!("{PREFIX}fn:{}", row.target),
                function.map(|f| format!("{}:{}", f.signature_hash, f.sha256)),
            ));
        }
    }
    if let Some(rows) = native
        .map(|native| &native.r#loop)
        .filter(|rows| !rows.is_empty())
    {
        #[derive(serde::Deserialize)]
        struct Loops {
            loops: Vec<api_gen::loops::PublishedLoop>,
        }
        let file = api_gen::loops::FILE;
        let (_, published): (_, Loops) = build_data::read(file, &read(file)?)?;
        for row in rows {
            let published = published.loops.iter().find(|l| l.name == row.name);
            entries.push((
                format!("{PREFIX}loop:{}", row.name),
                published.map(|l| format!("{}|{}|{}|{}", l.entry, l.generic, l.slot, l.side)),
            ));
        }
    }
    if !manifest.internals.is_empty() {
        let listed = internals::read(&dir.join(build_data::INTERNALS))?;
        for declared in &manifest.internals {
            let found = internals::resolve(
                &listed,
                &manifest.id,
                &declared.path,
                declared.kind.as_deref(),
            )
            .ok();
            entries.push((
                format!(
                    "{PREFIX}{}:{}",
                    declared.kind.as_deref().unwrap_or("internal"),
                    declared.path
                ),
                found.map(|entry| entry.hash.clone()),
            ));
        }
    }
    entries.sort();
    entries.dedup();
    Ok(entries)
}

/// What of `now` (a build's [`entries`]) is not as the list's `recorded` entries have it, and whether the list lacks a record of something the mod declares.
pub fn compare(recorded: &[&Entry], now: Vec<(String, Option<String>)>) -> (Vec<Misfit>, bool) {
    let mut misfits = Vec::new();
    let mut unrecorded = false;
    for (id, now) in now {
        let change = match (recorded.iter().find(|entry| entry.id == id), now) {
            (None, _) => {
                unrecorded = true;
                continue;
            }
            (Some(_), None) => Change::Removed,
            (Some(made), Some(now)) if made.hash != now => Change::Changed,
            _ => continue,
        };
        misfits.push(Misfit { id, change });
    }
    (misfits, unrecorded)
}
