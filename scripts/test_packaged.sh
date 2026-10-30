#!/usr/bin/env bash
# Runs the tests shipped inside each packaged crate (target/package/*.crate), the way a user of the
# published crates would see them. Catches tests and doctests that read files outside the package.
#
# Usage: cargo package --workspace --exclude noi-example-app --exclude noi-dark-pool-example
#        scripts/test_packaged.sh [cargo test flags, e.g. --all-features]
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Ask cargo where `cargo package` wrote (it honours CARGO_TARGET_DIR and build.target-dir) and
# take exactly the current version of each publishable crate, ignoring older .crate files.
META="$(cargo metadata --format-version 1 --no-deps --manifest-path "${ROOT_DIR}/Cargo.toml")"
PKG_DIR="$(jq -r .target_directory <<<"$META")/package"
# A stable location: test binaries embed their CARGO_MANIFEST_DIR, so a fresh temporary directory
# per run would leave cargo reusing binaries that point at a deleted one.
WS="$(jq -r .target_directory <<<"$META")/packaged-tests/ws"
rm -rf "$WS"
mkdir -p "$WS"
crates=()
while IFS= read -r name_version; do
  crate="${PKG_DIR}/${name_version}.crate"
  if [ ! -f "$crate" ]; then
    echo "missing ${crate}; run cargo package first" >&2
    exit 1
  fi
  crates+=("$crate")
done < <(jq -r '.packages[] | select(.publish != []) | "\(.name)-\(.version)"' <<<"$META")

members=()
patches=()
for crate in "${crates[@]}"; do
  # -m: stamp files with the current time so cargo rebuilds them (archives keep old mtimes).
  tar -xmzf "$crate" -C "$WS"
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
CARGO_TARGET_DIR="$(jq -r .target_directory <<<"$META")/packaged-tests/target" cargo test --workspace "$@"
