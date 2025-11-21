#!/usr/bin/env bash
set -euo pipefail

if [[ $# -gt 0 && "$1" == "--help" ]]; then
  cat <<'USAGE'
Usage: scripts/publish.sh [cargo-publish-flags]

Publishes the core crates in dependency order:
  1. noi-core
  2. noi-build
  3. noi-macro
  4. noi (the user-facing facade crate)

Additional flags are forwarded to each `cargo publish` invocation, e.g.:
  scripts/publish.sh --dry-run
USAGE
  exit 0
fi

crates=(
  "noi-core"
  "noi-build"
  "noi-macro"
  "noi"
)

for crate in "${crates[@]}"; do
  echo "==> Publishing $crate"
  cargo publish -p "$crate" "$@"
done
