#!/usr/bin/env bash
# Rustant installer script
# Usage: curl -fsSL https://raw.githubusercontent.com/DevJadhav/Rustant/main/scripts/install.sh | bash

set -euo pipefail

REPO="DevJadhav/Rustant"
BINARY_NAME="rustant"
INSTALL_DIR="/usr/local/bin"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}[info]${NC} $1"; }
warn() { echo -e "${YELLOW}[warn]${NC} $1"; }
error() { echo -e "${RED}[error]${NC} $1"; exit 1; }

# Detect OS and architecture
detect_platform() {
    local os arch

    case "$(uname -s)" in
        Linux*)  os="linux" ;;
        Darwin*) os="macos" ;;
        *)       error "Unsupported OS: $(uname -s)" ;;
    esac

    case "$(uname -m)" in
        x86_64)  arch="x86_64" ;;
        aarch64) arch="aarch64" ;;
        arm64)   arch="aarch64" ;;
        *)       error "Unsupported architecture: $(uname -m)" ;;
    esac

    echo "${os}-${arch}"
}

# Get the latest release version
get_latest_version() {
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | \
        grep '"tag_name"' | sed -E 's/.*"v?([^"]+)".*/\1/'
}

main() {
    info "Rustant installer"
    info ""

    local platform
    platform=$(detect_platform)
    info "Detected platform: ${platform}"

    info "Fetching latest release..."
    local version
    version=$(get_latest_version)
    if [ -z "$version" ]; then
        error "Could not determine latest version"
    fi
    info "Latest version: v${version}"

    local asset_name="rustant-${platform}"
    local download_url="https://github.com/${REPO}/releases/download/v${version}/${asset_name}.tar.gz"

    info "Downloading ${download_url}..."
    local tmpdir
    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT

    curl -fsSL "$download_url" -o "${tmpdir}/${asset_name}.tar.gz"
    tar xzf "${tmpdir}/${asset_name}.tar.gz" -C "$tmpdir"

    local binary="${tmpdir}/${BINARY_NAME}"
    if [ ! -f "$binary" ]; then
        error "Binary not found in archive"
    fi

    chmod +x "$binary"

    info "Installing to ${INSTALL_DIR}/${BINARY_NAME}..."
    if [ -w "$INSTALL_DIR" ]; then
        mv "$binary" "${INSTALL_DIR}/${BINARY_NAME}"
    else
        warn "Requires sudo to install to ${INSTALL_DIR}"
        sudo mv "$binary" "${INSTALL_DIR}/${BINARY_NAME}"
    fi

    info ""
    info "Rustant v${version} installed successfully!"
    info "Run 'rustant --help' to get started."
}

main
