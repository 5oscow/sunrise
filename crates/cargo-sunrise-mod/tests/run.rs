use std::{path::Path, process::Command};

#[test]
#[ignore = "daily"]
fn run_refuses_code_mods_without_building() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("native-run-refusal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // A code mod's folder, made by hand: `run` looks for its `Cargo.toml` and nothing else.
    let project = dir.join("machine");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = 'machine'\nversion = '0.1.0'\n",
    )
    .unwrap();
    let executable = env!("CARGO_BIN_EXE_cargo-sunrise-mod");

    let refused = Command::new(executable)
        .current_dir(&project)
        .args(["run", "--game"])
        .arg(dir.join("must-not-start"))
        .output()
        .unwrap();
    assert!(!refused.status.success(), "{refused:?}");
    let error = String::from_utf8(refused.stderr).unwrap();
    assert!(
        error.contains("code mods run through the launcher's native build"),
        "{error}"
    );
    assert!(
        error.contains("sunrise-launcher native-build . --trust-native-code"),
        "{error}"
    );
    assert!(
        error.contains("docs/modding/src/how-to/native-mods.md"),
        "{error}"
    );
    assert!(
        !project.join("target").exists(),
        "run must refuse before building"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
