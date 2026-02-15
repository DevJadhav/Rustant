# Installation

## From Cargo (Recommended)

```bash
cargo install rustant
```

## From Cargo with Binary Install

If you have `cargo-binstall`, you can install pre-built binaries:

```bash
cargo binstall rustant
```

## Homebrew (macOS/Linux)

```bash
brew install DevJadhav/rustant/rustant
```

## Shell Installer (Linux/macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/DevJadhav/Rustant/main/scripts/install.sh | bash
```

You can also pin a specific version:

```bash
curl -fsSL https://raw.githubusercontent.com/DevJadhav/Rustant/main/scripts/install.sh | bash -s -- --version 1.0.0
```

## Pre-built Binaries

Download the latest release for your platform from the
[GitHub Releases](https://github.com/DevJadhav/Rustant/releases) page:

| Platform | Binary |
|----------|--------|
| Linux x86_64 | `rustant-linux-x86_64.tar.gz` |
| Linux aarch64 | `rustant-linux-aarch64.tar.gz` |
| macOS x86_64 | `rustant-macos-x86_64.tar.gz` |
| macOS Apple Silicon | `rustant-macos-aarch64.tar.gz` |
| Windows x86_64 | `rustant-windows-x86_64.exe` |

Each release includes a `checksums-sha256.txt` file for verification.

## Build from Source

```bash
git clone https://github.com/DevJadhav/Rustant.git
cd Rustant
cargo build --workspace --release
```

The binary will be at `target/release/rustant`.

## Self-Update

Once installed, Rustant can update itself:

```bash
rustant update check     # Check for new versions
rustant update install   # Download and install the latest version
```

## Verify Installation

```bash
rustant --version
rustant --help
```
