use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

const BIN: &str = env!("CARGO_BIN_EXE_cargo-sunrise-mod");
const ENTRY: &str = "mods/coords_hud.toml";
fn manifest(dir: &Path, version: &str) {
    fs::write(
        dir.join("sunrise-mod.toml"),
        format!(
            "id = 'coords_hud'\nname = 'Coordinates HUD'\nversion = '{version}'\n\
             description = 'A coordinates display for players who are exploring new places.'\n\
             license = 'MIT'\nhomepage = 'https://github.com/ada/coords_hud'\n"
        ),
    )
    .unwrap();
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

#[cfg(unix)]
fn success(result: Output) -> String {
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

#[test]
#[ignore = "daily"]
fn bad_version_or_checkout_leaves_destination_and_outside_files_unchanged() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path();
    let out = root.join("out");
    let checkout = root.join("registry");
    for directory in [out.join("mods"), checkout.join("mods")] {
        fs::create_dir_all(directory).unwrap();
    }
    fs::write(out.join(ENTRY), b"old destination").unwrap();
    for sentinel in ["sentinel.smod", "sentinel.toml"] {
        fs::write(root.join(sentinel), b"outside submission").unwrap();
    }
    let attempt = |expected: &str| {
        let result = run(root, &["publish", "--out", "out", "--registry", "registry"]);
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.contains(expected), "{error}");
        assert_eq!(fs::read(out.join(ENTRY)).unwrap(), b"old destination");
        for sentinel in ["sentinel.smod", "sentinel.toml"] {
            assert_eq!(
                fs::read(root.join(sentinel)).unwrap(),
                b"outside submission"
            );
        }
        assert_eq!(fs::read_dir(&out).unwrap().count(), 1);
        assert!(!root.join("target").exists());
    };
    manifest(root, "1/../../sentinel");
    attempt("sunrise-mod.toml: version");

    manifest(root, "1");
    fs::write(checkout.join(ENTRY), "id = [").unwrap();
    attempt(ENTRY);
    let entry = "kind = 'mod'\nowners = ['ada']\n";
    fs::write(
        checkout.join(ENTRY),
        format!("id = '../../sentinel'\n{entry}"),
    )
    .unwrap();
    attempt(ENTRY);

    let correct = format!("id = 'coords_hud'\n{entry}");
    fs::write(checkout.join(ENTRY), &correct).unwrap();
    fs::write(checkout.join("mods/other.toml"), &correct).unwrap();
    attempt("mods/other.toml");

    fs::remove_file(checkout.join("mods/other.toml")).unwrap();
    fs::write(checkout.join(ENTRY), format!("{correct}\n[[versions]]\nversion = '1'\npublished = '2026-11-18'\ngame = '26.3'\nfile = 'coords_hud-1.smod'\nsha256 = '{}'\n", "0".repeat(64))).unwrap();
    attempt("coords_hud 1");
}

#[cfg(unix)]
#[test]
#[ignore = "daily"]
fn output_replaces_file_symlinks_but_refuses_linked_directories() {
    use std::os::unix::fs::symlink;
    let work = tempfile::tempdir().unwrap();
    let root = work.path();
    manifest(root, "1");
    let outside = root.join("outside");
    fs::create_dir(&outside).unwrap();
    let sentinel = outside.join("coords_hud.toml");
    fs::write(&sentinel, b"untouched").unwrap();
    fs::create_dir(root.join("out")).unwrap();
    fs::write(root.join("out/PULL_REQUEST.md"), b"old request").unwrap();
    symlink(&outside, root.join("out/mods")).unwrap();
    let args = ["publish", "--out", "out"];
    let blocked = run(root, &args);
    assert!(!blocked.status.success());
    assert!(String::from_utf8_lossy(&blocked.stderr).contains("out/mods"));
    assert_eq!(fs::read(&sentinel).unwrap(), b"untouched");
    assert_eq!(
        fs::read(root.join("out/PULL_REQUEST.md")).unwrap(),
        b"old request"
    );
    assert!(!root.join("target").exists());

    fs::remove_file(root.join("out/mods")).unwrap();
    fs::create_dir(root.join("out/mods")).unwrap();
    let dest = root.join("out").join(ENTRY);
    symlink(&sentinel, &dest).unwrap();
    success(run(root, &args));
    assert!(fs::symlink_metadata(dest).unwrap().is_file());
    assert_eq!(fs::read(&sentinel).unwrap(), b"untouched");
}
