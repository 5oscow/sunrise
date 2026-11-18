# Install the mod tool

Every mod starts with `cargo sunrise-mod`, the tool that makes a project, fetches the mod SDK, checks the mod, builds it
and packs it. The blocks below install the newest tool built from main, from the rolling `mod-tool` release: a fix to
the tool reaches you when it merges, with no wait for a Nightly. Each SDK release also ships the tool it was built with
([which to take](#which-tool-to-take)).

You also need Rust 1.95 or newer (`cargo`, from [rustup.rs](https://rustup.rs)). The tool runs `cargo` itself.

## The prebuilt tool

The `mod-tool` release holds `tool-<target>.tar.xz` for each of the three computers, `tool.json` (the commit the tool
is built from, its date and its `native-*` contract number), `SHA256SUMS` and its signature. Each block below downloads
them, stops unless Sunrise's key signed the sums for the tag `mod-tool` and the tool's and `tool.json`'s hashes are in
them, and only then puts `cargo-sunrise-mod` on your `PATH`. The key in each block is the one in
[`release-keys.txt`](https://github.com/sunrise-org/sunrise/blob/main/crates/sunrise-release-sign/release-keys.txt); the signature is checked with OpenSSL
(3.0 or newer, for `pkeyutl -rawin`), which is what [how a download is checked](../reference/sdk/01-release.md#how-a-download-is-checked)
describes. macOS's own `openssl` cannot check it: the block uses Homebrew's (`brew install openssl@3`). On Windows
it uses the `openssl.exe` that Git for Windows installs.

### Linux (x86_64)

```sh
(
  set -eu
  BASE=https://github.com/sunrise-org/sunrise/releases/download
  KEY=r53xb3cuafGxZSicLz06KC+xXp9NJdPH8snNmXo5pAc=
  OPENSSL=openssl
  TOOL=tool-x86_64-unknown-linux-gnu.tar.xz
  cd "$(mktemp -d)" || exit 1
  for FILE in "$TOOL" tool.json SHA256SUMS SHA256SUMS.sig; do
    curl -fsSLO "$BASE/mod-tool/$FILE" || exit 1
  done
  printf 'horizon release mod-tool\n' | cat - SHA256SUMS > message
  { printf '\060\052\060\005\006\003\053\145\160\003\041\000'; printf '%s' "$KEY" | "$OPENSSL" base64 -d -A; } > key.der
  signed=
  while read -r line || [ -n "$line" ]; do
    printf '%s' "$line" | "$OPENSSL" base64 -d -A > signature
    "$OPENSSL" pkeyutl -verify -rawin -pubin -keyform DER -inkey key.der -in message -sigfile signature > /dev/null 2>&1 && signed=1
  done < SHA256SUMS.sig
  [ -n "$signed" ] || { echo "SHA256SUMS is not signed by Sunrise" >&2; exit 1; }
  grep " $TOOL\$" SHA256SUMS | sha256sum -c - || { echo "$TOOL does not match SHA256SUMS" >&2; exit 1; }
  grep " tool.json\$" SHA256SUMS | sha256sum -c - || { echo "tool.json does not match SHA256SUMS" >&2; exit 1; }
  tar -xJf "$TOOL" || exit 1
  install -Dm755 cargo-sunrise-mod "$HOME/.local/bin/cargo-sunrise-mod"
)
export PATH="$HOME/.local/bin:$PATH"
```

The `export` lasts for this shell. Add `export PATH="$HOME/.local/bin:$PATH"` to your shell's startup file to keep it.

### macOS (Apple silicon)

```sh
(
  set -eu
  BASE=https://github.com/sunrise-org/sunrise/releases/download
  KEY=r53xb3cuafGxZSicLz06KC+xXp9NJdPH8snNmXo5pAc=
  OPENSSL="$(brew --prefix openssl@3)/bin/openssl"
  TOOL=tool-aarch64-apple-darwin.tar.xz
  cd "$(mktemp -d)" || exit 1
  for FILE in "$TOOL" tool.json SHA256SUMS SHA256SUMS.sig; do
    curl -fsSLO "$BASE/mod-tool/$FILE" || exit 1
  done
  printf 'horizon release mod-tool\n' | cat - SHA256SUMS > message
  { printf '\060\052\060\005\006\003\053\145\160\003\041\000'; printf '%s' "$KEY" | "$OPENSSL" base64 -d -A; } > key.der
  signed=
  while read -r line || [ -n "$line" ]; do
    printf '%s' "$line" | "$OPENSSL" base64 -d -A > signature
    "$OPENSSL" pkeyutl -verify -rawin -pubin -keyform DER -inkey key.der -in message -sigfile signature > /dev/null 2>&1 && signed=1
  done < SHA256SUMS.sig
  [ -n "$signed" ] || { echo "SHA256SUMS is not signed by Sunrise" >&2; exit 1; }
  grep " $TOOL\$" SHA256SUMS | shasum -a 256 -c - || { echo "$TOOL does not match SHA256SUMS" >&2; exit 1; }
  grep " tool.json\$" SHA256SUMS | shasum -a 256 -c - || { echo "tool.json does not match SHA256SUMS" >&2; exit 1; }
  tar -xJf "$TOOL" || exit 1
  install -d "$HOME/.local/bin"
  install -m755 cargo-sunrise-mod "$HOME/.local/bin/cargo-sunrise-mod"
)
export PATH="$HOME/.local/bin:$PATH"
```

Add the `export` line to `~/.zshrc` to keep it.

### Windows (x64, PowerShell)

```powershell
& {
  $ErrorActionPreference = "Stop"
  $Base = "https://github.com/sunrise-org/sunrise/releases/download"
  $Key = "r53xb3cuafGxZSicLz06KC+xXp9NJdPH8snNmXo5pAc="
  $OpenSsl = Join-Path $env:ProgramFiles "Git\usr\bin\openssl.exe"
  $Tool = "tool-x86_64-pc-windows-gnu.tar.xz"
  $Dir = (New-Item -ItemType Directory (Join-Path $env:TEMP "sunrise-tool-$PID")).FullName
  $At = { param($Name) Join-Path $Dir $Name }
  foreach ($Name in $Tool, "tool.json", "SHA256SUMS", "SHA256SUMS.sig") {
    Invoke-WebRequest "$Base/mod-tool/$Name" -OutFile (& $At $Name)
  }
  $Prefix = [byte[]](0x30,0x2a,0x30,0x05,0x06,0x03,0x2b,0x65,0x70,0x03,0x21,0x00)
  [IO.File]::WriteAllBytes((& $At "key.der"), $Prefix + [Convert]::FromBase64String($Key))
  $Head = [Text.Encoding]::UTF8.GetBytes("horizon release mod-tool`n")
  [IO.File]::WriteAllBytes((& $At "message"), $Head + [IO.File]::ReadAllBytes((& $At "SHA256SUMS")))
  $Signed = $false
  foreach ($Line in Get-Content (& $At "SHA256SUMS.sig")) {
    if ($Line.Trim() -eq "") { continue }
    [IO.File]::WriteAllBytes((& $At "signature"), [Convert]::FromBase64String($Line.Trim()))
    & $OpenSsl pkeyutl -verify -rawin -pubin -keyform DER -inkey (& $At "key.der") -in (& $At "message") -sigfile (& $At "signature") *> $null
    if ($LASTEXITCODE -eq 0) { $Signed = $true }
  }
  if (-not $Signed) { throw "SHA256SUMS is not signed by Sunrise" }
  foreach ($Name in $Tool, "tool.json") {
    $Want = ((Select-String -Path (& $At "SHA256SUMS") -Pattern "  $([regex]::Escape($Name))\s*$").Line -split " ")[0]
    if (-not $Want -or (Get-FileHash (& $At $Name) -Algorithm SHA256).Hash.ToLower() -ne $Want) { throw "$Name does not match SHA256SUMS" }
  }
  tar -xf (& $At $Tool) -C $Dir
  if ($LASTEXITCODE -ne 0) { throw "tar could not unpack $Tool" }
  $Bin = Join-Path $env:USERPROFILE ".sunrise\bin"
  New-Item -ItemType Directory -Force $Bin | Out-Null
  Move-Item -Force (& $At "cargo-sunrise-mod.exe") $Bin
  [Environment]::SetEnvironmentVariable("Path", "$Bin;" + [Environment]::GetEnvironmentVariable("Path", "User"), "User")
}
$env:Path = "$env:USERPROFILE\.sunrise\bin;$env:Path"
```

The `Path` change is saved for your user, so new terminals find the tool.

### Check it

```sh
cargo sunrise-mod --help
```

It prints the tool's usage, which shows it is installed:

```text
usage: cargo sunrise-mod new <path> [--name "Name"] [--template content|native|wasm [--sunrise <build>]] | update [--sunrise <build>] | …
```

A tool built before `--help` existed prints the same usage after `error:` and exits with 1; that also means it is installed. `error: no such command: sunrise-mod` means the folder with `cargo-sunrise-mod` is not on your `PATH`.

`cargo sunrise-mod --version` prints the commit the tool is built from and its `native-*` contract number. Compare the
commit with `commit` in the [`mod-tool` release's `tool.json`](https://github.com/sunrise-org/sunrise/releases/download/mod-tool/tool.json)
to see whether you have the newest. To move to a newer tool, run the same block again.

## Which tool to take

Take the rolling tool above unless you build for exactly one game build on a machine that must not change. The rolling
tool is newer than most SDKs and builds against them while their layout is one it knows. When a project pins an SDK
older than the tool can build against, `build` stops and says so, with two ways out: repin the project to a current
build (`cargo sunrise-mod update --sunrise nightly`), or take the tool that SDK shipped with. That one is the asset
`tool-<target>.tar.xz` of the release `sdk-<build>` (the build is the project's pin); a build server that wants the tool
of exactly the game build takes it from there too, with the same signature check as `sdk-<build>`'s own sums. Both tools
check every SDK they fetch against Sunrise's signature.

## With `cargo install`

If no prebuilt tool fits your computer, build it from source:

```sh
cargo install --locked --git https://github.com/sunrise-org/sunrise cargo-sunrise-mod
```

This compiles the tool and the crates it needs from the game repository, and puts it in
`~/.cargo/bin`, where `cargo` finds it. It builds the tool from the repository's newest commit, which can be newer
than the Nightly. A mod pinned to the Nightly still builds against the Nightly's SDK, and `new` and `update` write
the pins from that SDK, not from the tool.

## On NixOS

The prebuilt tool is a glibc binary linked for a standard Linux, which NixOS does not have. Run it through
`steam-run` (`steam-run cargo sunrise-mod …`), or turn on `nix-ld`, or use the `cargo install` path above. The tool
runs `cargo`, so Rust has to be installed in the same environment.

## Next

- [Your first native mod](first-mod.md)
- [Your first content mod](first-content-mod.md)
- [Your first WebAssembly mod](first-wasm-mod.md)
