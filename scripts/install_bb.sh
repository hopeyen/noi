#!/usr/bin/env bash
# Installs the Barretenberg `bb` CLI that matches the pinned Noir release into target/bb-bin/.
# Unlike `bbup`, it does not modify shell rc files. Prints the binary path on success.
#
# Usage: scripts/install_bb.sh            # then: export NOI_BB_BIN="$(scripts/install_bb.sh)"
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# bb release paired with Noir v1.0.0-beta.26 (see `@aztec/bb.js` in Noir's integration tests).
# Keep in sync with `noi_core::prove::BB_VERSION`.
BB_VERSION="5.0.0"
INSTALL_DIR="${ROOT_DIR}/target/bb-bin/${BB_VERSION}"
BB_BIN="${INSTALL_DIR}/bb"

if [ -x "${BB_BIN}" ]; then
  echo "${BB_BIN}"
  exit 0
fi

machine="$(uname -m)"
# A shell running under Rosetta reports x86_64 on Apple Silicon; the amd64 bb build then crashes
# with "Illegal instruction", so ask the hardware instead.
if [ "$(uname -s)" = "Darwin" ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null)" = "1" ]; then
  machine="arm64"
fi
case "${machine}" in
  arm64|aarch64) arch="arm64" ;;
  x86_64) arch="amd64" ;;
  *) echo "Unsupported architecture: ${machine}" >&2; exit 1 ;;
esac
case "$(uname -s)" in
  Darwin) platform="darwin" ;;
  Linux) platform="linux" ;;
  *) echo "Unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac

artifact="barretenberg-${arch}-${platform}.tar.gz"
primary="https://github.com/AztecProtocol/barretenberg/releases/download/v${BB_VERSION}/${artifact}"
fallback="https://github.com/AztecProtocol/aztec-packages/releases/download/v${BB_VERSION}/${artifact}"

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT
echo "Downloading ${artifact} (bb ${BB_VERSION})" >&2
curl -fsSL "${primary}" -o "${tmp}/bb.tar.gz" || curl -fsSL "${fallback}" -o "${tmp}/bb.tar.gz"
mkdir -p "${INSTALL_DIR}"
tar -xzf "${tmp}/bb.tar.gz" -C "${INSTALL_DIR}"
chmod +x "${BB_BIN}"
if ! "${BB_BIN}" --version >&2; then
  rm -rf "${INSTALL_DIR}"
  echo "Downloaded bb does not run on this machine" >&2
  exit 1
fi
echo "${BB_BIN}"
