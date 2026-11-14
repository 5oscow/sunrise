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

The command returns a `developer-…` tag. Select that exact tag and pin an instance to it.
Automatic updates do not replace a local unsigned base. Remove it with
`horizon-launcher remove DEVELOPER_TAG` when no instance needs it.

## Its resources come from the checkout

A base needs Minecraft's own files prepared under `.local/horizon-client/<version>` in its folder.
When the checkout has them already — `horizon-prepare --root CHECKOUT` made them, or you started the
checkout's game once — `developer-install` copies that folder beside the base, and the base is ready
to play: no official launcher install, no signed-in account, and no separate `prepare`, which would
stop at `no Minecraft 26.3 client jar` on a machine that has neither (#4315).

It is a copy, not a link. Preparing the base again rewrites only the base's files, and the base
keeps working once the checkout moves or is deleted. `horizon-data.bin` is left out because it names
the folder it was written for: the game writes the base's own on its first start.

When the checkout had nothing prepared yet, there are two ways:

- prepare the base as before: `horizon-launcher prepare --version DEVELOPER_TAG --minecraft <the game folder of your launcher>`;
- or prepare the checkout's folder with `horizon-prepare --root CHECKOUT` and repeat the same
  `developer-install` command. The tag comes from the checkout's committed sources, so it does not
  change and nothing is installed twice; the resources are copied then.

By hand, that is a copy of `CHECKOUT/.local/horizon-client/<version>` to
`<launcher folder>/versions/DEVELOPER_TAG/.local/horizon-client/<version>`, which is what
`developer-install` does for you.

A native mod build needs the prebuilt [SDK](../../reference/sdk.md) of the base's exact commit. For a commit without a
published SDK, build it from the same checkout and name its folder; one folder holds one side:

```sh
python3 tools/prebuilt_game.py build SDK_FOLDER            # add --server for a dedicated server
HORIZON_LAUNCHER_DEVELOPER_SDK=SDK_FOLDER horizon-launcher native-build MODS --version DEVELOPER_TAG --trust-native-code
```

The variable is read for developer bases only, and the folder is used where it is. It must be
that commit's build for this computer, side and the launcher's Rust, as a downloaded SDK must;
anything else is refused. Without it, the launcher uses the commit's SDK release when one is
published, and otherwise says the build has no mod tools. The native mod builder never compiles the checkout itself. For native mixins, the pinned
source must also be available for copying the target functions.
