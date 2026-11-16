//! The wire identity of the public types of the mod libraries a mod builds against, recorded in
//! `layouts.json`: one shape function over the types as the libraries' source declares them
//! ([`mod_source::library`]), with no compile.
use mod_source::library::{Environment, identities};
use mod_surface::Surface;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    resolve: Resolve,
}
#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}
#[derive(Deserialize)]
struct Node {
    id: String,
    dependencies: Vec<String>,
}
#[derive(Deserialize)]
struct Package {
    id: String,
    manifest_path: PathBuf,
    targets: Vec<Target>,
}
#[derive(Deserialize)]
struct Target {
    name: String,
    kind: Vec<String>,
    src_path: PathBuf,
}

fn output(command: &mut Command) -> Result<Vec<u8>, String> {
    let result = command.output().map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr).into_owned());
    }
    Ok(result.stdout)
}
fn save(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if fs::read(path).ok().as_deref() != Some(bytes) {
        fs::write(path, bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Runs a Rust tool (`cargo`): the launcher's pinned toolchain outside tests.
type Tool<'a> = &'a dyn Fn(&str) -> Command;

/// Writes `layouts.json`: for each mod library the mod builds against, the identity of each of
/// its public wire types, read from its source with the launcher's pinned toolchain's metadata.
/// `api` is the pinned SDK's surface, which shapes the `mod_api` types they name; a build with
/// none (inside the game's own checkout) refuses a library type that names one.
pub fn build(root: &Path, api: Option<&Surface>) -> Result<(), String> {
    build_with(&crate::wasm::pinned, root, api)
}

fn build_with(tool: Tool, root: &Path, api: Option<&Surface>) -> Result<(), String> {
    if !root.join("Cargo.toml").is_file() {
        return Ok(());
    }
    // `cargo metadata` prints plain Windows paths, without the verbatim prefix
    // `canonicalize` adds, so member manifests are compared in that form.
    let root = dunce::canonicalize(root).map_err(|e| e.to_string())?;
    let declaration = sunrise_mod_package::Manifest::parse(
        &fs::read_to_string(root.join(sunrise_mod_package::manifest::MANIFEST))
            .map_err(|e| e.to_string())?,
    )?;
    let cargo: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("Cargo.toml")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let lib = root.join(
        cargo
            .get("lib")
            .and_then(|v| v.get("path"))
            .and_then(toml::Value::as_str)
            .unwrap_or("src/lib.rs"),
    );
    let destination = root.join("layouts.json");
    let refuse_to_lose = || -> Result<(), String> {
        if destination.exists() {
            return Err("layouts: existing layouts.json would lose all recorded types; remove it explicitly after reviewing the public API change".into());
        }
        Ok(())
    };
    if declaration.dependencies.is_empty()
        && lib.is_file()
        && mod_source::wire_types::public_types("probe", &lib)?.is_empty()
    {
        return refuse_to_lose();
    }
    let metadata: Metadata = serde_json::from_slice(&output(
        tool("cargo")
            .args(["metadata", "--format-version", "1", "--offline"])
            .current_dir(&root),
    )?)
    .map_err(|e| e.to_string())?;
    let owner = metadata
        .packages
        .iter()
        .find(|p| p.manifest_path == root.join("Cargo.toml"))
        .ok_or("mod is not a Cargo package")?;
    // Every package the mod builds against, each after the ones it depends on.
    let mut order = Vec::new();
    let mut seen = BTreeSet::new();
    fn visit<'m>(
        id: &'m str,
        metadata: &'m Metadata,
        seen: &mut BTreeSet<&'m str>,
        order: &mut Vec<&'m str>,
    ) {
        if !seen.insert(id) {
            return;
        }
        if let Some(node) = metadata.resolve.nodes.iter().find(|n| n.id == id) {
            for dependency in &node.dependencies {
                visit(dependency, metadata, seen, order);
            }
        }
        order.push(id);
    }
    visit(&owner.id, &metadata, &mut seen, &mut order);
    let mut libraries: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut recorded: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for id in order {
        let package = metadata
            .packages
            .iter()
            .find(|p| p.id == id)
            .ok_or("a resolved package is missing from the metadata")?;
        let directory = package
            .manifest_path
            .parent()
            .ok_or("package manifest has no directory")?;
        let Ok(text) = fs::read_to_string(directory.join(sunrise_mod_package::manifest::MANIFEST))
        else {
            continue;
        };
        let manifest = sunrise_mod_package::Manifest::parse(&text)?;
        let Some(target) = package.targets.iter().find(|t| {
            t.kind
                .iter()
                .any(|k| matches!(k.as_str(), "lib" | "rlib" | "dylib" | "cdylib"))
        }) else {
            continue;
        };
        let krate = target.name.replace('-', "_");
        let types = mod_source::wire_types::public_type_definitions(&krate, &target.src_path)?;
        if types.is_empty() {
            continue;
        }
        let found = identities(
            &krate,
            &target.src_path,
            &types,
            &Environment {
                api,
                libraries: &libraries,
            },
        )?;
        libraries.insert(krate, found.clone());
        recorded.insert(manifest.id, found);
    }
    if recorded.is_empty() {
        return refuse_to_lose();
    }
    save(
        &destination,
        &serde_json::to_vec(&recorded).map_err(|e| e.to_string())?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str =
        "version = \"0.1.0\"\napi = \"0.2\"\nside = \"both\"\ncapabilities = [\"registries\"]\n";

    /// A mod `addon` that depends on a mod library `library` whose source is `library_source`.
    fn project(name: &str, library_source: &str, addon_source: &str) -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../target/layouts-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let library = root.join("library");
        fs::create_dir_all(library.join("src")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        let api = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../sunrise-mod-api")
            .canonicalize()
            .unwrap();
        fs::write(library.join("Cargo.toml"), format!("[package]\nname = \"library\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nsunrise-mod-api = {{ path = {:?} }}\n", api.to_string_lossy())).unwrap();
        fs::write(
            library.join("sunrise-mod.toml"),
            format!("id = \"library\"\nname = \"library\"\n{HEADER}"),
        )
        .unwrap();
        fs::write(library.join("src/lib.rs"), library_source).unwrap();
        fs::write(root.join("Cargo.toml"), format!("[package]\nname = \"addon\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\n[dependencies]\nsunrise-mod-api = {{ path = {:?} }}\nlibrary = {{ path = \"library\" }}\n", api.to_string_lossy())).unwrap();
        fs::write(
            root.join("sunrise-mod.toml"),
            format!(
                "id = \"addon\"\nname = \"addon\"\n{HEADER}[dependencies]\nlibrary = \"^0.1\"\n"
            ),
        )
        .unwrap();
        fs::write(root.join("src/lib.rs"), addon_source).unwrap();
        root
    }

    /// The toolchain running the tests: CI has no launcher-pinned toolchain installed.
    fn build(root: &Path) -> Result<(), String> {
        let tool = |name: &str| match name {
            "cargo" => Command::new(env!("CARGO")),
            other => Command::new(other),
        };
        build_with(&tool, root, None)
    }

    fn recorded(root: &Path) -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join("layouts.json")).unwrap()).unwrap()
    }

    #[test]
    #[ignore = "daily"]
    fn a_library_types_identity_follows_its_fields_and_the_types_they_nest() {
        let root = project(
            "fields",
            "mod inner { #[derive(mod_api::Wire)] pub struct Part { pub value: u32 } }\n\
             #[derive(mod_api::Wire)] pub struct Record { pub part: inner::Part, pub tag: String }\n",
            "pub fn run() {}\n",
        );
        build(&root).unwrap();
        let before = recorded(&root);
        let record = before["library"]["library::Record"]
            .as_str()
            .unwrap()
            .to_owned();
        // A nested type's field is part of the outer type's identity.
        fs::write(
            root.join("library/src/lib.rs"),
            "mod inner { #[derive(mod_api::Wire)] pub struct Part { pub value: u64 } }\n\
             #[derive(mod_api::Wire)] pub struct Record { pub part: inner::Part, pub tag: String }\n",
        )
        .unwrap();
        build(&root).unwrap();
        let after = recorded(&root);
        assert_ne!(
            after["library"]["library::Record"].as_str().unwrap(),
            record
        );
        // The path a type is defined at is not part of its identity.
        fs::write(
            root.join("library/src/lib.rs"),
            "mod moved { #[derive(mod_api::Wire)] pub struct Part { pub value: u64 } }\n\
             #[derive(mod_api::Wire)] pub struct Record { pub part: moved::Part, pub tag: String }\n",
        )
        .unwrap();
        build(&root).unwrap();
        assert_eq!(recorded(&root), after);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "daily"]
    fn removing_the_last_public_type_must_be_reviewed() {
        let root = project(
            "removed",
            "#[derive(mod_api::Wire)] pub struct Record { pub field: u32 }\n",
            "pub fn run() {}\n",
        );
        build(&root).unwrap();
        fs::write(root.join("library/src/lib.rs"), "pub struct Local;\n").unwrap();
        assert!(
            build(&root)
                .unwrap_err()
                .contains("lose all recorded types")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "daily"]
    fn a_library_type_naming_a_mod_api_type_needs_the_pinned_surface() {
        let root = project(
            "api",
            "#[derive(mod_api::Wire)] pub struct Held { pub stack: mod_api::ItemStack }\n",
            "pub fn run() {}\n",
        );
        let error = build(&root).unwrap_err();
        assert!(
            error.contains("ItemStack") && error.contains("no pinned SDK"),
            "{error}"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
