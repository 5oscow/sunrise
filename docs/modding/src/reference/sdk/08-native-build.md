# `native-build`

`cargo sunrise-mod native-build` compiles native mods against one build's SDK exactly as the launcher does: both run
the same code (`crates/sunrise-native-build`). Every SDK release carries the tool for each host it is built for, as
`tool-<target>.tar.xz` holding `cargo-sunrise-mod` (`.exe` on Windows), so a caller that already checked the SDK's
signed `sdk.json` gets a tool made from that build's own source.

The caller does what needs a player or their game: it checks that native code is allowed, fetches and verifies the
SDK's slices, installs the pinned Rust, chooses the mods and which of them a build leaves out, and installs what the
tool built. The tool compiles, and nothing else.

It reads a plan from the first line of standard input, writes one JSON event per line to standard output, and appends
its log to `build.log` in the plan's `work` folder: emptying it before a build is the caller's, as the launcher's own
lines about the build go there first.

```sh
cargo sunrise-mod native-build < plan.json
```

| Exit code | Meaning |
|---|---|
| 0 | It built: the last line is a `built` event. |
| 1 | It did not: the last line is a `failed` event. |
| 2 | No plan: the first line is not a plan of this contract (not one JSON object, a field missing, wrong or unknown). Standard error says why. |
| 3 | Another contract: the plan's `contract` isn't `2`. It is read before any other field, so a newer plan is refused for its number, never for a field this tool doesn't know. Standard error names both contracts. |

## The plan

One JSON object on one line. Its `contract` is checked first: anything but `2` exits 3. Then a field the tool
doesn't know is refused (exit 2).

| Field | What it is |
|---|---|
| `contract` | `2`. A change to any field or event below takes a new number. |
| `work` | The build's own folder: `build.log`, the horizon-mixins tool, the game source a mixin patch copies from, and kept compiles under `prebuilt-build/`. The launcher passes its `native/` folder, so the kept compiles are the ones its in-process build made. |
| `toolchain` | `dir` (holding `bin/cargo` and `bin/rustc`), `target` (its host triple), `cargo_home`, `cargo_config` (`--config` values every cargo command gets; the launcher's Windows ARM64 flags) and `c_bin` (the folder of its C and link programs, when it brings them). The Rust release must be the SDK's (`sdk.json`'s `toolchain.rust`). |
| `package` | `runtime`, `api` and `engine`: the folders the SDK's three slices for this host and side are unpacked into. The API route reads only `runtime` and `api`. |
| `mods` | A mod's folder, or a folder of mod folders, read in path order. Every mod in it is compiled: a caller leaves a mod out by not putting it there. |
| `target` | `client` (the game) or `server` (the headless engine). |
| `registry` | Optional. `true` or `false` answers in advance whether registry crates may be added (their build scripts and procedural macros run while compiling). Absent, the tool asks with a `consent` event. |
| `mixins_from_source` | Optional, `false` by default. Builds horizon-mixins from its pinned source instead of downloading its release. |

## The events

Each line is an object whose `event` names it.

| Event | Fields | When |
|---|---|---|
| `mod` | `id`, `done`, `total` | A mod's own library starts compiling, `done` of `total`. |
| `consent` | `added_crates`, `build_scripts`, `proc_macros` | Registry crates need a yes and the plan gave no `registry`. The caller answers with one line on standard input: `true` agrees, anything else refuses. |
| `built` | `route`, `binary`, `libraries`, `records`, `mods`, `internals`, `mixins`, `used` | The executable and the libraries to install beside it, in load order; `records` is each library's identity, the build record's `libraries`; `route` is `prebuilt-api` or `prebuilt`, as the record names it; `mods` is the compiled mods in dependency order. `internals` has each compiled mod's `[[internals]]` by mod id, every absent `hash` taken from the SDK's catalog, as the build record keeps them; `mixins` is the horizon-mixins revision the build ran; `used` lists the kept folders and files under `work` it used, for a caller's cache to count as used. |
| `failed` | `error`, `blame`, `used` | Why it did not build, as the build log ends with it. `blame` is `null`, or the mod the error is that mod's own and how: `{"mod": id, "kind": "compile" \| "build-script" \| "features"}`, the one a caller leaves out next time. `used` as for `built`. |

What it downloads (the horizon-mixins tool, a mod's GitHub source) goes through the proxy its environment names:
`HTTPS_PROXY`, `HTTP_PROXY` or `ALL_PROXY`, with the hosts `NO_PROXY` lists reached directly, as the launcher reads
them. A caller with a proxy setting of its own passes it in those variables.

The tool never edits an instance, never selects a version and never installs anything outside `work`.

## `native-check`

`cargo sunrise-mod native-check` says how a set of native mods stands against one build before anything compiles:
each mod's refusal, or its fit (SDK design §10), and the route a build of the usable ones takes. It reads one plan
line on standard input and writes one reply line. Exit codes are those of `native-build`, except that 1 means a file
the plan names didn't read (standard error says which).

```sh
cargo sunrise-mod native-check < plan.json
```

| Plan field | What it is |
|---|---|
| `contract` | `2`, checked first as for `native-build`. |
| `mods` | A mod's folder, or a folder of mod folders. |
| `build` | The build id (the game's full commit) the mods are checked against. |
| `surface` | Optional. The build's `surface.json`, read only for a mod made for another build. |
| `build_data` | Optional. The build's unpacked build data (`points.json`, `internals.json`), read only for a mod with `[[inject]]` rows made for another build. |

The reply is `{"contract": 2, "route": …, "mods": […]}`. `route` is `prebuilt-api` or `prebuilt` for the usable
mods, `null` when none is. Each of `mods` has:

| Field | What it is |
|---|---|
| `id` | The mod's id. |
| `refusal` | Why no build takes it, as the build log says it; then `verdict` is `null`. |
| `verdict` | `{"kind": …, "made_for": build}`: `made-for` (no `made_for`), `fits`, `exact` (`why`: the table that changes game code, or `internals`), `no-list` (its package doesn't say what it uses), `does-not-fit` (`misfits`: each `{id, change}`, the surface id and how it changed), `points` (`why`: an `[[inject]]` point that isn't the same). `made-for` and `fits` are usable. |
| `needs` | `surface` or `build-data` when the plan didn't give the file its verdict needs; then `verdict` is `null`. The caller fetches it and asks again. |

A `change` is `"removed"`, `"changed"`, `{"lost-impl": trait}`, `{"fields": {gained, at}}`, `{"version": {named, now}}`,
`{"not-buildable": {at}}`, `{"path": path}` or `{"route": {from, owner}}`.

## Changelog

- contract 2: `native-check`; `failed` names the blamed mod (`blame`) instead of a caller matching its text; `built` adds `internals` and `mixins` for the build record; both list the kept files they `used`.
- contract 1: the plan, `mod`, `consent`, `built` and `failed`.
