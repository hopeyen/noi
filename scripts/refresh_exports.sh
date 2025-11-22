#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NOIR_TAG="v0.36.0"

ensure_nargo() {
  if command -v nargo >/dev/null 2>&1; then
    return
  fi

  echo "nargo not found; installing nargo_cli @ ${NOIR_TAG}"
  cargo install --locked --git https://github.com/noir-lang/noir --tag "${NOIR_TAG}" nargo_cli
}

refresh_program() {
  local program_dir="$1"
  echo "==> nargo export (${program_dir})"
  (cd "${program_dir}" && nargo export)
}

main() {
  ensure_nargo

  refresh_program "${ROOT_DIR}/examples/demo/noir"
  refresh_program "${ROOT_DIR}/examples/dark-pool/noir"

  echo "Exports refreshed. Remember to commit any changed files under examples/*/noir/export/."
}

main "$@"
