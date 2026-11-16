//! A mod built by the build tool names each item of the game it uses; the real fixture loads on
//! the game's table, and is refused, naming the item, when one of those items changed.
#[path = "../src/test_stack.rs"]
mod test_stack;
use sunrise_mod_package::manifest::Package;
use sunrise_mod_wasm::{WasmPackage, load};

fn fixture() -> (WasmPackage, Vec<u8>) {
    let (Ok(fixture), Ok(std)) = (
        std::env::var("SUNRISE_WASM_FIXTURE"),
        std::env::var("SUNRISE_WASM_STD"),
    ) else {
        panic!(
            "SUNRISE_WASM_FIXTURE and SUNRISE_WASM_STD must be set: run with the wasm-mods workflow's environment"
        );
    };
    let path = std::path::Path::new(&fixture);
    let mut package = WasmPackage {
        manifest: Package::read(path).unwrap().manifest,
        module: Package::read_wasm(path).unwrap(),
    };
    package.manifest.inject.clear();
    (package, std::fs::read(std).unwrap())
}

/// The module with the last digit of the first name that starts `prefix` changed: the name keeps
/// its length, so no section moves.
fn changed(module: &[u8], prefix: &[u8]) -> (Vec<u8>, String) {
    let start = module
        .windows(prefix.len())
        .position(|window| window == prefix)
        .expect("the fixture imports such a name");
    let end = start
        + module[start..]
            .iter()
            .position(|&byte| byte == b'#')
            .unwrap()
        + 33;
    let mut out = module.to_vec();
    out[end - 1] = if out[end - 1] == b'0' { b'1' } else { b'0' };
    (out, String::from_utf8(module[start..end].to_vec()).unwrap())
}

#[test]
#[ignore = "needs SUNRISE_WASM_FIXTURE and SUNRISE_WASM_STD, which the wasm-mods workflow builds"]
fn the_fixture_loads_on_the_games_table() {
    test_stack::on_guest_stack(|| {
        mod_api::__private::initialize().unwrap();
        let (package, std) = fixture();
        let loaded = load(&std, vec![package]);
        assert!(loaded.refused.is_empty(), "{:?}", loaded.refused);
        assert_eq!(loaded.mods.len(), 1);
    })
}

#[test]
#[ignore = "needs SUNRISE_WASM_FIXTURE and SUNRISE_WASM_STD, which the wasm-mods workflow builds"]
fn a_changed_call_the_fixture_uses_refuses_it_by_name() {
    test_stack::on_guest_stack(|| {
        mod_api::__private::initialize().unwrap();
        for prefix in [&b"call:"[..], b"hook:", b"part:"] {
            let (mut package, std) = fixture();
            let (module, name) = changed(&package.module, prefix);
            package.module = module;
            let loaded = load(&std, vec![package]);
            assert!(loaded.mods.is_empty());
            let reason = loaded.refused[0].1.to_string();
            let item = name.split('#').next().unwrap();
            assert!(
                reason.contains(item.trim_start_matches("call:")) && reason.contains("changed"),
                "{name}: {reason}"
            );
            assert!(
                reason.ends_with("(or a type it carries changed)"),
                "{reason}"
            );
        }
    })
}
