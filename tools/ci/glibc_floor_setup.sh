#!/usr/bin/env bash
# Give the Linux SDK Jammy's glibc ABI without mixing the runner's libc into Zig's link search.
set -euo pipefail
umask 077
zig_version=0.15.2
jammy_snapshot=20261118T044000Z
root="${DYNA_CI_BUILD:?}/sdk-glibc-2.35-zig-$zig_version-jammy-$jammy_snapshot"
mkdir -p "$root/downloads"
cd "$root/downloads"

verify() {
  if ! printf '%s  %s\n' "$1" "$2" | sha256sum --check --status; then
    echo "::error::glibc floor setup: checksum mismatch for $3" >&2
    return 1
  fi
}

fetch() {
  local url=$1 sha=$2 name=${1##*/} incoming
  if [[ -f $name ]]; then
    verify "$sha" "$name" "$name"
    return
  fi
  incoming=$(mktemp "$name.part.XXXXXX")
  if curl --proto '=https' --fail --silent --show-error --location --retry 2 --max-time 180 \
      --output "$incoming" "$url" && verify "$sha" "$incoming" "$name"; then
    mv -- "$incoming" "$name"
  else
    rm -f -- "$incoming"
    return 1
  fi
}

zig_archive=zig-x86_64-linux-$zig_version.tar.xz
fetch "https://ziglang.org/download/$zig_version/$zig_archive" \
  02aa270f183da276e5b5920b1dac44a63f1a49e55050ebde3aecc9eb82f93239

# Re-extract from the verified downloads on every job. Never execute a cached unpacked compiler or wrapper.
stage=$(mktemp -d "$root/.toolchain.XXXXXX")
trap 'rm -rf -- "$stage"' EXIT
mkdir -p "$stage"/{zig,bin,jammy,desktop/include,desktop/lib/pkgconfig}
tar -xJf "$zig_archive" -C "$stage/zig" --strip-components=1
while read -r sha package; do
  fetch "https://snapshot.ubuntu.com/ubuntu/$jammy_snapshot/pool/main/$package" "$sha"
  dpkg -x "${package##*/}" "$stage/jammy"
done <<'PACKAGES'
dce4ce1043cde35f4bc375a4b1bd84badd5dec7e656ea2a8849edf9215bc8c33 a/alsa-lib/libasound2_1.2.6.1-1ubuntu1.2_amd64.deb
b809605fef4259e04ad6c9b8362fe6013209ddc67ac52a6bf43f87723f8ec815 a/alsa-lib/libasound2-dev_1.2.6.1-1ubuntu1.2_amd64.deb
7e056a0e8bd49a4a16e3b9490707443fbd918a828f51284ea473776e4f94d956 s/systemd/libudev-dev_249.11-0ubuntu3.23_amd64.deb
e6bf821bc82eb05ba44692242a4da43b34b65ce137c0c4a46986bb122a4624ef s/systemd/libudev1_249.11-0ubuntu3.23_amd64.deb
PACKAGES

# Do not put either distro's entire include or library directory on Zig's search path.
ln -s ../../jammy/usr/include/alsa "$stage/desktop/include/alsa"
ln -s ../../jammy/usr/include/libudev.h "$stage/desktop/include/libudev.h"
libdir="$stage/jammy/usr/lib/x86_64-linux-gnu"
for name in libasound.so libudev.so; do
  test -f "$libdir/$name"
  ln -s "$(realpath --relative-to="$stage/desktop/lib" "$libdir/$name")" "$stage/desktop/lib/$name"
done
for module in alsa libudev; do
  sed -E 's|^prefix=.*|prefix=${pcfiledir}/../..|; s|^libdir=.*|libdir=${prefix}/lib|; s|^includedir=.*|includedir=${prefix}/include|' \
    "$libdir/pkgconfig/$module.pc" > "$stage/desktop/lib/pkgconfig/$module.pc"
done

cat > "$stage/bin/cc" <<'WRAPPER'
#!/usr/bin/env bash
set -euo pipefail
root=${0%/*}/..
args=()
for arg in "$@"; do
  # cc-rs may supply Clang's Rust triple. Keep Zig's spelling and explicit glibc version instead.
  case $arg in
    --target=x86_64-unknown-linux-gnu) ;;
    *) args+=("$arg") ;;
  esac
done
exec "$root/zig/zig" cc -target x86_64-linux-gnu.2.35 -L"$root/desktop/lib" "${args[@]}"
WRAPPER
chmod +x "$stage/bin/cc"

toolchain="$root/toolchain"
rm -rf -- "$toolchain"
mv -- "$stage" "$toolchain"
# Actions protects PATH itself; GITHUB_PATH puts the wrapper first for naked rustc and host proc-macro links.
printf '%s\n' "$toolchain/bin" >> "${GITHUB_PATH:?}"
{
  echo "CC=$toolchain/bin/cc"
  echo "PKG_CONFIG_LIBDIR=$toolchain/desktop/lib/pkgconfig"
  echo 'PKG_CONFIG_PATH='
  echo 'PKG_CONFIG_SYSROOT_DIR='
  # hidapi uses pkg-config to link but does not pass its include flags to cc.
  echo "CFLAGS=-idirafter $toolchain/desktop/include"
} >> "${GITHUB_ENV:?}"
