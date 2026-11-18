//! `cargo sunrise-mod publish`: the files of a registry submission, written beside the built package
//! (docs/modding/src/how-to/publish.md). Never uploads, never opens a pull request, never uses the network.
//!
//! The output folder is a registry checkout in miniature: `mods/<id>.toml` (the entry, serialised through
//! the registry's own [`Entry`]), `icons/<id>.png`, `files/<sha256>` (where `registry check --files` and the
//! registry's `fetch_files.py` keep a file), the package under its release name, and `PULL_REQUEST.md`.
use std::path::{Path, PathBuf};
use sunrise_mod_package::{
    Package,
    icon::MAX_ICON,
    index::{
        Kind,
        page::{self, ICON_SIDE},
        registry::{ENTRIES, Entry, EntryVersion, TODO},
    },
};

const USAGE: &str = "cargo sunrise-mod publish [--out <dir>] [--registry <checkout>] [--summary <text>] [--owner <login>]… [--categories a,b] [--tags a,b] [--changelog <text>] [--key <public key> --signature <file.minisig>] [--wasm]";

#[derive(Default)]
struct Options {
    out: Option<PathBuf>,
    registry: Option<PathBuf>,
    summary: Option<String>,
    owners: Vec<String>,
    categories: Vec<String>,
    tags: Vec<String>,
    changelog: String,
    key: Option<String>,
    signature: Option<PathBuf>,
    wasm: bool,
}

fn list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--wasm" {
            o.wasm = true;
            continue;
        }
        let known = [
            "--out",
            "--registry",
            "--summary",
            "--owner",
            "--categories",
            "--tags",
            "--changelog",
            "--key",
            "--signature",
        ];
        if !known.contains(&arg.as_str()) {
            return Err(format!(
                "publish does not take {arg:?}: see the options in `{USAGE}`"
            ));
        }
        let value = it
            .next()
            .ok_or_else(|| format!("{arg} needs a value: `{USAGE}`"))?;
        match arg.as_str() {
            "--out" => o.out = Some(value.into()),
            "--registry" => o.registry = Some(value.into()),
            "--summary" => o.summary = Some(value.clone()),
            "--owner" => o.owners.push(value.clone()),
            "--categories" => o.categories = list(value),
            "--tags" => o.tags = list(value),
            "--changelog" => o.changelog = value.clone(),
            "--key" => o.key = Some(value.clone()),
            _ => o.signature = Some(value.into()),
        }
    }
    Ok(o)
}

/// The icon at the documented place (`icon.png` beside `sunrise-mod.toml`) against the registry's limits,
/// as a sentence of what was found, why it is refused and what to do.
fn icon_problem(len: u64, bytes: Option<&[u8]>) -> Option<String> {
    if len > MAX_ICON as u64 {
        return Some(format!(
            "icon.png is {} KiB, and the registry keeps icons of at most {} KiB: shrink it (fewer colours or a smaller size) and run publish again",
            len.div_ceil(1024),
            MAX_ICON >> 10
        ));
    }
    page::check_icon(bytes?).err().map(|e| {
        format!(
            "icon.png is refused by the registry ({e}): save it as a square PNG, {} to {} pixels a side, and run publish again",
            ICON_SIDE.start(),
            ICON_SIDE.end()
        )
    })
}

/// The version row's file name in the release, and the name a maintainer stages it under
/// (`<sha256>.<extension>`, which `scripts/fetch_files.py` of the registry looks for).
fn release_name(id: &str, version: &str, extension: &str) -> String {
    format!("{id}-{version}.{extension}")
}

fn staged_name(sha256: &str, file: &str) -> String {
    let extension = file.rsplit_once('.').map_or("", |(_, e)| e);
    format!("{sha256}.{extension}")
}

/// `YYYY-MM-DD` of a day count since 1970-01-01.
fn date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    date(i64::try_from(secs / 86_400).unwrap_or(0))
}

fn todo(what: &str) -> String {
    format!("{TODO}: {what}")
}

/// The new entry for a project's first version, filled from its manifest; what the manifest cannot say
/// is the options' or a [`TODO`] value that the registry's checks refuse.
fn draft(
    m: &sunrise_mod_package::Manifest,
    kind: Kind,
    o: &Options,
    icon: bool,
    version: EntryVersion,
) -> Entry {
    let or_todo = |value: &str, what: &str| {
        if value.trim().is_empty() {
            todo(what)
        } else {
            value.trim().to_owned()
        }
    };
    Entry {
        id: m.id.clone(),
        kind,
        repo: None,
        name: m.name.clone(),
        summary: o
            .summary
            .clone()
            .unwrap_or_else(|| todo("one line, at most 160 characters (--summary)")),
        description: or_todo(
            &m.description,
            "50 to 10,000 characters: set `description` in sunrise-mod.toml",
        ),
        authors: m.authors.clone(),
        owners: if o.owners.is_empty() {
            vec![todo("your GitHub login (--owner)")]
        } else {
            o.owners.clone()
        },
        owner_ids: Vec::new(),
        license: or_todo(
            &m.license,
            "an SPDX licence: set `license` in sunrise-mod.toml",
        ),
        source: or_todo(
            m.homepage.as_deref().unwrap_or(""),
            "the project's repository: set `homepage` in sunrise-mod.toml",
        ),
        icon: icon.then(|| format!("icons/{}.png", m.id)),
        categories: if o.categories.is_empty() {
            vec![todo("1 to 3 of the kind's categories (--categories)")]
        } else {
            o.categories.clone()
        },
        tags: o.tags.clone(),
        screenshots: Vec::new(),
        key: o.key.clone(),
        versions: vec![version],
    }
}

/// `existing` with `version` added at the top; published versions never change, so a version
/// that is already listed is refused.
fn add_version(
    mut existing: Entry,
    version: EntryVersion,
    key: Option<&str>,
) -> Result<Entry, String> {
    let id = &existing.id;
    if let Some(repo) = &existing.repo {
        return Err(format!(
            "{id} follows the releases of {repo}: publish a release there; the registry takes no file for it"
        ));
    }
    if existing
        .versions
        .iter()
        .any(|v| v.version == version.version)
    {
        return Err(format!(
            "{id} {} is already listed in the registry, and a published version never changes: raise `version` in sunrise-mod.toml and run publish again",
            version.version
        ));
    }
    if key.is_some() && key != existing.key.as_deref() {
        return Err(format!(
            "{id} is listed with another key: pass the key its entry names, or none if it has none"
        ));
    }
    if existing.key.is_some() && version.signature.is_none() {
        return Err(format!(
            "{id} has a key, so every version must be signed: sign the package and pass --key and --signature (docs/modding/src/how-to/publish.md#signing)"
        ));
    }
    existing.versions.insert(0, version);
    Ok(existing)
}

fn pull_request(entry: &Entry, v: &EntryVersion, size: u64, first: bool) -> (String, String) {
    let id = &entry.id;
    let title = if first {
        format!("Add {} {}", id, v.version)
    } else {
        format!("{id} {}", v.version)
    };
    let signed = if v.signature.is_some() {
        "signed (the signature is in the entry)"
    } else {
        "unsigned"
    };
    let body = format!(
        "{title}\n\n\
         - Project: `{id}` ({}), version {}, for Minecraft {}\n\
         - File: `{}`, {size} bytes, {signed}\n\
         - Artifact URL: {TODO}: paste the address of `{}` (a release asset of your repository works)\n\
         - SHA-256: `{}`\n\n\
         The entry and the icon are in this pull request; the file's facts (API, side, capabilities) are read from the file.\n\n\
         For the maintainer: copy the file from the artifact URL to this repository's `staging` release as `{}`, check that its SHA-256 is the one above, and let the `check` workflow run. After the merge, `publish` releases it as `{id}-{}`.\n",
        entry.kind.name(),
        v.version,
        v.game,
        v.file,
        v.file,
        v.sha256,
        staged_name(&v.sha256, &v.file),
        v.version,
    );
    (title, body)
}

pub fn run(entry_point: mod_surface::child::ChildEntry, args: &[String]) -> Result<(), String> {
    let o = parse(args)?;
    let root = Path::new(".");
    let icon_path = root.join("icon.png");
    let icon = match std::fs::metadata(&icon_path) {
        Ok(meta) => {
            let bytes = if meta.len() <= MAX_ICON as u64 {
                Some(std::fs::read(&icon_path).map_err(|e| format!("icon.png: {e}"))?)
            } else {
                None
            };
            if let Some(problem) = icon_problem(meta.len(), bytes.as_deref()) {
                return Err(problem);
            }
            bytes
        }
        Err(_) => None,
    };
    let build_args: Vec<String> = if o.wasm {
        vec!["--wasm".into()]
    } else {
        Vec::new()
    };
    let package = crate::build_command(entry_point, &build_args)?;
    let manifest = Package::read(root)?.manifest;
    let kind = crate::check::kind_of(&package).ok_or("the built package is not a .smod")?;
    let signature = o
        .signature
        .as_ref()
        .map(|p| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display())))
        .transpose()?;
    let report = crate::check::check(
        &package,
        kind.clone(),
        o.key.as_deref(),
        signature.as_deref(),
    )
    .map_err(|e| format!("the package {} fails its check: {e}", package.display()))?;
    let file = release_name(&manifest.id, &manifest.version, "smod");
    let version = EntryVersion {
        native_binaries: Vec::new(),
        version: manifest.version.clone(),
        published: today(),
        game: report
            .facts
            .game
            .clone()
            .unwrap_or_else(|| sunrise_mod_package::sdk::GAME.to_owned()),
        file: file.clone(),
        sha256: report.sha256.clone(),
        signature,
        changelog: o.changelog.clone(),
    };
    let existing_path = o
        .registry
        .as_ref()
        .map(|r| r.join(ENTRIES).join(format!("{}.toml", manifest.id)))
        .filter(|p| p.is_file());
    let existing = existing_path
        .as_ref()
        .map(|p| {
            let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            toml::from_str::<Entry>(&text).map_err(|e| format!("{}: {e}", p.display()))
        })
        .transpose()?;
    let first = existing.is_none();
    let entry = match existing {
        Some(entry) if entry.kind != kind => {
            return Err(format!(
                "{} is listed as a {}, and a project's kind never changes: this package is a {}",
                entry.id,
                entry.kind.name(),
                kind.name()
            ));
        }
        Some(entry) => add_version(entry, version.clone(), o.key.as_deref())?,
        None => draft(&manifest, kind, &o, icon.is_some(), version.clone()),
    };

    let out = o
        .out
        .clone()
        .unwrap_or_else(|| root.join("target/sunrise-mod/publish"));
    let write = |path: PathBuf, bytes: &[u8]| -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    };
    let package_bytes =
        std::fs::read(&package).map_err(|e| format!("{}: {e}", package.display()))?;
    write(
        out.join(ENTRIES).join(format!("{}.toml", entry.id)),
        entry.to_toml()?.as_bytes(),
    )?;
    write(out.join("files").join(&version.sha256), &package_bytes)?;
    write(out.join(&file), &package_bytes)?;
    if let (true, Some(bytes)) = (first, &icon) {
        write(out.join(format!("icons/{}.png", entry.id)), bytes)?;
    }
    let base = match &o.registry {
        Some(r) => r.clone(),
        None => {
            std::fs::create_dir_all(out.join("base").join(ENTRIES))
                .map_err(|e| format!("{}: {e}", out.display()))?;
            out.join("base")
        }
    };
    let (title, body) = pull_request(&entry, &version, report.size, first);
    write(out.join("PULL_REQUEST.md"), body.as_bytes())?;

    let mut remaining = entry.check_fields();
    remaining.extend(
        page::check_required(
            &entry.id,
            &entry.description,
            &entry.categories,
            entry.icon.as_deref(),
            &entry.screenshots,
        )
        .into_iter()
        .map(|p| format!("{}: {p}", entry.id)),
    );
    // A category that is still the TODO is already listed as such.
    remaining.retain(|r| !r.contains("is not a category") || !r.contains(TODO));
    println!(
        "{} {} ({}, {} bytes, SHA-256 {})",
        entry.id,
        version.version,
        entry.kind.name(),
        report.size,
        report.sha256
    );
    println!("Wrote in {}:", out.display());
    println!("  {ENTRIES}/{}.toml   the registry entry", entry.id);
    println!("  {file}   the package, to attach to a release of your repository");
    println!(
        "  files/{}   the same file, named as `registry check --files` reads it",
        report.sha256
    );
    if first && icon.is_some() {
        println!("  icons/{}.png   your icon.png", entry.id);
    }
    println!("  PULL_REQUEST.md   the pull request text, below");
    if remaining.is_empty() {
        println!("\nNothing is left to fill in.");
    } else {
        println!("\nStill to fill in before the pull request:");
        for r in &remaining {
            println!("  - {r}");
        }
    }
    println!(
        "\nNext, check the result as the registry's CI does:\n  cargo sunrise-mod registry check {} {} --files {}\n  cargo sunrise-mod registry build {} --files {} --out {}",
        base.display(),
        out.display(),
        out.join("files").display(),
        out.display(),
        out.join("files").display(),
        out.join("index.json").display(),
    );
    println!("\nPull request: {title}\n\n{body}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12";

    fn manifest() -> sunrise_mod_package::Manifest {
        toml::from_str(
            "id = 'glowing_blocks'\nname = 'Glowing Blocks'\nversion = '0.2.0'\napi = '0.2'\ncapabilities = ['registries']\n\
             description = 'Blocks that glow in the dark, and the items to make them.'\nauthors = ['Ada']\nlicense = 'MIT'\nhomepage = 'https://github.com/ada/glowing_blocks'\n",
        )
        .unwrap()
    }

    fn row(version: &str) -> EntryVersion {
        EntryVersion {
            native_binaries: Vec::new(),
            version: version.into(),
            published: "2026-11-18".into(),
            game: "26.3".into(),
            file: format!("glowing_blocks-{version}.smod"),
            sha256: SHA.into(),
            signature: None,
            changelog: String::new(),
        }
    }

    #[test]
    fn the_entry_is_written_from_the_manifest_and_names_what_is_left() {
        let o = Options {
            owners: vec!["ada".into()],
            categories: vec!["decoration".into()],
            ..Options::default()
        };
        let entry = draft(&manifest(), Kind::Mod, &o, true, row("0.2.0"));
        let text = entry.to_toml().unwrap();
        // The registry reads back exactly what was written.
        assert_eq!(toml::from_str::<Entry>(&text).unwrap(), entry);
        assert!(text.contains("name = \"Glowing Blocks\""), "{text}");
        assert!(text.contains("source = \"https://github.com/ada/glowing_blocks\""));
        assert!(text.contains("icon = \"icons/glowing_blocks.png\""));
        assert!(text.contains(&format!("sha256 = \"{SHA}\"")));
        assert!(text.contains("file = \"glowing_blocks-0.2.0.smod\""));
        // Only the summary was missing, and the registry's own check says so.
        let left = entry.check_fields();
        assert_eq!(left.len(), 1, "{left:?}");
        assert!(
            left[0].contains("the summary is still the TODO(publish)"),
            "{left:?}"
        );
    }

    #[test]
    fn a_version_already_listed_is_refused_by_name() {
        let o = Options::default();
        let listed = draft(&manifest(), Kind::Mod, &o, false, row("0.2.0"));
        let e = add_version(listed.clone(), row("0.2.0"), None).unwrap_err();
        assert!(e.contains("glowing_blocks 0.2.0 is already listed"), "{e}");
        let next = add_version(listed, row("0.3.0"), None).unwrap();
        let versions: Vec<_> = next.versions.iter().map(|v| v.version.as_str()).collect();
        assert_eq!(versions, ["0.3.0", "0.2.0"]);
    }

    #[test]
    fn the_staging_name_is_the_hash_and_the_extension() {
        assert_eq!(
            staged_name(SHA, "glowing_blocks-0.2.0.smod"),
            format!("{SHA}.smod")
        );
        assert_eq!(release_name("m", "1.0.0", "smod"), "m-1.0.0.smod");
    }

    #[test]
    fn an_icon_outside_the_registrys_limits_gets_a_three_part_error() {
        let png = |side: u32| {
            let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
            b.extend(side.to_be_bytes());
            b.extend(side.to_be_bytes());
            b
        };
        assert_eq!(icon_problem(1000, Some(&png(64))), None);
        let big = icon_problem(300 << 10, None).unwrap();
        assert!(
            big.contains("300 KiB") && big.contains("256 KiB") && big.contains("shrink it"),
            "{big}"
        );
        let small = icon_problem(500, Some(&png(32))).unwrap();
        assert!(
            small.contains("64 to 1024") && small.contains("run publish again"),
            "{small}"
        );
    }

    #[test]
    fn dates_count_from_1970() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(19_723), "2024-01-01");
        assert_eq!(date(19_782), "2024-02-29");
    }
}
