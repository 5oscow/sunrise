# Publishing your mod

Sunrise's registry, [sunrise-org/sunrise-mods](https://github.com/sunrise-org/sunrise-mods), lists mods, modpacks,
resource packs and shader packs made for Sunrise. Players find them at [playsunrise.net/mods](https://playsunrise.net/mods)
and in the launcher; each file is checked before it is installed. Listing a project is a pull request that a person
reviews. `cargo sunrise-mod publish` writes everything that pull request holds; it never uploads and never opens the pull
request for you.

## 1. Build and write the submission

In your mod's folder (content, native or wasm; add `--wasm` for a wasm mod, as for `build`):

```sh
cargo sunrise-mod publish --summary "One line about the mod." --owner your-github-login --categories utility
```

It builds the package as `build` does, then writes `target/sunrise-mod/publish/` (change it with `--out <dir>`):

| Path | What it is |
| --- | --- |
| `mods/<id>.toml` | The registry entry, filled from `sunrise-mod.toml`: name, description, authors, license, `homepage` as the source, and the version's file name, SHA-256 and date |
| `<id>-<version>.smod` | The package: attach exactly this file to a release of your repository |
| `files/<sha256>` | The same file under the name the registry's checks read |
| `icons/<id>.png` | Your `icon.png`, when the project has one beside `sunrise-mod.toml` |
| `PULL_REQUEST.md` | The text of the pull request |

What the manifest cannot say comes from flags: `--summary` (at most 160 characters), `--owner <login>` (repeat it for
several; the GitHub accounts allowed to publish updates), `--categories a,b` (1 to 3 from [your kind's
list](../reference/registry-index.md#submitting-a-project)), `--tags a,b` and `--changelog <text>`. Anything still
missing is written as a value that starts with `TODO(publish)`, and the command lists what remains. The registry
refuses an entry that holds one, so edit `mods/<id>.toml` (or the manifest, then run `publish` again) until the list is empty.

The icon is a square PNG of 64 to 1024 pixels and at most 256 KiB, as `icon.png` beside `sunrise-mod.toml`; `publish`
says what is wrong with one that does not fit.

## 2. Check it as the registry will

`publish` prints the two commands, run from your mod's folder:

```sh
cargo sunrise-mod registry check target/sunrise-mod/publish/base target/sunrise-mod/publish --files target/sunrise-mod/publish/files
cargo sunrise-mod registry build target/sunrise-mod/publish --files target/sunrise-mod/publish/files --out target/sunrise-mod/publish/index.json
```

The first tests the file (its kind's rules, its signature), the second holds the entry, the icon and the file to every
rule of the index, which is what the registry's CI runs. `registry build` fails until the `TODO(publish)` values are gone.

## 3. Open the pull request

1. Create a release of your repository and attach `<id>-<version>.smod` to it. Its address is the artifact URL.
2. Fork [sunrise-org/sunrise-mods](https://github.com/sunrise-org/sunrise-mods) and copy in `mods/<id>.toml` and
   `icons/<id>.png`. The registry's own folder layout is the output folder's, so nothing else changes.
3. Commit with `git commit -s`: every commit needs a `Signed-off-by:` line.
4. Open the pull request with the text of `PULL_REQUEST.md`, after pasting the artifact URL into it.

## 4. What review and staging do

- A maintainer copies your file from the artifact URL to the registry's `staging` release, named
  `<sha256>.smod`. Nothing is fetched from an address in a pull request on its own, and you cannot stage a file yourself.
- The `check` workflow then downloads each listed file, compares its SHA-256 with the entry's, and runs
  `registry review`, `registry check` and `registry build` on the result.
- A reviewer reads the change and merges it. The `publish` workflow then releases the file as `<id>-<version>` and
  adds the project to the index.
- The hash is checked again by the launcher before any download reaches the game folder, so a file changed after
  review is refused.

## 5. A new version

Raise `version` in `sunrise-mod.toml`, then run `publish` with `--registry <your checkout of sunrise-mods>`. It reads
`mods/<id>.toml` there, adds the new version at the top of its list and writes the entry to the output folder;
the other fields stay as listed. Published versions never change and are never removed: a version already in the entry
is refused by name, and a file that must go is taken down by the registry's maintainers in `revoked.toml`. Then repeat
steps 2 to 4 (the check's first command takes your checkout in place of `.../publish/base`).

## Signing

Authors may sign with an Ed25519 key in [minisign](https://jedisct1.github.io/minisign/) format. The entry then
holds the public key, every version of the project must be signed with it, and an update signed with another key is
refused. `cargo sunrise-mod` verifies a signature but does not make keys or signatures: use `minisign`.

```sh
minisign -G -p my_mod.pub -s my_mod.key           # once: a key pair
cargo sunrise-mod publish --summary "..." --owner you --categories utility
minisign -S -s my_mod.key -m target/sunrise-mod/publish/<id>-<version>.smod
cargo sunrise-mod publish --summary "..." --owner you --categories utility \
  --key "$(tail -n 1 my_mod.pub)" --signature target/sunrise-mod/publish/<id>-<version>.smod.minisig
```

Packages are built the same way each time, so the second `publish` makes the file that was signed; it checks the
signature and writes it into the entry. Keep the secret key (`my_mod.key`) out of your repository and out of any
release; losing it means a new key and a review.

## Integrity and takedowns

- **Integrity.** Every download is checked against the SHA-256 the index lists before the file reaches the game
  folder. Unsigned local files stay allowed, with an "unsigned" mark.
- **Takedowns.** A file the registry takes down (`revoked.toml`) is refused by the launcher and turned off where
  installed. What a listed project agrees to is in the registry's `MOD-POLICY.md`.

Once it is published, its page and its card in the launcher show its rating: players signed in with GitHub on its
page give it 1 to 5 stars, one rating per account, which they can change. Its owners can't rate it, and nobody sees
who rated.
