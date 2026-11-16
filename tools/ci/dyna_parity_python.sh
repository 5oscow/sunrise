#!/usr/bin/env bash
# Run the workflow's parity harnesses with scratch outputs on the disk-backed slot.
# A harness's output carries vanilla's values, so it goes to a log on the runner's slot and the public job log gets
# one PASS or FAIL line per harness.
set -euo pipefail
case "${1:-}" in
  tools/test_*_parity.py)
    log="$DYNA_CI_BUILD/parity-logs/$(basename "$1" .py).log"
    status=0
    /usr/bin/python3 "$@" > "$log" 2>&1 || status=$?
    rm -rf "$DYNA_CI_BUILD"/parity-out/rust-*
    if [ "$status" -eq 0 ]; then echo "PASS $1"; else echo "FAIL $1 (exit $status; its log stays on the runner: $log)"; fi
    exit "$status"
    ;;
esac
exec /usr/bin/python3 "$@"
