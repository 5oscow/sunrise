#!/bin/bash
# PURPOSE: proves packaging/server/install.sh installs only what sunrise-verify has checked against the release's
# signature, and refuses everything else with a message naming the cause (docs/release-flow.md, "Signed checksums").
# WHY: the first install used to check the signature only when OpenSSL 3 was installed (docs/security.md).
# REFS: tools/ci/sign_release.sh (signs the fake releases with a throwaway key), crates/sunrise-release-sign (the
# sunrise-verify built with its test-key feature, trusting that key), tests/it/install.rs (the daily run's driver).
# LOGS: prints one ok/FAIL line per case; exit 1 when any case fails.
# READS-BY: tests/it/install.rs in sunrise-release-sign (daily), tests/test_install_sh_signature.py (skips unless
# SUNRISE_VERIFY_TEST_BIN is set), by hand: SUNRISE_VERIFY_TEST_BIN=target/debug/sunrise-verify bash tests/install_sh_signature.sh
# ENDS-WHEN: it ends by itself; leaves nothing outside its own mktemp folder.
# Needs OpenSSL 3 (to sign the throwaway release; install.sh itself uses none) and SUNRISE_VERIFY_TEST_BIN, a
# sunrise-verify built with `--features test-key`; skips (exit 77) without them. curl is replaced by a fake that
# serves local files, so nothing touches the network.
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
openssl version | grep -Eq '^OpenSSL [3-9]\.' || { echo "skip: OpenSSL 3 needed"; exit 77; }
VERIFY_BIN=${SUNRISE_VERIFY_TEST_BIN:-}
[ -x "$VERIFY_BIN" ] || { echo "skip: SUNRISE_VERIFY_TEST_BIN (a sunrise-verify built with --features test-key) needed"; exit 77; }
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
fails=0
check() {  # name, expected exit (0 or 1), output grep (may be empty), then the case's exit and output files
  local name=$1 want=$2 pat=$3 rc=$4 out=$5
  local exit_ok=no
  if [ "$want" = 0 ]; then [ "$rc" = 0 ] && exit_ok=yes; else [ "$rc" != 0 ] && exit_ok=yes; fi
  if [ "$exit_ok" = yes ] && { [ -z "$pat" ] || grep -q -- "$pat" "$out"; }; then
    echo "ok   $name"
  else echo "FAIL $name (exit $rc, wanted $want, pattern '$pat')"; sed 's/^/     /' "$out"; fails=$((fails+1)); fi
}

# A fake release: an archive holding sunrise-server, signed for TAG with a throwaway key. A Nightly (TAG nightly)
# is also signed for its own version ALSO (nightly-<date>-<commit>) in SHA256SUMS.v2.sig, titled as
# nightly-build.yml titles it.
make_release() {  # dir tag [also]
  local d=$1 tag=$2 also=${3:-}
  mkdir -p "$d/src/sunrise-server-x" "$d/files"
  printf '#!/bin/sh\necho fake\n' > "$d/src/sunrise-server-x/sunrise-server"; chmod +x "$d/src/sunrise-server-x/sunrise-server"
  tar -czf "$d/files/sunrise-server-linux-x86_64.tar.gz" -C "$d/src" sunrise-server-x
  cp "$VERIFY_BIN" "$d/files/sunrise-verify-linux-x86_64"
  ALSO_FOR=$also EPHEMERAL=yes bash "$root/tools/ci/sign_release.sh" "$tag" "$d/files/sunrise-server-linux-x86_64.tar.gz" > /dev/null
  json "$d" "$tag" yes "$(title "$tag" "$also")"
}
title() {  # tag also: the release's GitHub title
  if [ -n "$2" ]; then printf 'Nightly %s · %s' "$(printf '%s' "$2" | cut -c9-18)" "$(printf '%s' "$2" | cut -c20-)"
  else printf '%s' "$1"; fi
}
json() {  # dir tag with-sums [title]: GitHub's release JSON, as install.sh reads it
  local d=$1 tag=$2 sums=$3 name=${4:-$2} sha
  sha=$(sha256sum "$d/files/sunrise-server-linux-x86_64.tar.gz" | cut -d' ' -f1)
  {
    printf '{\n  "tag_name": "%s",\n  "name": "%s",\n  "assets": [\n' "$tag" "$name"
    printf '    {\n      "name": "sunrise-server-linux-x86_64.tar.gz",\n      "digest": "sha256:%s",\n      "browser_download_url": "https://dl.test/sunrise-server-linux-x86_64.tar.gz"\n    }' "$sha"
    if [ "$sums" = yes ]; then for f in SHA256SUMS SHA256SUMS.sig SHA256SUMS.v2.sig; do
      [ -f "$d/files/$f" ] || continue
      printf ',\n    {\n      "name": "%s",\n      "browser_download_url": "https://dl.test/%s"\n    }' "$f" "$f"; done; fi
    printf '\n  ]\n}\n'
  } > "$d/release.json"
}
# The fake curl: the API address answers with release.json, any other address with the file of that name.
mkdir "$W/fake"
cat > "$W/fake/curl" <<'CURL'
#!/bin/sh
out=; url=
while [ $# -gt 0 ]; do case "$1" in -o) out=$2; shift 2 ;; -*) shift ;; *) url=$1; shift ;; esac; done
case "$url" in
  https://api.github.com/*) src=$REL/release.json ;;
  *) src=$REL/files/$(basename "$url") ;;
esac
[ -f "$src" ] || exit 22
if [ -n "$out" ]; then cp "$src" "$out"; else cat "$src"; fi
CURL
chmod +x "$W/fake/curl"
# The fake uname: install.sh picks the Linux x86-64 archive, the one these releases hold, on any machine.
cat > "$W/fake/uname" <<'UNAME'
#!/bin/sh
case "$1" in -m) echo x86_64 ;; *) echo Linux ;; esac
UNAME
chmod +x "$W/fake/uname"

# Every case runs the real install.sh. The verifier is the test build, which trusts the case's throwaway key
# (SUNRISE_VERIFY_TEST_KEY); its pin is replaced, as install.sh allows only for a --dir inside SUNRISE_INSTALL_TEST_ROOT.
VERIFY_SHA=$(sha256sum "$VERIFY_BIN" | cut -d' ' -f1)
export SUNRISE_INSTALL_TEST_ROOT=$W
run() {  # dir, ...install args: runs install.sh with the fake curl; KEY and PIN override the case's key and pin
  local d=$1; shift
  REL=$d PATH="$W/fake:/usr/bin:/bin" HOME="$W/home" \
    SUNRISE_VERIFY_TEST_KEY="${KEY:-$(cat "$d/files/ephemeral.pub")}" \
    SUNRISE_INSTALL_TEST_VERIFIER_VERSION=test SUNRISE_INSTALL_TEST_VERIFIER_SHA256="${PIN-$VERIFY_SHA}" \
    sh "${INSTALLER:-$root/packaging/server/install.sh}" --dir "$d/inst" --bin "$d/bin" "$@" > "$d/out" 2>&1
  echo $? > "$d/rc"
}
verdict() { check "$1" "$2" "$3" "$(cat "$4/rc")" "$4/out"; }
refused() { [ ! -e "$1/inst/current" ] || { echo "FAIL $2: it was installed"; fails=$((fails+1)); }; }
# sed -i differs between GNU and BSD (macOS) sed: edit through a copy instead.
edit() {  # sed-expression file
  sed "$1" "$2" > "$2.tmp" && mv "$2.tmp" "$2"
}
mkdir -p "$W/home"

# Passing cases.
# 1. Good signature, a Nightly: SHA256SUMS.v2.sig signed for the version its title names.
NIGHTLY=nightly-2026-11-03-0c02067
d=$W/good; make_release "$d" nightly "$NIGHTLY"
run "$d"; verdict "good release installs (Nightly, signed for its own version)" 0 "Release signature OK" "$d"
[ -x "$d/inst/current/sunrise-server" ] || { echo "FAIL nothing installed"; fails=$((fails+1)); }
grep -q "Downloading Sunrise $NIGHTLY " "$d/out" || { echo "FAIL not installed as $NIGHTLY"; fails=$((fails+1)); }

# 2. Good signature, a normal tag.
d=$W/tag; make_release "$d" v1.2.3
run "$d" --version v1.2.3; verdict "good release installs (release tag)" 0 "Release signature OK" "$d"

# 3. --insecure-checksum-only: installs on GitHub's SHA-256, with a loud warning, and needs no verifier at all.
d=$W/insecure; make_release "$d" v1.2.3; rm "$d/files/sunrise-verify-linux-x86_64" "$d/files/SHA256SUMS.sig"; json "$d" v1.2.3 no
PIN=unpinned run "$d" --version v1.2.3 --insecure-checksum-only
verdict "--insecure-checksum-only installs, with its warning" 0 "WARNING: --insecure-checksum-only" "$d"
[ -x "$d/inst/current/sunrise-server" ] || { echo "FAIL insecure: nothing installed"; fails=$((fails+1)); }
d=$W/insecure2; make_release "$d" v1.2.3; printf 'x' >> "$d/files/sunrise-server-linux-x86_64.tar.gz"
edit 's/"digest": "sha256:[0-9a-f]*"/"digest": "sha256:0000"/' "$d/release.json"
run "$d" --version v1.2.3 --insecure-checksum-only; verdict "--insecure-checksum-only still refuses a wrong SHA-256" 1 "doesn't match its SHA-256" "$d"

# The five tamper cases (docs: every refusal names its cause).
# T1. One byte of the archive flipped; GitHub's digest agrees with it, only the signed SHA256SUMS catches it.
d=$W/arc; make_release "$d" v1.2.3
printf 'x' >> "$d/files/sunrise-server-linux-x86_64.tar.gz"; json "$d" v1.2.3 yes
run "$d" --version v1.2.3; verdict "T1 flipped archive byte refused" 1 "doesn't match its signed checksum" "$d"; refused "$d" "flipped archive byte"

# T2. SHA256SUMS signed with another key (a different throwaway key than the verifier trusts).
d=$W/wrong; make_release "$d" v1.2.3; d2=$W/otherkey; make_release "$d2" v1.2.3
KEY=$(cat "$d2/files/ephemeral.pub") run "$d" --version v1.2.3
verdict "T2 SHA256SUMS signed with another key refused" 1 "signature doesn't match Sunrise's release key" "$d"; refused "$d" "another key"
# T2b. Its list replaced with the tampered archive's hash, the signature kept.
d=$W/sums; make_release "$d" v1.2.3
printf 'x' >> "$d/files/sunrise-server-linux-x86_64.tar.gz"
new=$(sha256sum "$d/files/sunrise-server-linux-x86_64.tar.gz" | cut -d' ' -f1)
printf '%s  sunrise-server-linux-x86_64.tar.gz\n' "$new" > "$d/files/SHA256SUMS"; json "$d" v1.2.3 yes
run "$d" --version v1.2.3; verdict "T2b tampered SHA256SUMS refused" 1 "signature doesn't match" "$d"

# T3. The .sig missing from the release.
d=$W/nosig; make_release "$d" v1.2.3; rm "$d/files/SHA256SUMS.sig"; json "$d" v1.2.3 yes
run "$d" --version v1.2.3; verdict "T3 missing SHA256SUMS.sig refused" 1 "has no signed checksums (SHA256SUMS.sig)" "$d"; refused "$d" "missing .sig"

# T4. The verifier swapped for another binary: its hash isn't the pinned one, so it never runs.
d=$W/swap; make_release "$d" v1.2.3
printf '#!/bin/sh\nexit 0\n' > "$d/files/sunrise-verify-linux-x86_64"
run "$d" --version v1.2.3; verdict "T4 swapped verifier refused (hash mismatch)" 1 "isn't the pinned build" "$d"; refused "$d" "swapped verifier"

# T5. No verifier for this platform (no pin), and a verifier whose download fails.
d=$W/nopin; make_release "$d" v1.2.3
PIN= run "$d" --version v1.2.3; verdict "T5 no verifier for the platform refused" 1 "there is no sunrise-verify for linux-x86_64" "$d"; refused "$d" "no verifier"
d=$W/nodl; make_release "$d" v1.2.3; rm "$d/files/sunrise-verify-linux-x86_64"
run "$d" --version v1.2.3; verdict "T5 verifier download failure refused" 1 "could not download sunrise-verify" "$d"; refused "$d" "no verifier download"
# The placeholder pin: a script that pins no verifier refuses; and the test overrides apply only to a --dir inside
# SUNRISE_INSTALL_TEST_ROOT, so with another root the script's own pin decides (the test build isn't it).
d=$W/unpinned; make_release "$d" v1.2.3
sed 's/^VERIFIER_VERSION=.*/VERIFIER_VERSION=unpinned/' "$root/packaging/server/install.sh" > "$d/install.sh"
REL=$d PATH="$W/fake:/usr/bin:/bin" HOME="$W/home" sh "$d/install.sh" --dir "$d/inst" --bin "$d/bin" --version v1.2.3 > "$d/out" 2>&1; echo $? > "$d/rc"
verdict "unpinned script refused" 1 "has no pinned sunrise-verify" "$d"; refused "$d" "unpinned script"
d=$W/outside; make_release "$d" v1.2.3
REL=$d PATH="$W/fake:/usr/bin:/bin" HOME="$W/home" SUNRISE_INSTALL_TEST_ROOT="$W/elsewhere" SUNRISE_INSTALL_TEST_VERIFIER_VERSION=test \
  SUNRISE_INSTALL_TEST_VERIFIER_SHA256="$VERIFY_SHA" sh "$root/packaging/server/install.sh" --dir "$d/inst" --bin "$d/bin" --version v1.2.3 > "$d/out" 2>&1; echo $? > "$d/rc"
verdict "a pin override outside the test root is ignored" 1 "isn't the pinned build\|has no pinned sunrise-verify\|could not download sunrise-verify" "$d"; refused "$d" "override outside the test root"

# Nightly and replay cases from before.
# N1. A Nightly with only the channel's signature (published before v2): refused, never the channel's signature.
d=$W/channel; make_release "$d" nightly; json "$d" nightly yes "$(title nightly "$NIGHTLY")"
run "$d"; verdict "Nightly without SHA256SUMS.v2.sig refused" 1 "has no SHA256SUMS.v2.sig" "$d"; refused "$d" "a Nightly without v2"

# N2. An older Nightly's files and signature retitled as a newer one: its v2 signature names the old version.
d=$W/rollback; make_release "$d" nightly nightly-2026-11-01-1111111
json "$d" nightly yes "$(title nightly "$NIGHTLY")"
run "$d"; verdict "older Nightly passed off as a newer one refused" 1 "signature doesn't match" "$d"

# N3. A Nightly whose title doesn't name its date and commit: refused, not dated some other way.
d=$W/title; make_release "$d" nightly "$NIGHTLY"; json "$d" nightly yes "Nightly"
run "$d"; verdict "Nightly with an unreadable title refused" 1 "title doesn't name its date and commit" "$d"

# N4. A signature made for another tag does not pass (a release's list replayed under another tag).
d=$W/replay; make_release "$d" v1.2.3
edit 's/"tag_name": "v1.2.3"/"tag_name": "v9.9.9"/' "$d/release.json"
run "$d" --version v9.9.9; verdict "signature replayed under another tag refused" 1 "signature doesn't match" "$d"

# N5. A release published before signing (no SHA256SUMS): refused now; the escape hatch is the flag above.
d=$W/legacy; make_release "$d" v1.2.3; json "$d" v1.2.3 no
run "$d" --version v1.2.3; verdict "release without signed checksums refused" 1 "has no signed checksums" "$d"

echo "$fails case(s) failed"; [ "$fails" = 0 ]
