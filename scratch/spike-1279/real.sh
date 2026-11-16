#!/usr/bin/env bash
# The real wasm mod build step for each guest variant: the exact cargo invocation and environment of
# `cargo sunrise-mod build --wasm` (crates/cargo-sunrise-mod/src/wasm.rs `cargo_command`, wasm/group.rs build),
# with cargo-sunrise-mod as rustc wrapper and as linker (rust-lld --shared, then the post-link rewrite).
# The spike crates are not full mods (no mod_api), so the stage is written here instead of by `build --wasm`.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
work="$root/target/spike-1279"
tool="$root/target/debug/cargo-sunrise-mod"
sysroot="$(rustc --print sysroot)"
host="$(rustc -vV | sed -n 's/^host: //p')"
lld="$sysroot/lib/rustlib/$host/bin/rust-lld"
variants="${*:-dyn static concrete dyn_one dyn_all dead names dyn_inl static_inl dyn_one_inl dyn_all_inl}"
for variant in $variants; do
  name="spike-${variant//_/-}"
  dir="$work/guests/$variant"
  python3 "$here/gen_guest.py" "$dir" "$variant" 50
  # as group.rs does for a staged member: rlib + cdylib
  sed -i 's/crate-type = \["cdylib"\]/crate-type = ["rlib", "cdylib"]/' "$dir/Cargo.toml"
  target="$work/real/$variant"
  mkdir -p "$target"
  (cd "$dir" && \
    CARGO_TARGET_DIR="$target" CARGO_INCREMENTAL=0 RUSTC_WRAPPER="$tool" \
    SUNRISE_GROUP_WRAPPER="{\"$name\":{\"id\":\"${name//-/_}\",\"version\":\"0.1.0\",\"stage\":\"$dir\",\"registrations\":\"\",\"points\":\"[]\",\"provided\":\"\"}}" \
    SUNRISE_STD_ARTIFACTS="$target/std-artifacts" SUNRISE_BUILD_ROOT="$target" RUSTC_BOOTSTRAP=1 \
    RUSTFLAGS="-Crelocation-model=pic -Zshare-generics=n -Zdefault-visibility=hidden --cfg=sunrise_shared_artifacts_v2" \
    CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER="$tool" SUNRISE_MOD_LINK=member SUNRISE_MOD_LLD="$lld" \
    cargo build --lib --target wasm32-unknown-unknown --profile sunrise-wasm -Zbuild-std=std,panic_abort \
      --config 'profile.sunrise-wasm.inherits="release"' --config 'profile.sunrise-wasm.panic="abort"' \
      --config 'profile.sunrise-wasm.codegen-units=1' --config 'profile.sunrise-wasm.build-override.codegen-units=1' \
      --workspace -q)
  echo -n "$variant real: "
  python3 "$here/imports.py" "$target/wasm32-unknown-unknown/sunrise-wasm/${name//-/_}.wasm"
done
