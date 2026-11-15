# Install a local unsigned developer base

`developer-install` installs a game executable built from a trusted checkout. This is a separate
launcher installation command, not a native mod build route. It runs the checkout's packaging
code with your rights and requires `--trust-unsigned`.

Commit tracked changes, build the game and companion programs, generate the wasm standard library
beside the game, and prepare compatible exporters from your own Minecraft installation before
installing. The packager copies `horizon-wasm-std.wasm` from the game's folder for every developer
base, whether the mods you build are native or wasm; the command names it if it is missing:

```sh
cargo run --locked -p cargo-horizon-mod -- wasm-std CHECKOUT/target/debug/horizon-wasm-std.wasm
horizon-launcher developer-install CHECKOUT --program CHECKOUT/target/debug/horizon-client --trust-unsigned
```

The install needs these files. The checkout's packager (`cargo run -p xtask -- package`, and
`package-server` for a server folder) checks them all before it writes anything, and names every
missing one in one message with the command that makes it. Its list is
`crates/horizon-mod-package/src/sdk/inputs.json`; a test keeps this table equal to it. In the
commands, `PROFILE` is the cargo flag of the profile the game program was built with (`--release`
for `target/release`, nothing for `target/debug`), `PROGRAM_FOLDER` its folder, and on Windows the
programs end in `.exe`:

<!-- required inputs: crates/horizon-mod-package/src/sdk/inputs.json, checked by a test -->
| File | Where | Needed by | How to make it |
|---|---|---|---|
| `horizon-ctl` | beside the game program | `package` | cargo build PROFILE -p horizon-client-lib --bin horizon-ctl |
| `horizon-prepare` | beside the game program | `package`, `package-server` | cargo build PROFILE -p horizon-prepare |
| `horizon-server` | beside the game program | `package`, `package-server` | cargo build PROFILE -p horizon-server |
| `horizon-wasm-std.wasm` | beside the game program | `package`, `package-server` | cargo run --locked -p cargo-horizon-mod -- wasm-std PROGRAM_FOLDER/horizon-wasm-std.wasm |
| `exporters/manifest.json` | in the checkout | `package`, `package-server` | horizon-prepare compile-exporters --root CHECKOUT, then copy its server, client and manifest.json into CHECKOUT/exporters/ |
<!-- end of required inputs -->

The command returns a `developer-…` tag. Select that exact tag, prepare its resources, and pin an
instance to it. Automatic updates do not replace a local unsigned base. Remove it with
`horizon-launcher remove DEVELOPER_TAG` when no instance needs it.

A native mod build needs the prebuilt [SDK](../../reference/sdk.md) of the base's exact commit. For a commit without a
published SDK, build it from the same checkout and name its folder; one folder holds one side:

```sh
python3 tools/prebuilt_game.py build SDK_FOLDER --developer # add --server for a dedicated server
HORIZON_LAUNCHER_DEVELOPER_SDK=SDK_FOLDER horizon-launcher native-build MODS --version DEVELOPER_TAG --trust-native-code
```

The launcher reads the variable for developer bases only, and uses the folder where it is. It must be
that commit's build for this computer, side and the launcher's Rust, as a downloaded SDK must;
anything else is refused. Without it, the launcher uses the commit's SDK release when one is
published, and otherwise says the build has no mod tools. The native mod builder never compiles the checkout itself. For native mixins, the pinned
source must also be available for copying the target functions.

## Cargo tooling and editor sources

`--developer` also generates `SDK_FOLDER.developer-tools` beside the compiled SDK. It reuses
`tools/sdk_api_source.py` for the complete standalone API workspace, including generated code,
source dependencies and its lockfile, and the existing surface and SDK-index producers. Keep
both folders together. Published compiled-slice packaging is unchanged. Source generation
requires a POSIX host and the `wasm32-unknown-unknown` Rust target, just like the standalone
source-SDK builder. Before rebuilding, remove the old `SDK_FOLDER.developer-tools` folder.
The source collector checks rustc's dep-info against Cargo's declared build-script outputs.
Use unwrapped Cargo for this step if a compiler cache rewrites generated reads outside the
reported `OUT_DIR`; those paths are deliberately rejected.

In the mod's `Cargo.toml`, explicitly opt in:

```toml
[package.metadata.horizon]
developer-sdk = true
```

Keep the full commit SHA in `[native] horizon` in `horizon-mod.toml` (or the existing
`package.metadata.horizon.sdk` pin for wasm). This flag is project build-workflow metadata,
not a runtime compatibility mode or a change to mod dependencies.

Use an absolute SDK path so the launcher, Cargo and your editor resolve the same folder:

```sh
# In the mod folder:
export HORIZON_LAUNCHER_DEVELOPER_SDK=/absolute/path/to/SDK_FOLDER
cargo horizon-mod check
cargo horizon-mod build
cargo horizon-mod hooks player_logged_in
cargo horizon-mod fits
cargo horizon-mod update
cargo check
```

These commands share the pinned-SDK resolver. It checks the actual prebuilt identity,
commit, host target, side, package format, Rust release and build-data headers, verifies the
source archive against the generated index, then stages the API workspace under
`.horizon/sdk/api/`. Normal Cargo and editor imports use the template's unchanged
`.horizon/sdk/api/crates/horizon-mod-api` path. Start the editor after the first SDK-resolving
command; it needs no manual source copying or dependency-path repair.

In this explicit developer workflow, the default `nightly` target of `fits` and `update`
means the project's exact pinned local build, not the public nightly. Explicit target builds
must also match the supplied local SDK; unrelated published or local builds are refused.
`update` preserves the opt-in and leaves sources staged for Cargo. To move to a different
developer commit, update the full SHA pin and build/select that commit's matching SDK.

Unsigned local sources are accepted only with **both** `developer-sdk = true` and the explicit
developer-SDK environment variable. Missing or incompatible local output is an error, never
a silent remote fallback. With the flag absent or false, the environment variable has no
effect: the existing signed-release verification remains mandatory. Developer staging never
writes a signed-cache stamp, so removing the opt-in cannot reuse an unsigned SDK as a release.
