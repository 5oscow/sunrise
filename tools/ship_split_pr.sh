#!/usr/bin/env bash
# Validate and ship one split: format, local CI checks, remote type check +
# layer check, signed commit, push, PR. The type check runs twice because the
# workspace manifest is auto-rewritten by the first run (cargo lock/format
# churn), so a fresh run after that rewrite is the one whose exit code counts.
set -euo pipefail

BRANCH="$1"
NEW_FILE="$2"
COMMIT_MSG="$3"
PR_TITLE="$4"
PR_BODY_FILE="$5"

cd "$(git rev-parse --show-toplevel)"

export SUNRISE_GAME_DISCOVERY="/run/media/arthur/Data/horizone/bin/sunrise-game-discovery"
export PATH="/run/media/arthur/Data/horizone/bin:$PATH"
export GH_TOKEN=$(git remote get-url darkliro-sunrise | sed -n 's/.*:\(.*\)@.*/\1/p')

rustfmt --edition 2024 --config skip_children=true "crates/sunrise-client/src/game/network/remote/${NEW_FILE}"
git add crates/sunrise-client/src/game/network/remote/ tools/ci/ignored-tests/

python3 tools/ci/format_scan.py > /dev/null
python3 tools/ci/check_ignored_test_rows.py > /dev/null
python3 tools/check_public_tree.py > /dev/null

rsync -aq --delete --exclude='.git' --exclude='target' --exclude='.local' \
    /run/media/arthur/Data/horizone/horizon-main/ pc-fixe:~/projets/horizon-main/

CHECK='flock /tmp/creatures-cargo.lock cargo check -p sunrise-client --lib --tests --no-default-features --message-format short'
dark-run "$CHECK" > /tmp/typecheck-1.log 2>&1 || true
if ! dark-run "$CHECK" > /tmp/typecheck-2.log 2>&1; then
    grep -E "error" /tmp/typecheck-2.log | head -20 >&2
    echo "type check failed" >&2
    exit 1
fi
dark-run "python3 tools/check_layers.py" > /dev/null

git commit -s -q -m "${COMMIT_MSG}"
git push -u -q darkliro-sunrise "${BRANCH}"

gh pr create --repo sunrise-org/sunrise --base main --head "darkliro:${BRANCH}" \
    --title "${PR_TITLE}" --body-file "${PR_BODY_FILE}"
