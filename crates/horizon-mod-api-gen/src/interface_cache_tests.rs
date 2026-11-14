use std::{fs, path::Path};

fn compare(root: &Path, plain: &Path, cached: &Path) {
    // Each generation gets a fresh bounded proc_macro2 span arena; its disk cache stays warm.
    for (generate, output) in [
        (
            crate::build::generate_points_inner as fn(&Path, &Path) -> Result<(), String>,
            plain,
        ),
        (crate::generate_points, cached),
        (crate::build::generate_game_points_inner, plain),
        (crate::generate_game_points, cached),
        (crate::build::generate_engine_points_inner, plain),
        (crate::generate_engine_points, cached),
    ] {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(64 << 20)
                .spawn_scoped(scope, move || generate(root, output).unwrap())
                .unwrap()
                .join()
                .unwrap();
        });
    }
    for name in OUTPUTS {
        assert_eq!(
            fs::read(plain.join(name)).unwrap(),
            fs::read(cached.join(name)).unwrap(),
            "{name}"
        );
    }
}

const OUTPUTS: [&str; 4] = [
    "points_generated.rs",
    "game_records.rs",
    "game_catalog.rs",
    "engine_points.rs",
];

fn snapshot(output: &Path) -> Vec<Vec<u8>> {
    OUTPUTS
        .iter()
        .map(|name| fs::read(output.join(name)).unwrap())
        .collect()
}

#[test]
#[ignore = "full source-tree generation equivalence; run explicitly when changing the cache"]
fn cached_points_match_full_generation_across_source_changes() {
    let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scratch = original
        .join("target")
        .join(format!("interface-cache-test-{}", std::process::id()));
    fs::create_dir_all(&scratch).unwrap();
    let root = scratch.join("source");
    let tracked = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(&original)
        .output()
        .unwrap();
    assert!(tracked.status.success());
    for relative in tracked
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let relative = std::str::from_utf8(relative).unwrap();
        let source = original.join(relative);
        if source.is_file() {
            let destination = root.join(relative);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(source, destination).unwrap();
        }
    }
    let plain = scratch.join("plain");
    let cached = scratch.join("cached");
    fs::create_dir_all(&plain).unwrap();
    fs::create_dir_all(&cached).unwrap();
    compare(&root, &plain, &cached);
    compare(&root, &plain, &cached);
    let lower = fs::read(cached.join("engine_points.rs")).unwrap();
    let probe = root.join("crates/horizon-client/src/game/mobs/build_cache_probe.rs");
    fs::write(
        &probe,
        "pub fn build_cache_probe(value: i32) -> i32 { value }\n",
    )
    .unwrap();
    compare(&root, &plain, &cached);
    assert_eq!(lower, fs::read(cached.join("engine_points.rs")).unwrap());
    let before = snapshot(&cached);
    fs::write(
        &probe,
        "pub fn build_cache_probe(value: i32) -> i32 { value + 1 }\n",
    )
    .unwrap();
    compare(&root, &plain, &cached);
    assert_eq!(before, snapshot(&cached));
    fs::write(
        &probe,
        "pub fn build_cache_probe(value: i64) -> i64 { value }\n",
    )
    .unwrap();
    compare(&root, &plain, &cached);
    assert_ne!(before, snapshot(&cached));
    assert_eq!(lower, fs::read(cached.join("engine_points.rs")).unwrap());
    let renamed = probe.with_file_name("build_cache_renamed.rs");
    fs::rename(&probe, &renamed).unwrap();
    compare(&root, &plain, &cached);
    fs::remove_file(renamed).unwrap();
    compare(&root, &plain, &cached);
    assert_eq!(lower, fs::read(cached.join("engine_points.rs")).unwrap());
    fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn unchanged_outputs_keep_their_timestamp_but_changed_outputs_replace_bytes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    fs::create_dir_all(&root).unwrap();
    let output = root.join(format!("write-if-changed-test-{}", std::process::id()));
    crate::write_if_changed(&output, b"first").unwrap();
    let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000);
    fs::File::options()
        .write(true)
        .open(&output)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(old))
        .unwrap();
    crate::write_if_changed(&output, b"first").unwrap();
    assert_eq!(fs::metadata(&output).unwrap().modified().unwrap(), old);
    crate::write_if_changed(&output, b"second").unwrap();
    assert_eq!(fs::read(&output).unwrap(), b"second");
    fs::remove_file(output).unwrap();
}
