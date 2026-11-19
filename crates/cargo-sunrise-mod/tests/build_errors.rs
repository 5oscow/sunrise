use std::process::Command;

/// A starter native mod with a type error, built by the tool in a folder whose name has an `=`:
/// the error names the mod's own file, not the copy the build compiles in its cache.
#[test]
#[ignore = "daily"]
fn a_failed_native_build_names_the_authors_file_and_not_the_cache_copy() {
    let scratch = tempfile::tempdir().unwrap();
    let parent = scratch.path().join("mods=one");
    std::fs::create_dir(&parent).unwrap();
    let cache = scratch.path().join("cache");
    let tool = |dir: &std::path::Path, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_cargo-sunrise-mod"))
            .current_dir(dir)
            .env("SUNRISE_MOD_CACHE", &cache)
            .args(args)
            .output()
            .unwrap()
    };
    let made = tool(&parent, &["new", "greeter", "--template", "native"]);
    assert!(made.status.success(), "{made:?}");
    let project = parent.join("greeter");
    let source = project.join("src/lib.rs");
    let template = std::fs::read_to_string(&source).unwrap();
    std::fs::write(
        &source,
        template.replace("format!(", "{ let wrong: u32 = \"text\"; wrong }; format!("),
    )
    .unwrap();

    let built = tool(&project, &["build"]);
    assert!(!built.status.success(), "{built:?}");
    let shown = String::from_utf8_lossy(&built.stderr);
    let project = std::fs::canonicalize(&project).unwrap();
    assert!(
        shown.contains(&project.join("src/lib.rs").display().to_string()),
        "{shown}"
    );
    // The error's locations; the line naming the build log is in the cache, as it should be.
    let locations: Vec<_> = shown.lines().filter(|line| line.contains("-->")).collect();
    assert!(!locations.is_empty(), "{shown}");
    assert!(
        locations
            .iter()
            .all(|line| !line.contains(&cache.display().to_string())),
        "{shown}"
    );
}
