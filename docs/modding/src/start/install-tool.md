# Install the mod tool

Every mod starts with `cargo sunrise-mod`, the tool that makes a project, fetches the mod SDK, checks the mod and
packs it. Each SDK release ships the tool for three computers, built from that release's own source. Take the one
from the release the Nightly names, so the tool, the SDK your mod builds against and the game players run are the
same build.

You also need Rust 1.95 or newer (`cargo`, from [rustup.rs](https://rustup.rs)). The tool runs `cargo` itself.

## The prebuilt tool

All of Sunrise's releases are prereleases, so there is no "latest" to ask for. The `nightly` release's `sdk.json`
names the current Nightly's build, and the `sdk-<build>` release of that build holds the tool. Each block below
reads the build, downloads the tool, `SHA256SUMS` and its signature, stops unless Sunrise's key signed the sums and the
tool's hash is in them, and only then puts `cargo-sunrise-mod` on your `PATH`. The key in each block is the one in
[`release-keys.txt`](../../../../crates/sunrise-release-sign/release-keys.txt); the signature is checked with OpenSSL
(1.1.1 or newer), which is what [how a download is checked](../reference/sdk/01-release.md#how-a-download-is-checked)
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
  COMMIT=$(curl -fsSL "$BASE/nightly/sdk.json" | sed -n 's/.*"commit": *"\([0-9a-f]\{40\}\)".*/\1/p' | head -n 1)
  cd "$(mktemp -d)"
  curl -fsSLO "$BASE/sdk-$COMMIT/$TOOL"
  curl -fsSLO "$BASE/sdk-$COMMIT/SHA256SUMS"
  curl -fsSLO "$BASE/sdk-$COMMIT/SHA256SUMS.sig"
  printf 'horizon release sdk-%s\n' "$COMMIT" | cat - SHA256SUMS > message
  { printf '\060\052\060\005\006\003\053\145\160\003\041\000'; printf '%s' "$KEY" | "$OPENSSL" base64 -d -A; } > key.der
  signed=
  while read -r line || [ -n "$line" ]; do
    printf '%s' "$line" | "$OPENSSL" base64 -d -A > signature
    "$OPENSSL" pkeyutl -verify -rawin -pubin -keyform DER -inkey key.der -in message -sigfile signature > /dev/null 2>&1 && signed=1
  done < SHA256SUMS.sig
  [ -n "$signed" ] || { echo "SHA256SUMS is not signed by Sunrise" >&2; exit 1; }
  grep " $TOOL\$" SHA256SUMS | sha256sum -c -
  tar -xJf "$TOOL"
  install -Dm755 cargo-sunrise-mod "$HOME/.local/bin/cargo-sunrise-mod"
) && export PATH="$HOME/.local/bin:$PATH"
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
  COMMIT=$(curl -fsSL "$BASE/nightly/sdk.json" | sed -n 's/.*"commit": *"\([0-9a-f]\{40\}\)".*/\1/p' | head -n 1)
  cd "$(mktemp -d)"
  curl -fsSLO "$BASE/sdk-$COMMIT/$TOOL"
  curl -fsSLO "$BASE/sdk-$COMMIT/SHA256SUMS"
  curl -fsSLO "$BASE/sdk-$COMMIT/SHA256SUMS.sig"
  printf 'horizon release sdk-%s\n' "$COMMIT" | cat - SHA256SUMS > message
  { printf '\060\052\060\005\006\003\053\145\160\003\041\000'; printf '%s' "$KEY" | "$OPENSSL" base64 -d -A; } > key.der
  signed=
  while read -r line || [ -n "$line" ]; do
    printf '%s' "$line" | "$OPENSSL" base64 -d -A > signature
    "$OPENSSL" pkeyutl -verify -rawin -pubin -keyform DER -inkey key.der -in message -sigfile signature > /dev/null 2>&1 && signed=1
  done < SHA256SUMS.sig
  [ -n "$signed" ] || { echo "SHA256SUMS is not signed by Sunrise" >&2; exit 1; }
  grep " $TOOL\$" SHA256SUMS | shasum -a 256 -c -
  tar -xJf "$TOOL"
  install -d "$HOME/.local/bin"
  install -m755 cargo-sunrise-mod "$HOME/.local/bin/cargo-sunrise-mod"
) && export PATH="$HOME/.local/bin:$PATH"
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
  Set-Location (New-Item -ItemType Directory (Join-Path $env:TEMP "sunrise-tool-$PID"))
  Invoke-WebRequest "$Base/nightly/sdk.json" -OutFile sdk.json
  $Commit = (Get-Content sdk.json -Raw | ConvertFrom-Json).build.commit
  Invoke-WebRequest "$Base/sdk-$Commit/$Tool" -OutFile $Tool
  Invoke-WebRequest "$Base/sdk-$Commit/SHA256SUMS" -OutFile SHA256SUMS
  Invoke-WebRequest "$Base/sdk-$Commit/SHA256SUMS.sig" -OutFile SHA256SUMS.sig
  $Prefix = [byte[]](0x30,0x2a,0x30,0x05,0x06,0x03,0x2b,0x65,0x70,0x03,0x21,0x00)
  [IO.File]::WriteAllBytes("key.der", $Prefix + [Convert]::FromBase64String($Key))
  $Head = [Text.Encoding]::UTF8.GetBytes("horizon release sdk-$Commit`n")
  [IO.File]::WriteAllBytes("message", $Head + [IO.File]::ReadAllBytes("SHA256SUMS"))
  $Signed = $false
  foreach ($Line in Get-Content SHA256SUMS.sig) {
    if ($Line.Trim() -eq "") { continue }
    [IO.File]::WriteAllBytes("signature", [Convert]::FromBase64String($Line.Trim()))
    & $OpenSsl pkeyutl -verify -rawin -pubin -keyform DER -inkey key.der -in message -sigfile signature *> $null
    if ($LASTEXITCODE -eq 0) { $Signed = $true }
  }
  if (-not $Signed) { throw "SHA256SUMS is not signed by Sunrise" }
  $Want = ((Select-String -Path SHA256SUMS -Pattern "  $([regex]::Escape($Tool))\s*$").Line -split " ")[0]
  if ((Get-FileHash $Tool -Algorithm SHA256).Hash.ToLower() -ne $Want) { throw "checksum mismatch" }
  tar -xf $Tool
  $Bin = Join-Path $env:USERPROFILE ".sunrise\bin"
  New-Item -ItemType Directory -Force $Bin | Out-Null
  Move-Item -Force cargo-sunrise-mod.exe $Bin
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

The tool checks every SDK it fetches against Sunrise's release signature before it uses it, whatever the tool's own
origin.

To move to a newer tool later, run the same block again.

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
