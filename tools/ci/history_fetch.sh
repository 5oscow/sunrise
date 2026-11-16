#!/usr/bin/env bash
# Unshallow this checkout's HEAD for the gate's history checks (public-tree ranges, sync ancestry, selection): two
# tries of at most 300 s each, then fail closed naming $FETCH_STEP.
#
# On a Dyna runner the slot keeps a bare mirror of the repository in $DYNA_CI_BUILD, the slot's persistent folder.
# The checkout borrows its objects as an alternate, so GitHub sends only what the last job on this slot didn't see
# (a full unshallow was about 146 s), and the mirror takes this head afterwards. A mirror that is missing, from
# another repository or broken costs nothing but the saving: it is dropped and the fetch goes on without it.
set -uo pipefail
step=${FETCH_STEP:?FETCH_STEP names the step in errors}
[ "$(git rev-parse --is-shallow-repository)" = false ] && exit 0
head=$(git rev-parse HEAD)
alternates=$(git rev-parse --git-path objects/info/alternates)
mirror= borrowed=
if [ -n "${DYNA_CI_BUILD:-}" ]; then
  mirror="$DYNA_CI_BUILD/git-mirror/${GITHUB_REPOSITORY:?}.git"
  if git -C "$mirror" rev-parse -q --verify refs/heads/ci-head >/dev/null 2>&1; then
    echo "$mirror/objects" >> "$alternates"
    borrowed=1
  fi
fi
for attempt in 1 2; do
  if timeout --kill-after=10s 300s git fetch -q --no-tags --unshallow origin "$head"; then
    if [ -n "$mirror" ]; then
      # Keep this head for the next job; a failure only loses the saving. A mirror this checkout borrows from stays
      # for the job's later steps, and the next job, finding it unusable, borrows nothing and drops it here.
      { git init -q --bare "$mirror" &&
        timeout --kill-after=10s 300s git -C "$mirror" fetch -q --no-tags "$(git rev-parse --show-toplevel)" \
          "+$head:refs/heads/ci-head" &&
        git -C "$mirror" -c gc.autoDetach=false gc --auto --quiet; } ||
        { echo "$step: the slot's history mirror could not take this head."; [ -n "$borrowed" ] || rm -rf "$mirror"; }
    fi
    exit 0
  fi
  echo "$step: GitHub fetch attempt $attempt failed or timed out."
  if [ -n "$borrowed" ]; then
    # The objects fetched before the alternate was added stand alone. Retry without the mirror, in case it is what
    # broke; a mirror that then can't take the head is dropped above.
    echo "$step: retrying without the slot's history mirror."
    sed -i "\\|^$mirror/objects\$|d" "$alternates"
    borrowed=
  fi
done
echo "::error::$step: history fetch from GitHub failed or timed out twice."
exit 1
