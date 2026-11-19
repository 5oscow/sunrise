#!/usr/bin/env python3
"""The list of gated Host methods that can only refuse by unwinding only shrinks (#3123).

crates/sunrise-mod-api-gen/unwinding-methods.txt names each `#[access]` method whose return type has no
refusal-shaped `Result`. The generator refuses a method missing from it and a line that no longer applies; this
refuses a line added since the base, so the list cannot be grown to admit a new method.

    python3 tools/ci/check_unwinding_methods.py BASE_SHA
"""
import subprocess
import sys

LIST = "crates/sunrise-mod-api-gen/unwinding-methods.txt"


def lines(text):
    return {line for line in text.splitlines() if line and not line.startswith("#")}


def main():
    base = subprocess.run(["git", "show", f"{sys.argv[1]}:{LIST}"], capture_output=True, text=True)
    if base.returncode != 0:
        return  # the list is new in this change
    with open(LIST) as current:
        added = sorted(lines(current.read()) - lines(base.stdout))
    if added:
        sys.exit(f"{LIST} only shrinks; these lines were added since the base:\n" + "\n".join(added))


if __name__ == "__main__":
    main()
