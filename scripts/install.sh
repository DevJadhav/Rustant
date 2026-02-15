#!/usr/bin/env bash
# Rustant installer script
# Usage: curl -fsSL https://raw.githubusercontent.com/DevJadhav/Rustant/main/scripts/install.sh | bash
# Options:
#   --version <version>   Install a specific version (e.g., 1.0.0)

set -euo pipefail

REPO="DevJadhav/Rustant"
BINARY_NAME="rustant"
INSTALL_DIR="/usr/local/bin"
VERSION=""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}[info]${NC} $1"; }
warn() { echo -e "${YELLOW}[warn]${NC} $1"; }
error() { echo -e "${RED}[error]${NC} $1"; exit 1; }

# Parse arguments
parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --version)
                shift
                VERSION="${1:-}"
                [ -z "$VERSION" ] && error "--version requires a value"
                ;;
            --help|-h)
                echo "Usage: install.sh [--version <version>]"
                echo ""
                echo "Options:"
                echo "  --version <version>   Install a specific version (e.g., 1.0.0)"
                echo "  --help, -h            Show this help message"
                exit 0
                ;;
            *)
                warn "Unknown argument: $1"
                ;;
        esac
        shift
    done
}

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

# Compute SHA256 hash (cross-platform)
compute_sha256() {
    local file="$1"
    if command -v sha256sum &>/dev/null; then
        sha256sum "$file" | cut -d' ' -f1
    elif command -v shasum &>/dev/null; then
        shasum -a 256 "$file" | cut -d' ' -f1
    else
        warn "No SHA256 tool found, skipping checksum verification"
        echo ""
    fi
}

# Verify SHA256 checksum against published checksums
verify_checksum() {
    local archive="$1"
    local asset_name="$2"
    local version="$3"
    local tmpdir="$4"

    local checksums_url="https://github.com/${REPO}/releases/download/v${version}/checksums-sha256.txt"
    local checksums_file="${tmpdir}/checksums-sha256.txt"

    info "Downloading checksums..."
    if ! curl -fsSL "$checksums_url" -o "$checksums_file" 2>/dev/null; then
        warn "Checksums file not available, skipping verification"
        return 0
    fi

    local actual_hash
    actual_hash=$(compute_sha256 "$archive")
    if [ -z "$actual_hash" ]; then
        return 0
    fi

    local expected_hash
    expected_hash=$(grep "${asset_name}.tar.gz" "$checksums_file" | cut -d' ' -f1)
    if [ -z "$expected_hash" ]; then
        warn "No checksum found for ${asset_name}.tar.gz, skipping verification"
        return 0
    fi

    if [ "$actual_hash" != "$expected_hash" ]; then
        error "Checksum verification failed!\n  Expected: ${expected_hash}\n  Actual:   ${actual_hash}"
    fi

    info "Checksum verified successfully"
}

main() {
    parse_args "$@"

    info "Rustant installer"
    info ""

    local platform
    platform=$(detect_platform)
    info "Detected platform: ${platform}"

    if [ -z "$VERSION" ]; then
        info "Fetching latest release..."
        VERSION=$(get_latest_version)
        if [ -z "$VERSION" ]; then
            error "Could not determine latest version"
        fi
    fi
    info "Version: v${VERSION}"

    local asset_name="rustant-${platform}"
    local download_url="https://github.com/${REPO}/releases/download/v${VERSION}/${asset_name}.tar.gz"

    info "Downloading ${download_url}..."
    local tmpdir
    tmpdir=$(mktemp -d)
    trap 'rm -rf "$tmpdir"' EXIT

    curl -fsSL "$download_url" -o "${tmpdir}/${asset_name}.tar.gz"

    verify_checksum "${tmpdir}/${asset_name}.tar.gz" "$asset_name" "$VERSION" "$tmpdir"

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
    info "Rustant v${VERSION} installed successfully!"
    info "Run 'rustant --help' to get started."
}

main "$@"
