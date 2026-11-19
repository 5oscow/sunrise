//! Add an exact, original Java jar as a passive asset/data source.
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use sunrise_mod_package::{
    Manifest,
    manifest::{JarRef, MANIFEST, collect_jar_requirements},
    selection::{LaunchArguments, data_dir_with},
};
use sunrise_port_pack::fetch::hash_reader;

/// The store the game would use with neither the tool nor a `--home` argument.
fn default_store() -> PathBuf {
    std::env::var_os("SUNRISE_HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir_with(|key| std::env::var_os(key)).join("sunrise"))
        .join("jars")
}

/// Stage only the project's exact originals. Returns whether `run` needs to set `SUNRISE_HOME`.
pub(crate) fn stage_for_run(
    manifest: &Manifest,
    home: &Path,
    source: Option<&Path>,
    game_args: &[String],
) -> Result<bool, String> {
    if manifest.jar.is_empty() {
        return Ok(false);
    }
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let launch = LaunchArguments::parse(game_args.iter().cloned(), &cwd, home, None)
        .map_err(|e| e.to_string())?;
    if launch.context.home != cwd.join(home) {
        return Err(format!(
            "{} needs jars in its development home ({}); omit the game's --home and use `cargo sunrise-mod run --jars <dir>` to choose where to read originals",
            manifest.id,
            home.display()
        ));
    }
    let source = source.map_or_else(default_store, Path::to_path_buf);
    fs::create_dir_all(home).map_err(|e| format!("{}: {e}", home.display()))?;
    let home = dunce::canonicalize(home).map_err(|e| format!("{}: {e}", home.display()))?;
    let dest = home.join("jars");
    let source_in_stage = dunce::canonicalize(&source).is_ok_and(|dir| {
        dir.starts_with(&dest) || dunce::canonicalize(&dest).is_ok_and(|staged| dir == staged)
    });
    let requirements = collect_jar_requirements([(manifest.id.as_str(), manifest.jar.as_slice())])
        .map_err(|e| e.to_string())?;
    let pending = tempfile::Builder::new()
        .prefix("jars-")
        .tempdir_in(&home)
        .map_err(|e| format!("{}: {e}", home.display()))?;
    for requirement in requirements {
        let jar = requirement.jar;
        let path = jar.cache_path(&source);
        let unavailable = |why: &str| {
            format!(
                "{} needs Java jar {} {} (SHA-512 {}): {}: {why}\n\
                 Add the exact original with `cargo sunrise-mod use-jar <path>` and run without --jars, or use `cargo sunrise-mod run --jars <dir>` with `<sha512>.jar` files",
                manifest.id,
                jar.id(),
                jar.version(),
                jar.sha512(),
                path.display()
            )
        };
        let original = dunce::canonicalize(&path).map_err(|e| unavailable(&e.to_string()))?;
        if source_in_stage || original.starts_with(&dest) {
            return Err(unavailable(
                "the development home is not an original jar store",
            ));
        }
        let metadata = fs::metadata(&original).map_err(|e| unavailable(&e.to_string()))?;
        if !metadata.is_file() {
            return Err(unavailable("not a file"));
        }
        let file = fs::File::open(&original).map_err(|e| unavailable(&e.to_string()))?;
        let staged = jar.cache_path(pending.path());
        // The game opens these as passive ZIPs. Do not change the original's permissions or bytes.
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&original, &staged).is_ok();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&original, &staged).is_ok();
        #[cfg(not(any(unix, windows)))]
        let linked = false;
        let hash = if linked {
            hash_reader::<sha2::Sha512>(file, io::sink())
        } else {
            let staged =
                fs::File::create(&staged).map_err(|e| format!("{}: {e}", staged.display()))?;
            hash_reader::<sha2::Sha512>(file, staged)
        }
        .map_err(|e| unavailable(&e.to_string()))?;
        if format!("{hash:x}") != jar.sha512() {
            return Err(unavailable("the file does not match the listed SHA-512"));
        }
    }
    match fs::remove_dir_all(&dest) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => {
            return Err(format!("{}: {e}", dest.display()));
        }
        _ => {}
    }
    fs::rename(pending.path(), &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(true)
}

pub fn run(args: &[String]) -> Result<(), String> {
    let [path] = args else {
        return Err("usage: cargo sunrise-mod use-jar <path>".into());
    };
    let manifest_path = Path::new(MANIFEST);
    let text = fs::read_to_string(manifest_path).map_err(|e| format!("{MANIFEST}: {e}"))?;
    let manifest = Manifest::parse(&text)?;
    let path = Path::new(path);
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let store = default_store();
    fs::create_dir_all(&store).map_err(|e| format!("{}: {e}", store.display()))?;
    let mut original =
        tempfile::NamedTempFile::new_in(&store).map_err(|e| format!("{}: {e}", store.display()))?;
    let hash = hash_reader::<sha2::Sha512>(file, &mut original)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let identity = sunrise_port_pack::jar::read_identity(original.path())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let jar = JarRef::new(identity.id, identity.version, format!("{hash:x}"))?;
    let mut listed = false;
    for existing in manifest
        .jar
        .iter()
        .filter(|entry| entry.sha512() == jar.sha512())
    {
        if existing != &jar {
            return Err(format!(
                "{MANIFEST}: this hash lists {} {}, but {} identifies {} {}; correct the existing entry",
                existing.id(),
                existing.version(),
                path.display(),
                jar.id(),
                jar.version()
            ));
        }
        listed = true;
    }
    let candidate = if listed {
        None
    } else {
        let entry = toml::to_string(&jar).map_err(|e| e.to_string())?;
        let candidate = format!("{text}\n[[jar]]\n{entry}");
        Manifest::parse(&candidate).map_err(|e| format!(
            "{MANIFEST}: cannot append a jar entry; use [[jar]] tables rather than an inline jar array: {e}"
        ))?;
        Some(candidate)
    };
    // Publish the exact bytes that were hashed before naming them in the manifest. Re-listing a
    // jar can restore a deleted or damaged cache entry without editing the manifest.
    let cached = jar.cache_path(&store);
    original
        .persist(&cached)
        .map_err(|e| format!("{}: {e}", cached.display()))?;
    let Some(candidate) = candidate else {
        println!("Jar {} {} is already listed", jar.id(), jar.version());
        return Ok(());
    };
    let partial = manifest_path.with_extension(format!("toml.{}.partial", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)
        .map_err(|e| format!("{}: {e}", partial.display()))?;
    let result = (|| -> io::Result<()> {
        file.write_all(candidate.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&partial, manifest_path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result.map_err(|e| format!("{MANIFEST}: {e}"))?;
    println!("Added jar {} {}", jar.id(), jar.version());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_stages_only_declared_originals_and_refuses_a_missing_jar() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("originals");
        let home = root.path().join("dev-home");
        let sha512 = "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
                      2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f";
        let manifest = Manifest::parse(&format!(
            "id = 'garden'\nname = 'Garden'\nversion = '0.1.0'\napi = '0.2'\ncapabilities = ['registries']\n\
             [[jar]]\nid = 'original'\nversion = '1.0'\nsha512 = '{sha512}'\n"
        ))
        .unwrap();
        let original = source.join(format!("{sha512}.jar"));
        fs::create_dir(&source).unwrap();
        fs::write(&original, b"abc").unwrap();
        fs::write(source.join("unrelated.jar"), b"do not stage").unwrap();
        fs::create_dir_all(home.join("jars")).unwrap();
        fs::write(home.join("jars/stale.jar"), b"previous run").unwrap();

        assert!(stage_for_run(&manifest, &home, Some(&source), &[]).unwrap());
        let dest = home.join("jars").join(original.file_name().unwrap());
        assert_eq!(fs::read(dest).unwrap(), b"abc");
        assert_eq!(fs::read_dir(home.join("jars")).unwrap().count(), 1);
        assert_eq!(fs::read(&original).unwrap(), b"abc");
        assert_eq!(fs::read_dir(&source).unwrap().count(), 2);

        fs::remove_file(&original).unwrap();
        let error = stage_for_run(&manifest, &home, Some(&source), &[]).unwrap_err();
        for expected in [
            "garden needs Java jar original 1.0",
            sha512,
            &original.to_string_lossy(),
            "cargo sunrise-mod use-jar <path>",
            "cargo sunrise-mod run --jars <dir>",
        ] {
            assert!(error.contains(expected), "{error}");
        }
    }
}
