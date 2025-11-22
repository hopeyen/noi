#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_HOME="${ROOT_DIR}/target/cargo-home"
RUSTUP_HOME="${ROOT_DIR}/target/rustup-home"
export CARGO_HOME
export RUSTUP_HOME
NOIR_TAG="v0.36.0"
NARGO_BIN="${NARGO_BIN:-}"

detect_target() {
  local os="$(uname -s)"
  local arch="$(uname -m)"
  case "$os" in
    Darwin) os_triple="apple-darwin" ;;
    Linux) os_triple="unknown-linux-gnu" ;;
    *) echo "Unsupported OS: $os" >&2; exit 1 ;;
  esac
  case "$arch" in
    x86_64) arch_triple="x86_64" ;;
    arm64|aarch64) arch_triple="aarch64" ;;
    *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
  esac
  echo "${arch_triple}-${os_triple}"
}

download_nargo_release() {
  local target
  target="$(detect_target)"
  local api="https://api.github.com/repos/noir-lang/noir/releases/tags/${NOIR_TAG}"
  local json
  if ! json="$(curl -fsSL "$api")"; then
    echo "Failed to fetch release metadata from $api" >&2
    return 1
  fi
  local url
  if ! url="$(printf '%s' "$json" | python3 - "$target" <<'PY'
import json, sys

data = json.load(sys.stdin)
target = sys.argv[1]
for asset in data.get("assets", []):
    name = asset.get("name", "")
    if "nargo" in name and target in name and name.endswith(".tar.gz"):
        print(asset["browser_download_url"])
        break
else:
    sys.exit(1)
PY
)"; then
    echo "Unable to locate prebuilt nargo binary for target $target" >&2
    return 1
  fi

  local tmpdir
  tmpdir="$(mktemp -d)"
  local tarball="$tmpdir/nargo.tar.gz"
  echo "Downloading nargo binary from $url"
  curl -fsSL "$url" -o "$tarball"
  tar -xzf "$tarball" -C "$tmpdir"
  local bin_path
  bin_path="$(find "$tmpdir" -type f -name nargo -perm -111 | head -n 1)"
  if [ -z "$bin_path" ]; then
    echo "Downloaded archive did not contain a nargo binary" >&2
    rm -rf "$tmpdir"
    return 1
  fi
  local install_dir="${ROOT_DIR}/target/nargo-bin"
  mkdir -p "$install_dir"
  cp "$bin_path" "$install_dir/nargo-${target}"
  chmod +x "$install_dir/nargo-${target}"
  NARGO_BIN="$install_dir/nargo-${target}"
  echo "Installed nargo to $NARGO_BIN"
  rm -rf "$tmpdir"
}

ensure_nargo() {
  if [ -n "${NARGO_BIN}" ] && [ -x "${NARGO_BIN}" ]; then
    return
  fi

  if command -v nargo >/dev/null 2>&1; then
    NARGO_BIN="$(command -v nargo)"
    return
  fi

  local workspace_bin="${ROOT_DIR}/target/nargo/debug/nargo"
  if [ -x "$workspace_bin" ]; then
    NARGO_BIN="$workspace_bin"
    return
  fi

  local cached_bin="${ROOT_DIR}/target/nargo-bin/nargo-$(detect_target)"
  if [ -x "$cached_bin" ]; then
    NARGO_BIN="$cached_bin"
    return
  fi

  if download_nargo_release; then
    return
  fi

  echo "Failed to download prebuilt nargo; attempting cargo install (may require Rust downloads)" >&2
  cargo install --locked --git https://github.com/noir-lang/noir --tag "${NOIR_TAG}" nargo_cli
  NARGO_BIN="$(command -v nargo)"
}

refresh_program() {
  local program_dir="$1"
  echo "==> nargo export (${program_dir})"
  (cd "${program_dir}" && "${NARGO_BIN}" export)
}

main() {
  ensure_nargo

  refresh_program "${ROOT_DIR}/examples/demo/noir"
  refresh_program "${ROOT_DIR}/examples/dark-pool/noir"

  echo "Exports refreshed. Remember to commit any changed files under examples/*/noir/export/."
}

main "$@"
