//! Registry crates linked once for every mod of a build.
//!
//! Each mod is a `dylib`, and a dylib carries its own copy of every registry `rlib` it uses. rustc
//! refuses to link a crate that reaches the same rlib through two dylibs ("cannot satisfy
//! dependencies so `serde` only shows up once"), so a mod depending on two mods that each use
//! `serde` would not link. When two or more mods use registry crates, every direct registry
//! dependency of every mod goes into one more dylib, [`PACKAGE`], which every one of those mods
//! depends on: they then link those crates, and everything they pull in, from that one library
//! instead of carrying copies of their own.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

/// The shared library's folder, relative to the staging folder.
pub const RELATIVE: &str = "native-shared/registry";
/// Its Cargo package, a local workspace member like the mods.
pub const PACKAGE: &str = "sunrise-registry-shared";
/// Its crate name, which names the built library.
pub const CRATE: &str = "sunrise_registry_shared";
/// The path a mod's root manifest (`native-api/<id>/Cargo.toml`) names it by.
pub const DEPENDENCY_PATH: &str = "../../native-shared/registry";

/// One registry crate of the shared library: a package within one semver-compatible range, since
/// two incompatible versions of a package are two crates.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    package: String,
    compatible: String,
}

/// A registry dependency merged across the mods that declare it: the first requirement (cargo
/// still unifies it with each mod's own), the union of features, and default features if any mod
/// keeps them.
struct Spec {
    version: String,
    default_features: bool,
    features: BTreeSet<String>,
}

/// The direct registry dependencies of the staged mods, gathered manifest by manifest.
#[derive(Default)]
pub struct Shared {
    specs: BTreeMap<Key, Spec>,
    users: BTreeSet<String>,
}

impl Shared {
    /// Records the registry dependencies of one staged manifest of mod `id`, its root or one of
    /// its local crates, once its SDK and sibling edges are rewritten. Local paths, optional
    /// dependencies (their mod's features decide whether they are built) and target-specific
    /// tables are left to the mod.
    pub fn add(&mut self, id: &str, manifest: &toml::Table) {
        let Some(dependencies) = manifest.get("dependencies").and_then(toml::Value::as_table)
        else {
            return;
        };
        for (key, dependency) in dependencies {
            let (version, table) = match dependency {
                toml::Value::String(version) => (version.as_str(), None),
                toml::Value::Table(table) => {
                    let Some(version) = table.get("version").and_then(toml::Value::as_str) else {
                        continue;
                    };
                    (version, Some(table))
                }
                _ => continue,
            };
            let field = |name: &str| table.and_then(|table| table.get(name));
            if field("path").is_some()
                || field("optional").and_then(toml::Value::as_bool) == Some(true)
            {
                continue;
            }
            let package = field("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(key)
                .to_owned();
            let default_features = field("default-features")
                .or_else(|| field("default_features"))
                .and_then(toml::Value::as_bool)
                != Some(false);
            let spec = self
                .specs
                .entry(Key {
                    package,
                    compatible: compatible(version),
                })
                .or_insert_with(|| Spec {
                    version: version.to_owned(),
                    default_features,
                    features: BTreeSet::new(),
                });
            spec.default_features |= default_features;
            spec.features.extend(
                field("features")
                    .and_then(toml::Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(toml::Value::as_str)
                    .map(str::to_owned),
            );
            self.users.insert(id.to_owned());
        }
    }

    /// Writes the shared library under `out` when two or more mods use registry crates, and
    /// returns the mods that must depend on it (none when one mod or none uses them: nothing can
    /// then reach a registry crate twice).
    pub fn write(self, out: &Path) -> Result<BTreeSet<String>, String> {
        if self.users.len() < 2 {
            return Ok(BTreeSet::new());
        }
        let dir = out.join(RELATIVE);
        fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
        fs::write(dir.join("Cargo.toml"), self.manifest().to_string())
            .map_err(|e| e.to_string())?;
        fs::write(dir.join("src/lib.rs"), self.source()).map_err(|e| e.to_string())?;
        Ok(self.users)
    }

    fn manifest(&self) -> toml::Table {
        let mut dependencies = toml::Table::new();
        for (position, (key, spec)) in self.specs.iter().enumerate() {
            let mut dependency = toml::toml! {
                package = (key.package.clone())
                version = (spec.version.clone())
                default-features = (spec.default_features)
            };
            if !spec.features.is_empty() {
                dependency.insert(
                    "features".into(),
                    spec.features
                        .iter()
                        .cloned()
                        .map(toml::Value::String)
                        .collect::<Vec<_>>()
                        .into(),
                );
            }
            dependencies.insert(alias(position), dependency.into());
        }
        let mut manifest = toml::toml! {
            [package]
            name = PACKAGE
            version = "0.0.0"
            edition = "2024"
            [lib]
            crate-type = ["dylib"]
        };
        manifest.insert("dependencies".into(), dependencies.into());
        manifest
    }

    /// Names every crate, so the library links each one (an unnamed dependency is not linked).
    fn source(&self) -> String {
        let mut source = String::from(
            "//! Registry crates used by the mods of this build, linked once (written by the launcher).\n",
        );
        for position in 0..self.specs.len() {
            source.push_str(&format!("pub extern crate {};\n", alias(position)));
        }
        source
    }
}

/// Makes the mod whose root manifest and library root these are depend on the shared library and
/// name it, so its registry crates come from there.
pub fn link(manifest: &mut toml::Table, source: &mut String) -> Result<(), String> {
    let dependencies = manifest
        .entry("dependencies")
        .or_insert_with(|| toml::Table::new().into())
        .as_table_mut()
        .ok_or("invalid dependencies")?;
    if dependencies.contains_key(CRATE) {
        return Err(format!("dependency {CRATE} is reserved for the launcher"));
    }
    dependencies.insert(
        CRATE.into(),
        toml::toml! { path = DEPENDENCY_PATH package = PACKAGE }.into(),
    );
    source.push_str(&format!("\nextern crate {CRATE} as _;\n"));
    Ok(())
}

fn alias(position: usize) -> String {
    format!("shared_{position}")
}

/// The semver-compatible range a requirement selects in (`1`, `0.4`, `0.0.3`), cargo's unit of
/// one crate per package; the requirement itself when it does not parse (cargo then refuses it).
fn compatible(requirement: &str) -> String {
    let Some(first) = semver::VersionReq::parse(requirement)
        .ok()
        .and_then(|req| req.comparators.first().cloned())
    else {
        return requirement.to_owned();
    };
    match (first.major, first.minor, first.patch) {
        (0, Some(0), Some(patch)) => format!("0.0.{patch}"),
        (0, Some(minor), _) => format!("0.{minor}"),
        (major, ..) => major.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(text: &str) -> toml::Table {
        text.parse().unwrap()
    }

    /// Two mods using the same crates share one entry each: features unioned, default features
    /// kept when one mod keeps them, the first requirement kept.
    #[test]
    fn merges_two_mods_registry_dependencies() {
        let mut shared = Shared::default();
        shared.add(
            "left",
            &manifest(
                r#"[dependencies]
serde = { version = "1.0.100", default-features = false, features = ["alloc"] }
itoa = "1""#,
            ),
        );
        shared.add(
            "right",
            &manifest(
                r#"[dependencies]
serde = { version = "1", features = ["derive"] }
json = { package = "serde_json", version = "1" }"#,
            ),
        );
        let written = shared.manifest();
        let dependencies = written["dependencies"].as_table().unwrap();
        let by_package = |package: &str| {
            dependencies
                .values()
                .find(|dep| dep["package"].as_str() == Some(package))
                .unwrap_or_else(|| panic!("{package} shared"))
                .as_table()
                .unwrap()
                .clone()
        };
        let serde = by_package("serde");
        assert_eq!(serde["version"].as_str(), Some("1.0.100"));
        assert_eq!(serde["default-features"].as_bool(), Some(true));
        assert_eq!(
            serde["features"].as_array().unwrap(),
            &vec![toml::Value::from("alloc"), toml::Value::from("derive")]
        );
        by_package("itoa");
        by_package("serde_json");
        assert_eq!(dependencies.len(), 3);
        assert_eq!(written["lib"]["crate-type"][0].as_str(), Some("dylib"));
        let source = shared.source();
        for alias in dependencies.keys() {
            assert!(source.contains(&format!("pub extern crate {alias};")));
        }
    }

    /// Local paths, optional and target-specific dependencies stay the mod's own; a mod with only
    /// those uses no shared crate.
    #[test]
    fn leaves_local_optional_and_target_dependencies() {
        let mut shared = Shared::default();
        shared.add(
            "left",
            &manifest(
                r#"[dependencies]
right = { path = "../right", package = "mod-right" }
log = { version = "0.4", optional = true }
[target.'cfg(windows)'.dependencies]
windows-sys = "0.59""#,
            ),
        );
        shared.add("right", &manifest("[dependencies]\nitoa = \"1\""));
        assert!(shared.manifest()["dependencies"].as_table().unwrap().len() == 1);
        assert_eq!(shared.users, BTreeSet::from(["right".to_owned()]));
    }

    /// Incompatible versions of one package are two crates, each shared.
    #[test]
    fn keeps_incompatible_versions_apart() {
        let mut shared = Shared::default();
        shared.add("left", &manifest("[dependencies]\nrand = \"0.8\""));
        shared.add("right", &manifest("[dependencies]\nrand = \"0.9.1\""));
        shared.add("third", &manifest("[dependencies]\nrand = \"~0.8.5\""));
        let written = shared.manifest();
        let versions: Vec<_> = written["dependencies"]
            .as_table()
            .unwrap()
            .values()
            .map(|dep| dep["version"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(versions, ["0.8", "0.9.1"]);
        assert_eq!(compatible("1.2"), "1");
        assert_eq!(compatible("0.0.3"), "0.0.3");
        assert_eq!(compatible(">=0.4, <0.5"), "0.4");
    }

    /// One mod alone cannot reach a registry crate twice: no shared library is written.
    #[test]
    fn writes_nothing_for_one_mod() {
        let out = tempfile::tempdir().unwrap();
        let mut shared = Shared::default();
        shared.add("only", &manifest("[dependencies]\nitoa = \"1\""));
        assert!(shared.write(out.path()).unwrap().is_empty());
        assert!(!out.path().join(RELATIVE).exists());
    }

    /// The written library parses back as a dylib package, and a linked mod names it.
    #[test]
    fn writes_the_library_and_links_a_mod() {
        let out = tempfile::tempdir().unwrap();
        let mut shared = Shared::default();
        shared.add("left", &manifest("[dependencies]\nitoa = \"1\""));
        shared.add("right", &manifest("[dependencies]\nitoa = \"1\""));
        let users = shared.write(out.path()).unwrap();
        assert_eq!(users.len(), 2);
        let written: toml::Table = fs::read_to_string(out.path().join(RELATIVE).join("Cargo.toml"))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(written["package"]["name"].as_str(), Some(PACKAGE));
        let mut mod_manifest = manifest("[dependencies]\nitoa = \"1\"");
        let mut source = String::from("pub fn f() {}\n");
        link(&mut mod_manifest, &mut source).unwrap();
        assert_eq!(
            mod_manifest["dependencies"][CRATE]["path"].as_str(),
            Some(DEPENDENCY_PATH)
        );
        assert!(source.ends_with(&format!("extern crate {CRATE} as _;\n")));
        assert!(link(&mut mod_manifest, &mut source).is_err());
    }
}
