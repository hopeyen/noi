#!/usr/bin/env bash
# Runs the tests shipped inside each packaged crate (target/package/*.crate), the way a user of the
# published crates would see them. Catches tests and doctests that read files outside the package.
#
# Usage: cargo package --workspace --exclude noi-example-app --exclude noi-dark-pool-example
#        scripts/test_packaged.sh [cargo test flags, e.g. --all-features]
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WS="$(mktemp -d)"
trap 'rm -rf "${WS}"' EXIT

shopt -s nullglob
crates=("${ROOT_DIR}"/target/package/*.crate)
if [ ${#crates[@]} -eq 0 ]; then
  echo "no packaged crates in target/package; run cargo package first" >&2
  exit 1
fi

members=()
patches=()
for crate in "${crates[@]}"; do
  tar -xzf "$crate" -C "$WS"
  dir="$(basename "$crate" .crate)"
  name="${dir%-*}"
  members+=("\"${dir}\"")
  patches+=("${name} = { path = \"${dir}\" }")
done

{
  echo "[workspace]"
  echo "resolver = \"2\""
  echo "members = [$(IFS=,; echo "${members[*]}")]"
  echo
  echo "# The packaged manifests depend on each other by version; use the extracted sources."
  echo "[patch.crates-io]"
  printf '%s\n' "${patches[@]}"
} > "${WS}/Cargo.toml"

cd "$WS"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT_DIR}/target/packaged-tests}" cargo test --workspace "$@"
