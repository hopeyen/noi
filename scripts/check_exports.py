#!/usr/bin/env python3
"""Fail if regenerated Noir exports differ from the committed ones in ABI or bytecode.

Artifacts embed absolute source paths in their debug info (`file_map`), which differ between
machines, so a plain `git diff` cannot tell real drift from a different checkout location.
"""
import json
import subprocess
import sys

paths = subprocess.run(
    ["git", "ls-files", "--others", "--modified", "--exclude-standard",
     "examples/*/noir/export/*.json", "crates/noi/tests/fixtures/demo/*.json"],
    check=True, capture_output=True, text=True,
).stdout.split()

drift = []
for path in paths:
    committed = subprocess.run(["git", "show", f"HEAD:{path}"], capture_output=True, text=True)
    if committed.returncode != 0:
        drift.append(f"{path}: not committed")
        continue
    old, new = json.loads(committed.stdout), json.load(open(path))
    for key in ("noir_version", "abi", "bytecode"):
        if old.get(key) != new.get(key):
            drift.append(f"{path}: `{key}` changed")

if drift:
    print("Noir exports are out of date; run scripts/refresh_exports.sh and commit:", *drift, sep="\n  ")
    sys.exit(1)
print(f"exports up to date ({len(paths)} file(s) differ only in debug info)")
