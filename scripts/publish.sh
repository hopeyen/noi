#!/usr/bin/env bash
# Publishes noi-core, noi-build, noi-macro, and noi to crates.io, in dependency order.
#
# Usage: scripts/publish.sh [--dry-run] [other cargo publish flags]
#
# Requires cargo >= 1.90 (workspace publishing). Run `cargo package --workspace ...` first (CI's
# `package` job does) to verify every crate builds from its packaged sources.
set -euo pipefail

if [[ "${1:-}" == "--help" ]]; then
  sed -n '2,7p' "$0"
  exit 0
fi

cd "$(dirname "${BASH_SOURCE[0]}")/.."
cargo publish --workspace --exclude noi-example-app --exclude noi-dark-pool-example "$@"
