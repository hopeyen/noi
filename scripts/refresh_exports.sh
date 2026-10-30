#!/usr/bin/env bash
# Re-runs `nargo export` for the example Noir packages with the pinned nargo release and copies the
# demo artifacts into the `noi` crate's test fixtures.
#
# nargo is taken from $NARGO_BIN, a matching `nargo` on PATH, a previously installed copy under
# target/nargo-bin/, a prebuilt GitHub release, or (last resort) `cargo install` from source.
# The user's own Rust toolchain and cargo cache are used; nothing outside target/ is modified.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Keep in sync with the Noir crates in Cargo.toml and `noi_core::NOIR_VERSION`.
NOIR_TAG="v1.0.0"
NOIR_VERSION="${NOIR_TAG#v}"
INSTALL_DIR="${ROOT_DIR}/target/nargo-bin/${NOIR_VERSION}"
NARGO_BIN="${NARGO_BIN:-}"

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  # A shell running under Rosetta reports x86_64 on Apple Silicon; ask the hardware instead.
  if [ "$os" = "Darwin" ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null)" = "1" ]; then
    arch="arm64"
  fi
  case "$arch" in
    x86_64) arch="x86_64" ;;
    arm64|aarch64) arch="aarch64" ;;
    *) echo "Unsupported architecture: $arch" >&2; return 1 ;;
  esac
  case "$os" in
    Darwin) echo "${arch}-apple-darwin" ;;
    Linux) echo "${arch}-unknown-linux-gnu" ;;
    *) echo "Unsupported OS: $os" >&2; return 1 ;;
  esac
}

is_pinned_nargo() {
  [ -x "$1" ] && "$1" --version 2>/dev/null | grep -q "nargo version = ${NOIR_VERSION}\$"
}

download_nargo_release() {
  local target tmpdir bin_path
  target="$(detect_target)"
  tmpdir="$(mktemp -d)"
  # Release assets were renamed from `nargo-*` to `noir-*`; accept either.
  for asset in "noir-${target}.tar.gz" "nargo-${target}.tar.gz"; do
    local url="https://github.com/noir-lang/noir/releases/download/${NOIR_TAG}/${asset}"
    if curl -fsSL "$url" -o "${tmpdir}/nargo.tar.gz"; then
      echo "Downloaded ${url}" >&2
      tar -xzf "${tmpdir}/nargo.tar.gz" -C "$tmpdir"
      bin_path="$(find "$tmpdir" -type f -name nargo -perm -111 | head -n 1)"
      if [ -n "$bin_path" ]; then
        mkdir -p "$INSTALL_DIR"
        cp "$bin_path" "${INSTALL_DIR}/nargo"
        rm -rf "$tmpdir"
        return 0
      fi
    fi
  done
  rm -rf "$tmpdir"
  return 1
}

ensure_nargo() {
  if [ -n "$NARGO_BIN" ]; then
    is_pinned_nargo "$NARGO_BIN" && return
    echo "NARGO_BIN=$NARGO_BIN is not nargo ${NOIR_VERSION}" >&2
    exit 1
  fi
  local path_bin
  path_bin="$(command -v nargo || true)"
  if [ -n "$path_bin" ] && is_pinned_nargo "$path_bin"; then
    NARGO_BIN="$path_bin"
    return
  fi
  if is_pinned_nargo "${INSTALL_DIR}/nargo"; then
    NARGO_BIN="${INSTALL_DIR}/nargo"
    return
  fi
  if download_nargo_release && is_pinned_nargo "${INSTALL_DIR}/nargo"; then
    NARGO_BIN="${INSTALL_DIR}/nargo"
    return
  fi
  echo "No prebuilt nargo ${NOIR_VERSION} for this platform; building from source" >&2
  cargo install --locked --git https://github.com/noir-lang/noir --tag "${NOIR_TAG}" \
    --root "${ROOT_DIR}/target/nargo-install" nargo_cli
  mkdir -p "$INSTALL_DIR"
  cp "${ROOT_DIR}/target/nargo-install/bin/nargo" "${INSTALL_DIR}/nargo"
  NARGO_BIN="${INSTALL_DIR}/nargo"
}

refresh_program() {
  echo "==> nargo export ($1)"
  # nargo only adds files; clear old artifacts so removed `#[export]`s show up as deletions.
  rm -f "$1"/export/*.json
  (cd "$1" && "${NARGO_BIN}" export)
  # Artifacts embed absolute source paths in `file_map` (debug info only). Make them relative to
  # the repository so committed exports are byte-identical on every machine and CI.
  python3 - "$ROOT_DIR" "$1"/export/*.json <<'PY'
import json, os, sys
root = os.path.realpath(sys.argv[1])
for path in sys.argv[2:]:
    with open(path) as f:
        artifact = json.load(f)
    for entry in artifact.get("file_map", {}).values():
        src = os.path.realpath(entry.get("path", ""))
        if src.startswith(root + os.sep):
            entry["path"] = os.path.relpath(src, root)
    with open(path, "w") as f:
        json.dump(artifact, f, separators=(",", ":"))
PY
}

main() {
  ensure_nargo
  "${NARGO_BIN}" --version

  refresh_program "${ROOT_DIR}/examples/demo/noir"
  refresh_program "${ROOT_DIR}/examples/dark-pool/noir"

  # The `noi` and `noi-core` tests and doctests use copies of the demo artifacts, because a
  # published crate cannot read files outside its own package.
  for fixtures in crates/noi/tests/fixtures/demo crates/noi-core/tests/fixtures/demo; do
    rm -f "${ROOT_DIR}/${fixtures}"/*.json
    cp "${ROOT_DIR}"/examples/demo/noir/export/*.json "${ROOT_DIR}/${fixtures}/"
  done

  echo "Exports refreshed. Commit the changes under examples/*/noir/export/ and crates/*/tests/fixtures/demo/."
}

main "$@"
