//! Cargo-facing generation: the mod API's and the game's generated files.
use super::{
    Catalog, Target, canonical_sources, free_function_source, host, native_hook_files, sources,
};
use quote::ToTokens;
use std::{fs, path::Path};
mod vocab_boundary;

fn write_vocab_output(
    out_dir: &Path,
    name: &str,
    code: &str,
    inventory: &vocab_boundary::Inventory,
    sources: &std::collections::BTreeSet<std::path::PathBuf>,
) -> Result<(), String> {
    vocab_boundary::validate_with_modules(name, code, inventory, sources)
        .map_err(|e| format!("{name}: {e}"))?;
    write_if_changed(out_dir.join(name), code).map_err(|e| e.to_string())
}

/// Preserve output timestamps when generation produces identical bytes.
pub fn write_if_changed(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    let path = path.as_ref();
    let bytes = bytes.as_ref();
    match fs::read(path) {
        Ok(previous) if previous == bytes => return Ok(()),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::write(path, bytes)
}

/// Generate only this crate's files, tracking every source read by the generator.
/// `root` is supplied at run time by the consuming crate's build script.
pub fn generate(root: &Path, out_dir: &Path, target: Target) -> Result<(), String> {
    track(root, matches!(&target, Target::Client))?;
    let inventory = if matches!(target, Target::Vocab) {
        let path = root.join("tools/layering/mod_vocab_state.json");
        crate::rerun_if_changed(&path.display());
        vocab_boundary::inventory(root)?
    } else {
        vocab_boundary::Inventory::new()
    };
    let vocab_sources = if matches!(target, Target::Vocab) {
        vocab_boundary::modules::sources(root)?
    } else {
        std::collections::BTreeSet::new()
    };
    if matches!(&target, Target::Client) {
        crate::rerun_if_staged(root);
    }
    let generated = if matches!(target, Target::Client) {
        sources(root, super::FillMode::Require)?
    } else {
        canonical_sources(root)?
    };
    let outputs = match target {
        Target::Client => vec![
            ("native_hook_modules.rs", native_hook_modules(root)?),
            (
                "lan_action_modules.rs",
                super::game_modules::lan_action_modules(
                    &root.join("crates/horizon-client/src/game/network/lan_actions"),
                )?,
            ),
            ("game_host_modules.rs", game_host_modules(root)?),
            ("mod_api_fire_generated.rs", generated.fire),
            // The mob tick's goal read view (#1077), included beside the
            // engine's `GoalCx` in `mobs/part_goals.rs`.
            ("mod_api_goal_view_generated.rs", generated.goal_view),
        ],
        Target::Api | Target::Vocab => {
            if matches!(target, Target::Api) {
                write_if_changed(
                    out_dir.join("parts_registration_modules.rs"),
                    super::parts::registration_source(root)?,
                )
                .map_err(|e| e.to_string())?;
            }
            if matches!(target, Target::Vocab) {
                write_vocab_output(
                    out_dir,
                    super::parts::OUTPUT,
                    &super::parts::source(root)?,
                    &inventory,
                    &vocab_sources,
                )?;
                for (name, code) in super::exports::sources(root)? {
                    write_vocab_output(out_dir, &name, &code, &inventory, &vocab_sources)?;
                }
            }
            let hooks = generated.hooks;
            let included = [
                (host::AREAS_OUTPUT, generated.areas.as_str()),
                ("mod_api_generated.rs", hooks.as_str()),
                ("mod_api_generated_host.rs", generated.host.as_str()),
            ];
            let (providers, sources) = super::capability_providers::generate(root, &included)?;
            // The resolver recurses through the API's types: give it a deep stack.
            let generated_wire = std::thread::scope(|scope| {
                std::thread::Builder::new()
                    .stack_size(64 << 20)
                    .spawn_scoped(scope, || super::wire::generate(root, &included))
                    .map_err(|e| e.to_string())?
                    .join()
                    .map_err(|_| "generating the wire impls panicked".to_string())?
            })?;
            for path in sources.iter().chain(&generated_wire.sources) {
                crate::rerun_if_changed(&path.display());
            }
            let outputs = vec![
                (host::AREAS_OUTPUT, generated.areas),
                ("mod_api_generated.rs", hooks),
                ("mod_api_generated_host.rs", generated.host),
                ("capability_providers_generated.rs", providers),
                ("wire_generated.rs", generated_wire.wire),
                ("wire_guest_generated.rs", generated_wire.guest),
            ];
            outputs
                .into_iter()
                .filter(|(name, _)| match target {
                    Target::Api => *name == "wire_guest_generated.rs",
                    Target::Vocab => *name != "wire_guest_generated.rs",
                    Target::Client => unreachable!(),
                })
                .collect()
        }
    };
    for (name, code) in outputs {
        let code = generated_source(root, &target, name, code)?;
        if matches!(target, Target::Vocab) {
            write_vocab_output(out_dir, name, &code, &inventory, &vocab_sources)?;
        } else {
            write_if_changed(out_dir.join(name), &code).map_err(|e| e.to_string())?;
        }
        syn::parse_file(&code).map_err(|e| format!("generated Rust: {name}: {e}"))?;
    }
    Ok(())
}

fn generated_source(
    root: &Path,
    target: &Target,
    name: &str,
    code: String,
) -> Result<String, String> {
    if matches!(target, Target::Client)
        && matches!(
            name,
            "mod_api_fire_generated.rs" | "mod_api_goal_view_generated.rs"
        )
        && root.join(".horizon-source-staged").is_file()
    {
        super::internals::widen_generated(Path::new(name), code)
    } else {
        Ok(code)
    }
}

#[cfg(test)]
mod generated_visibility_tests {
    use super::*;

    #[test]
    fn staged_game_helpers_are_widened_but_source_helpers_are_unchanged() {
        let root =
            std::env::temp_dir().join(format!("fnr-generated-visibility-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let input = "impl Game<'_> { pub(super) fn fire(&mut self) {} }";
        for name in [
            "mod_api_fire_generated.rs",
            "mod_api_goal_view_generated.rs",
        ] {
            assert_eq!(
                generated_source(&root, &Target::Client, name, input.into()).unwrap(),
                input
            );
        }
        fs::write(root.join(".horizon-source-staged"), "").unwrap();
        for name in [
            "mod_api_fire_generated.rs",
            "mod_api_goal_view_generated.rs",
        ] {
            assert_eq!(
                generated_source(&root, &Target::Client, name, input.into()).unwrap(),
                "impl Game<'_> { pub fn fire(&mut self) {} }"
            );
            assert_eq!(
                generated_source(&root, &Target::Vocab, name, input.into()).unwrap(),
                input
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}

pub(crate) fn native_hook_modules(root: &Path) -> Result<String, String> {
    let mut modules = String::new();
    for path in native_hook_files(root)? {
        let module = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| format!("{}: invalid native hook module name", path.display()))?;
        if matches!(module, "mod" | "shared") {
            continue;
        }
        syn::parse_str::<syn::Ident>(module).map_err(|e| e.to_string())?;
        let path = fs::canonicalize(&path).map_err(|e| e.to_string())?;
        modules.push_str(&format!("#[path = {path:?}] mod {module};\n"));
    }
    Ok(modules)
}

/// The game's side of the host traits: one module for each area file of `game/host/`.
pub(crate) fn game_host_modules(root: &Path) -> Result<String, String> {
    let dir = root.join("crates/horizon-client/src/game/host");
    let mut modules = String::new();
    for stem in ["draw", "services"] {
        let path = fs::canonicalize(dir.join(format!("{stem}.rs"))).map_err(|e| e.to_string())?;
        modules.push_str(&format!("#[path = {path:?}] pub(crate) mod {stem};\n"));
    }
    modules.push_str(&format!(
        "mod generated {{\n{}\n}}\n",
        host::game_forwarders(root)?
    ));
    Ok(modules)
}

/// Generate the native-only raw render records, without trait dispatch or subscriptions.
pub fn generate_render(root: &Path, out_dir: &Path) -> Result<(), String> {
    let declarations = root.join(super::catalog::DECLARATIONS);
    crate::rerun_if_changed(&declarations.display());
    for module in Catalog::modules(&declarations)? {
        crate::rerun_if_changed(&declarations.join(format!("{module}.rs")).display());
    }
    crate::rerun_if_changed(
        &root
            .join("crates/horizon-mod-render/src/render.rs")
            .display(),
    );
    let source = super::dispatch::render(root, &Catalog::load(root)?)?.render_dispatch;
    use syn::spanned::Spanned;
    let file = syn::parse_file(&source).map_err(|error| error.to_string())?;
    let lines: Vec<_> = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(index, _)| index + 1))
        .collect();
    let offset = |point: proc_macro2::LineColumn| lines[point.line - 1] + point.column;
    let mut records = String::from("use super::*;\n");
    for item in file.items {
        let syn::Item::Struct(record) = item else {
            continue;
        };
        let start = record
            .attrs
            .iter()
            .find(|attr| !attr.path().is_ident("doc"))
            .map_or_else(|| record.vis.span().start(), |attr| attr.span().start());
        records.push_str(&source[offset(start)..offset(record.span().end())]);
        records.push('\n');
    }
    syn::parse_file(&records).map_err(|error| error.to_string())?;
    write_if_changed(out_dir.join("native_render_generated.rs"), records)
        .map_err(|error| error.to_string())
}

#[test]
fn render_records_need_no_game_sources() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = workspace.join(format!("target/render-records-only-{}", std::process::id()));
    let declarations = root.join(super::catalog::DECLARATIONS);
    fs::create_dir_all(&declarations).unwrap();
    for entry in fs::read_dir(workspace.join(super::catalog::DECLARATIONS)).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            fs::copy(&path, declarations.join(path.file_name().unwrap())).unwrap();
        }
    }
    let raw = "crates/horizon-mod-render/src/render.rs";
    fs::create_dir_all(root.join(raw).parent().unwrap()).unwrap();
    fs::copy(workspace.join(raw), root.join(raw)).unwrap();
    assert!(!root.join("crates/horizon-client").exists());
    generate_render(&root, &root).unwrap();
    let generated = fs::read_to_string(root.join("native_render_generated.rs")).unwrap();
    let file = syn::parse_file(&generated).unwrap();
    assert_eq!(
        file.items
            .iter()
            .filter(|item| matches!(item, syn::Item::Struct(_)))
            .count(),
        5
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "daily"]
fn listed_render_slots_have_eligible_points() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    super::points::game_points(&root).unwrap();
}

/// The `mod_api` types with a `Wire` layout ([`super::wire`]), for point records.
/// A source tree without the mod API (a generator fixture's) has none.
pub(crate) fn wire_types(root: &Path) -> Result<std::collections::BTreeSet<String>, String> {
    if !root.join("crates/horizon-mod-vocab/src/hooks").is_dir() {
        return Ok(Default::default());
    }
    let generated = canonical_sources(root)?;
    wire_types_from_sources(root, &generated)
}

/// Resolve Wire layouts from already validated, immutable API sources.
/// The caller has established that the mod API directory exists.
pub(super) fn wire_types_from_sources(
    root: &Path,
    generated: &super::Sources,
) -> Result<std::collections::BTreeSet<String>, String> {
    let included = [
        (host::AREAS_OUTPUT, generated.areas.as_str()),
        ("mod_api_generated.rs", generated.hooks.as_str()),
        ("mod_api_generated_host.rs", generated.host.as_str()),
    ];
    // The resolver recurses through the API's types: give it a deep stack.
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(64 << 20)
            .spawn_scoped(scope, || super::wire::owned_types(root, &included))
            .map_err(|e| e.to_string())?
            .join()
            .map_err(|_| "reading the wire types panicked".to_string())?
    })
}

/// The game/SDK owner's complete point catalog and canonical game records, tracking game inputs.
pub fn generate_points(root: &Path, out_dir: &Path) -> Result<(), String> {
    crate::interface_cache::with_cache(out_dir, || {
        crate::source::with_walk_cache(|| generate_points_inner(root, out_dir))
    })
}

/// Generate the engine-owned point vocabulary and typed dispatch.
pub fn generate_engine_points(root: &Path, out_dir: &Path) -> Result<(), String> {
    crate::interface_cache::with_cache(out_dir, || {
        crate::source::with_walk_cache(|| generate_engine_points_inner(root, out_dir))
    })
}

pub(super) fn generate_engine_points_inner(root: &Path, out_dir: &Path) -> Result<(), String> {
    track(root, false)?;
    for path in super::points::tracked_lower(root)? {
        crate::rerun_if_changed(&path.display());
    }
    crate::rerun_if_staged(root);
    let points = super::points::lower_points(root, &wire_types(root)?)?;
    println!("cargo:rerun-if-env-changed=HORIZON_POINT_NAMES");
    let selected = std::env::var("HORIZON_POINT_NAMES")
        .ok()
        .map(|json| {
            serde_json::from_str::<std::collections::BTreeSet<String>>(&json)
                .map_err(|error| format!("HORIZON_POINT_NAMES: {error}"))
        })
        .transpose()?;
    let code = super::points::engine_records(&points, selected.as_ref());
    let generated = syn::parse_file(&code).map_err(|e| format!("engine points: {e}"))?;
    let mut code = String::new();
    for item in generated.items {
        code.push_str(&item.to_token_stream().to_string());
        code.push('\n');
    }
    write_if_changed(out_dir.join("engine_points.rs"), code).map_err(|e| e.to_string())
}

/// Generate the neutral game-owned canonical records and complete erased catalog.
pub fn generate_game_points(root: &Path, out_dir: &Path) -> Result<(), String> {
    crate::interface_cache::with_cache(out_dir, || {
        crate::source::with_walk_cache(|| generate_game_points_inner(root, out_dir))
    })
}

pub(super) fn generate_game_points_inner(root: &Path, out_dir: &Path) -> Result<(), String> {
    track(root, true)?;
    for path in super::points::tracked(root)? {
        crate::rerun_if_changed(&path.display());
    }
    crate::rerun_if_staged(root);
    println!("cargo:rerun-if-env-changed=HORIZON_POINT_NAMES");
    let selected = std::env::var("HORIZON_POINT_NAMES")
        .ok()
        .map(|json| {
            serde_json::from_str::<std::collections::BTreeSet<String>>(&json)
                .map_err(|error| format!("HORIZON_POINT_NAMES: {error}"))
        })
        .transpose()?;
    let points = super::points::game_points(root)?;
    let staged =
        root.join(".horizon-source-staged").is_file() || cfg!(feature = "listener-fixture");
    for (name, code) in [
        (
            "game_records.rs",
            super::points::game_records(&points, selected.as_ref()),
        ),
        (
            "game_catalog.rs",
            super::points::game_catalog(&points)?
                + &format!(
                    "/// Whether this build compiled point sites.\npub const STAGED: bool = {staged};\n"
                ),
        ),
    ] {
        let parsed =
            syn::parse_file(&code).map_err(|error| format!("generated Rust: {name}: {error}"))?;
        let mut formatted = String::new();
        for item in parsed.items {
            formatted.push_str(&item.to_token_stream().to_string());
            formatted.push('\n');
        }
        write_if_changed(out_dir.join(name), formatted).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(super) fn generate_points_inner(root: &Path, out_dir: &Path) -> Result<(), String> {
    track(root, false)?;
    // Phase durations for CI's build report (points-timing.txt beside the output); never read back.
    let started = std::time::Instant::now();
    let mut timing = String::new();
    let mut phase = |name: &str| {
        timing.push_str(&format!("{name} {:.2}s\n", started.elapsed().as_secs_f64()));
    };
    let src = root.join("crates/horizon-client/src");
    crate::rerun_if_changed(&src.join("game.rs").display());
    crate::rerun_if_changed(&src.join("game").display());
    for path in super::points::tracked(root)? {
        crate::rerun_if_changed(&path.display());
    }
    phase("tracked");
    println!("cargo:rerun-if-env-changed=HORIZON_POINT_NAMES");
    let selected = std::env::var("HORIZON_POINT_NAMES")
        .ok()
        .map(|json| {
            serde_json::from_str::<std::collections::BTreeSet<String>>(&json)
                .map_err(|error| format!("HORIZON_POINT_NAMES: {error}"))
        })
        .transpose()?;
    // Only a staged copy compiles the points' sites into the game; the wasm host's listener
    // fixture brings its own.
    crate::rerun_if_staged(root);
    let staged =
        root.join(".horizon-source-staged").is_file() || cfg!(feature = "listener-fixture");
    let points = super::points::game_points(root)?;
    phase("game_points");
    let code = super::points::records_selected(&points, selected.as_ref())
        + &format!(
            "/// Whether this build compiled the points' sites into the game: only the prebuilt game, built from a staged copy of the source, has them. A build without them refuses every listener at load.\npub const STAGED: bool = {staged};\n"
        );
    phase("records");
    let generated = syn::parse_file(&code).map_err(|e| format!("generated Rust: points: {e}"))?;
    // A single multi-megabyte line makes Clippy's source lookups prohibitively slow.
    let mut code = String::new();
    for item in generated.items {
        code.push_str(&item.to_token_stream().to_string());
        code.push('\n');
    }
    phase("format");
    write_if_changed(out_dir.join("points_generated.rs"), code).map_err(|e| e.to_string())?;
    phase("written");
    fs::write(out_dir.join("points-timing.txt"), timing).map_err(|e| e.to_string())
}

pub(super) fn track(root: &Path, all_game: bool) -> Result<(), String> {
    fn directory(path: &Path, extension: &str, recursive: bool) -> Result<(), String> {
        // Track the directory as well: adding a helper source or fragment changes the inputs.
        crate::rerun_if_changed(&path.display());
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() && recursive {
                directory(&path, extension, recursive)?;
            } else if path.extension().is_some_and(|ext| ext == extension) {
                crate::rerun_if_changed(&path.display());
            }
        }
        Ok(())
    }
    crate::rerun_if_changed(&root.join("crates/horizon-mod-vocab/src").display());
    for retired in super::catalog::RETIRED {
        // Only while present: Cargo reruns a build script on every build for a path that doesn't
        // exist. One that appears fails the next generation, and every fresh build, such as CI's.
        let path = root.join(retired);
        if path.exists() {
            crate::rerun_if_changed(&path.display());
        }
    }
    directory(&root.join(super::catalog::DECLARATIONS), "rs", false)?;
    directory(&root.join(super::exports::DIRECTORY), "rs", false)?;
    directory(
        &root.join("crates/horizon-mod-vocab/src/parts"),
        "rs",
        false,
    )?;
    // Only the game owner reads and validates native implementations. Lower generation reads
    // canonical vocabulary declarations, including each native bridge signature.
    let catalog = Catalog::load(root)?;
    if all_game {
        directory(&root.join("crates/horizon-client/src/game"), "rs", true)?;
        crate::rerun_if_changed(&root.join("crates/horizon-client/src/game.rs").display());
    }
    for hook in catalog.hooks {
        if let Some((krate, path)) = free_function_source(&hook.target) {
            let (module, _) = path.rsplit_once("::").ok_or("target missing module")?;
            let path = root
                .join("crates")
                .join(krate)
                .join("src")
                .join(format!("{}.rs", module.replace("::", "/")));
            crate::rerun_if_changed(&path.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tracking_tests {
    use super::*;

    fn tracked(root: &Path, all_game: bool) -> Vec<String> {
        crate::CAPTURED_RERUNS.with_borrow_mut(|capture| *capture = Some(Vec::new()));
        let result = track(root, all_game);
        let paths = crate::CAPTURED_RERUNS.with_borrow_mut(|capture| capture.take().unwrap());
        result.unwrap();
        paths
    }

    #[test]
    fn lower_tracking_reads_declarations_not_game_implementations() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        for directory in [
            super::super::catalog::DECLARATIONS,
            super::super::exports::DIRECTORY,
            "crates/horizon-mod-vocab/src/parts",
            "crates/horizon-client/src/game/native_hooks",
            "crates/horizon-client/src/game/blocks",
        ] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        let hook = root.join("crates/horizon-client/src/game/native_hooks/one.rs");
        fs::write(&hook, "").unwrap();
        let game = root.join("crates/horizon-client/src/game");
        let beacon = game.join("blocks/beacon.rs");
        fs::write(&beacon, "fn implementation() {}").unwrap();
        fs::write(game.with_extension("rs"), "").unwrap();

        let inputs = tracked(root, false);
        assert!(!inputs.contains(&hook.display().to_string()));
        assert!(!inputs.contains(&game.display().to_string()));
        assert!(!inputs.contains(&beacon.display().to_string()));
        assert!(!inputs.contains(&game.with_extension("rs").display().to_string()));

        let client_inputs = tracked(root, true);
        assert!(client_inputs.contains(&hook.display().to_string()));
        assert!(client_inputs.contains(&beacon.display().to_string()));
        assert!(client_inputs.contains(&game.with_extension("rs").display().to_string()));

        // Named hooks also derive from the declaration: logical/body validation belongs to
        // the game owner, not the lower API dependency graph.
        fs::write(
            root.join(super::super::catalog::DECLARATIONS).join("one.rs"),
            "pub trait Hooks { /// Doc.\n #[hook(side = \"server\", contract = \"listen\", target = \"horizon_client::game::blocks::Game<'_>::native_one\")] fn one(); }",
        )
        .unwrap();
        let named_inputs = tracked(root, false);
        assert!(!named_inputs.contains(&beacon.display().to_string()));
        assert!(!named_inputs.contains(&game.with_extension("rs").display().to_string()));
        assert!(
            named_inputs.contains(
                &root
                    .join(super::super::catalog::DECLARATIONS)
                    .join("one.rs")
                    .display()
                    .to_string()
            )
        );
    }

    #[test]
    fn canonical_bridges_need_no_game_files_but_refuse_missing_or_mismatched_signatures() {
        let root = tempfile::tempdir().unwrap();
        let declaration = r#"pub trait Hooks {
            /// One moment.
            #[hook(side = "server", contract = "listen", signature = "fn native_one(&self)")]
            fn one();
        }"#;
        let catalog = Catalog::parse(&[("one.rs".into(), declaration.into())]).unwrap();
        let generated = crate::dispatch::declared(root.path(), &catalog).unwrap();
        assert!(generated.traits.contains("fn one("));
        assert!(!root.path().join("crates/horizon-client").exists());

        let missing = declaration.replace(", signature = \"fn native_one(&self)\"", "");
        let missing = Catalog::parse(&[("one.rs".into(), missing)]).unwrap();
        let error = crate::dispatch::declared(root.path(), &missing)
            .err()
            .unwrap();
        assert!(error.contains("one: declare its native bridge"));

        let implementation: syn::ItemFn = syn::parse_quote!(
            fn native_one(&self, value: u64) {}
        );
        let error =
            crate::dispatch::emit(root.path(), catalog.hooks.iter(), &[implementation], &[])
                .err()
                .unwrap();
        assert!(error.contains("one: native signature differs"));
    }
}
