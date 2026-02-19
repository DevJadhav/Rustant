# Release Process

## Version Management

All crates share the workspace version in the root `Cargo.toml`:

```toml
[workspace.package]
version = "1.0.1"
```

## Crate Publish Order

Crates must be published in dependency order:

```
Tier 1: rustant-core              (no internal deps)
Tier 2: rustant-tools             (depends on core)
        rustant-plugins           (depends on core)
Tier 3: rustant-security          (depends on core + tools)
        rustant-ml                (depends on core + tools)
Tier 4: rustant-mcp               (depends on core + tools + security)
Tier 5: rustant (CLI)             (depends on all above)
```

Each tier requires a 30-second delay for crates.io index propagation.

## Release Workflow

1. **Update version** in root `Cargo.toml` workspace section
2. **Update CHANGELOG.md** — Move [Unreleased] entries to new version section
3. **Create and push tag**: `git tag v1.0.2 && git push origin v1.0.2`
4. **Automated pipeline** (`.github/workflows/release.yml`):
   - Build binaries for 5 platforms (Linux x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64)
   - Create GitHub Release with checksums
   - Publish crates to crates.io in dependency order
   - Update Homebrew formula

## Pre-Release Checklist

- [ ] All tests pass: `cargo test --workspace --exclude rustant-ui`
- [ ] Clippy clean: `cargo clippy --workspace --all-targets --exclude rustant-ui -- -D warnings`
- [ ] CHANGELOG has entries for the version
- [ ] Version bumped in root `Cargo.toml`
- [ ] Dry-run publish: `cargo publish -p rustant-core --dry-run`

## Platform Targets

| Target | OS | Architecture |
|--------|----|----|
| `x86_64-unknown-linux-gnu` | Linux | x86_64 |
| `aarch64-unknown-linux-gnu` | Linux | ARM64 (cross-compiled) |
| `x86_64-apple-darwin` | macOS | Intel |
| `aarch64-apple-darwin` | macOS | Apple Silicon |
| `x86_64-pc-windows-msvc` | Windows | x86_64 |

## Homebrew

The release pipeline automatically updates the Homebrew tap at `DevJadhav/homebrew-rustant` with SHA256 checksums for each platform.
