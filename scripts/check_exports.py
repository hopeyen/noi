#!/usr/bin/env python3
"""Fail if `scripts/refresh_exports.sh` changed any committed Noir artifact.

Run it after the refresh script. The refresh script makes artifacts byte-reproducible (it rewrites
the absolute source paths nargo embeds to repository-relative ones), so any difference, including
an added or deleted artifact, means the committed exports are out of date.
"""
import subprocess
import sys

PATHS = [
    "examples/demo/noir/export",
    "examples/dark-pool/noir/export",
    "crates/noi/tests/fixtures/demo",
    "crates/noi-core/tests/fixtures/demo",
]

root = subprocess.run(
    ["git", "rev-parse", "--show-toplevel"], check=True, capture_output=True, text=True
).stdout.strip()
status = subprocess.run(
    ["git", "status", "--porcelain", "--untracked-files=all", "--", *PATHS],
    cwd=root, check=True, capture_output=True, text=True,
).stdout

if status.strip():
    print("Noir exports are out of date; run scripts/refresh_exports.sh and commit:")
    print(status, end="")
    sys.exit(1)
print("exports up to date")
