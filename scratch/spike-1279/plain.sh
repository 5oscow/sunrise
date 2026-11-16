#!/usr/bin/env bash
# Plain rustc + rust-lld builds (debug and release) of each guest variant, then the sunrise import counts.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
work="$root/target/spike-1279"
mkdir -p "$work"
for variant in dyn static concrete dyn_one dyn_all dead names; do
  dir="$work/guests/$variant"
  python3 "$here/gen_guest.py" "$dir" "$variant" 50
  name="spike_${variant//_/-}"
  for profile in debug release; do
    flag=""; [ "$profile" = release ] && flag="--release"
    (cd "$dir" && CARGO_TARGET_DIR="$work/plain" cargo build -q --target wasm32-unknown-unknown $flag)
    echo -n "$variant $profile: "
    python3 "$here/imports.py" "$work/plain/wasm32-unknown-unknown/$profile/${name//-/_}.wasm"
  done
done
