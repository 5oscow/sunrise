use std::{env, fs, path::Path};

fn discover(dir: &Path, declarations: &mut Vec<(String, String)>) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("extension tag declaration directory")
        .map(|e| e.expect("directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            discover(&path, declarations);
        } else if let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".extension-tags.json"))
        {
            declarations.push((
                name.to_uppercase(),
                fs::read_to_string(&path).expect("extension tag declaration"),
            ));
        }
    }
}

fn main() {
    let mut declarations = Vec::new();
    discover(Path::new("../sunrise-client/src"), &mut declarations);
    let mut generated = String::new();
    let mut all = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (name, text) in declarations {
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&text).expect("extension tags must be JSON records");
        generated.push_str(&format!("pub const {name}: &[ExtensionTag] = &[\n"));
        for row in rows {
            let registry = row["registry"].as_str().expect("extension tag registry");
            let id = row["id"].as_str().expect("extension tag id");
            let doc = row["doc"].as_str().expect("extension tag documentation");
            assert!(id.starts_with("sunrise:") && !id.starts_with("sunrise:class/"));
            assert!(
                seen.insert((registry.to_owned(), id.to_owned())),
                "duplicate extension tag"
            );
            let record =
                format!("ExtensionTag {{ registry: {registry:?}, id: {id:?}, doc: {doc:?} }},\n");
            generated.push_str(&record);
            all.push(record);
        }
        generated.push_str("];\n");
    }
    generated.push_str("pub const ALL: &[ExtensionTag] = &[\n");
    for record in all {
        generated.push_str(&record);
    }
    generated.push_str("];\n");
    fs::write(
        Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join("extension_tags.rs"),
        generated,
    )
    .expect("generated extension tags");
}
