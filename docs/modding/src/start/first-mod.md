# Your first native mod

A native mod is a Rust crate that implements hooks from the mod API. The launcher compiles it on your machine
against the exact Sunrise build an instance plays, then runs that instance with your mod in it. In this page you
make a mod that greets every player who joins, compile it, and see how it reaches the game.

You need Rust 1.95 or newer ([rustup.rs](https://rustup.rs)) and, to play the result, the Sunrise launcher
(`sunrise-launcher`) with an instance. The native build itself uses the launcher's own pinned Rust toolchain, not
yours.

## 1. Install the mod tool

Follow [Install the mod tool](install-tool.md) for your computer. When `cargo sunrise-mod` prints its usage, come
back here.

## 2. Make the project

```sh
cargo sunrise-mod new greeter --template native
cd greeter
```

```text
Created greeter (native mod id greeter). Your editor resolves sunrise-mod-api once the SDK is in .sunrise/sdk/: `cargo sunrise-mod check` fetches it in a fresh clone. Next: sunrise-launcher native-build greeter --instance <instance> --trust-native-code
SDK 2d81263 (build 2d8126336e579777eabf9b2fd8b3d476236a04f2) in greeter/.sunrise/sdk
```

The tool pins the project to the build of the current Nightly (or to `--sunrise <build>`) and fetches that build's
[SDK](../reference/sdk.md), checking its signature. It writes both pins in `sunrise-mod.toml`, the Sunrise build and
the `horizon-mixins` tool that build names, so you never write a commit by hand:

```text
greeter/
├── sunrise-mod.toml     # the manifest; [native] pins the exact Sunrise build
├── Cargo.toml           # depends on sunrise-mod-api in .sunrise/sdk/, never on the game
├── src/lib.rs           # the hooks
├── assets/
├── data/
└── .sunrise/sdk/        # the pinned SDK: fetched by the tool, never committed or packed
```

After a fresh clone, run `cargo sunrise-mod check` once before opening the project in an editor: it fetches
`.sunrise/sdk/` again. `cargo sunrise-mod update` moves both pins to the newer Nightly later.

## 3. Read the hook

`src/lib.rs` implements one method of `mod_api::Hooks`:

```rust,ignore
{{#include ../../examples/src/greeter.rs:mod}}
```

- Each method of `Hooks` is one moment in the game. You implement only the ones you need; the game calls
  your mod only for those.
- `#[mod_api::sunrise_mod]` marks the one `impl Hooks` at the crate root and subscribes the mod to exactly the
  methods it implements.
- The `host` is your handle on the game. A server hook gets a `ServerHost`, a client hook a `ClientHost`, so a call
  on the wrong side does not compile.
- The `event` carries the moment's data. Values you may change are writable fields of the event.

[Writing hooks](writing-hooks.md) explains these rules in full.

## 4. Check that it compiles

`check` compiles for wasm32 as well as for this computer, so install that target for the project's toolchain once:

```sh
rustup target add wasm32-unknown-unknown
cargo sunrise-mod check
```

`check` applies the package rules, then compiles the crate with `cargo check` against the pinned SDK's API source,
for this computer and for wasm32, and lists what the mod uses of its build. The expected end of the output (the compile runs twice, once for each target):

```text
    Checking mod-greeter v0.1.0 (…/greeter)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in …
greeter uses 34 items of the API (1 capability, 5 fields or variants, 1 hook, 2 hook arguments, 1 macro, 5 methods, 4 traits, 15 types). Made for 2d81263.
```

Run it after every edit. It is the quick loop; it does not link against the game.

## 5. Build against the prebuilt game

The real build compiles the crate against the prebuilt game of the pinned build and links it, as the launcher does.
The launcher's command makes that build the one an instance plays:

```sh
sunrise-launcher native-build . --instance "My Instance" --trust-native-code
```

The launcher downloads the prebuilt game for the pinned Sunrise build, compiles your crate against it, installs the
result as a game version of its own beside the base and selects that version for the instance. `--trust-native-code`
records that you allow native code on this install: a native mod runs with your rights, outside any sandbox. Start the
instance and join a world; the greeting appears in chat, and the mod is listed on the game's **Mods** screen. The
launcher's **Mods** page does the same from the window: put the project in the instance's `sunrise-native-mods/`
folder and choose **Build** ([Installing and using mods](installing.md)).

### Without the launcher

The same compile is a command of the tool, so it runs where there is no launcher: in a build server, or while you
check that a project builds. It takes a plan, one line of JSON on standard input ([the
contract](../reference/sdk/08-native-build.md)), and the SDK's `runtime`, `api` and `engine` slices, which you download once. `native-lock` runs first: it pins the
mod's registry dependencies against the game's, with a plan of its own.
This block is for Linux x86_64 and, from the project's folder, downloads the slices of the pinned build, checks
Sunrise's signature on their checksums and then each slice, installs the Rust the SDK was built with, and builds. It
needs `openssl` (1.1.1 or newer) and `python3`, and it stops at the first step that fails:

```sh
(
  set -eu
  BASE=https://github.com/sunrise-org/sunrise/releases/download
  KEY=r53xb3cuafGxZSicLz06KC+xXp9NJdPH8snNmXo5pAc=
  COMMIT=$(sed -n 's/^sunrise = "\(.*\)"/\1/p' sunrise-mod.toml)
  TARGET=x86_64-unknown-linux-gnu
  RUST=$(sed -n 's/.*"rust": *"\([^"]*\)".*/\1/p' .sunrise/sdk/sdk.json)
  rustup toolchain install "$RUST" --profile minimal
  B="${TMPDIR:-/tmp}/sunrise-native-build/$COMMIT"
  mkdir -p "$B/runtime" "$B/api" "$B/engine" "$B/work"
  curl -fsSL -o "$B/SHA256SUMS" "$BASE/sdk-$COMMIT/SHA256SUMS"
  curl -fsSL -o "$B/SHA256SUMS.sig" "$BASE/sdk-$COMMIT/SHA256SUMS.sig"
  (
    cd "$B"
    printf 'horizon release sdk-%s\n' "$COMMIT" | cat - SHA256SUMS > message
    { printf '\060\052\060\005\006\003\053\145\160\003\041\000'; printf '%s' "$KEY" | openssl base64 -d -A; } > key.der
    signed=
    while read -r line || [ -n "$line" ]; do
      printf '%s' "$line" | openssl base64 -d -A > signature
      openssl pkeyutl -verify -rawin -pubin -keyform DER -inkey key.der -in message -sigfile signature > /dev/null 2>&1 && signed=1
    done < SHA256SUMS.sig
    [ -n "$signed" ] || { echo "SHA256SUMS is not signed by Sunrise" >&2; exit 1; }
  )
  for slice in runtime api engine; do
    curl -fsSL -o "$B/$slice-$TARGET.tar.xz" "$BASE/sdk-$COMMIT/$slice-$TARGET.tar.xz"
    (cd "$B" && grep " $slice-$TARGET.tar.xz\$" SHA256SUMS | sha256sum -c -)
    tar -xJf "$B/$slice-$TARGET.tar.xz" -C "$B/$slice"
  done
  python3 - "$B" "$TARGET" "$RUST" "$PWD" <<'PY'
import json, subprocess, sys
b, target, rust, mods = sys.argv[1:]
sysroot = subprocess.check_output(["rustc", f"+{rust}", "--print", "sysroot"], text=True).strip()
plan = {
    "contract": 2,
    "work": f"{b}/work",
    "toolchain": {"dir": sysroot, "target": target, "cargo_home": f"{b}/cargo-home"},
    "mods": mods,
}
json.dump(plan, open(f"{b}/lock-plan.json", "w"))
plan["package"] = {"runtime": f"{b}/runtime", "api": f"{b}/api", "engine": f"{b}/engine"}
plan["target"] = "client"
plan["registry"] = False
json.dump(plan, open(f"{b}/plan.json", "w"))
PY
  cargo sunrise-mod native-lock < "$B/lock-plan.json"
  cargo sunrise-mod native-build < "$B/plan.json"
)
```

The tool prints one JSON event per line. A build that worked prints these two, the second cut short here:

```text
{"event":"mod","id":"greeter","done":1,"total":1}
{"event":"built","route":"prebuilt-api","binary":"…/runtime/bin/sunrise-client","libraries":["…/libmod_api.so", …, "…/libmod_greeter.so"], …}
```

The compiled mod is the last of `libraries`. The tool only compiles: installing the result into an instance, and
selecting it, is the launcher's step, so a mod built this way does not appear in the launcher until you run the
launcher's `native-build` on it. A build for another
computer needs that computer's target triple in `TARGET` (macOS arm64 is `aarch64-apple-darwin`, Windows x64 is
`x86_64-pc-windows-gnu`).

After an edit, build again. The [build log](../how-to/native-mods/05-build-log.md) shows the compiler's output when
a build fails.

## Next

- [Writing hooks](writing-hooks.md): the crate root, the hook factory, events and hosts.
- [Hooks and the Host](../concepts/hooks-and-the-host.md) and [Parts](../concepts/parts.md): how a mod reaches
  the game.
- [Your first WebAssembly mod](first-wasm-mod.md): the same code, sandboxed.
- [Native mods: building and shipping](../how-to/native-mods.md): what the launcher does, trust, multiplayer and
  servers.
- [Using an AI agent](../for-agents.md): the same steps on one page for a coding agent.
