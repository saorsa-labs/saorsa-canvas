# Saorsa Canvas Release Process

## Cutting a Release

### 1. Prepare the Release

1. Ensure all tests pass:
   ```bash
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   cargo fmt --all -- --check
   ```

2. Update the version in `Cargo.toml`:
   ```toml
   [workspace.package]
   version = "X.Y.Z"
   ```

3. Commit the version change:
   ```bash
   git add Cargo.toml
   git commit -m "chore: bump version to vX.Y.Z"
   git push
   ```

### 2. Create the Tag

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

This triggers the release workflow which:
- Builds for macOS (arm64 & x64) and Linux (x64)
- Creates release tarballs with binary + web assets
- Creates a GitHub Release with all artifacts attached

### 3. Monitor the Release

Check the [Actions tab](https://github.com/saorsa-labs/saorsa-canvas/actions) for the release workflow status.

## Supported Targets

| Platform | Architecture | Target |
|----------|--------------|--------|
| macOS | Apple Silicon | `aarch64-apple-darwin` |
| macOS | Intel | `x86_64-apple-darwin` |
| Linux | x64 | `x86_64-unknown-linux-gnu` |

## Release Contents

Each tarball contains:
- `canvas-server` - The main binary
- `web/` - Static web assets and PWA shell
- `web/pkg/` - WASM build output

## Testing a Release

### Local Build

```bash
# Build for current platform
./scripts/bundle-release.sh

# Build for specific target
./scripts/bundle-release.sh --target aarch64-apple-darwin --version 0.2.0
```

### Test the Archive

```bash
# Extract
tar -xzf dist/saorsa-canvas-vX.Y.Z-TARGET.tar.gz -C /tmp/saorsa-test

# Run
cd /tmp/saorsa-test
./saorsa-canvas

# Open browser to http://localhost:9473
```

### Pre-release Checklist

Before tagging a release:

- [ ] All CI checks pass on main
- [ ] WASM build succeeds locally
- [ ] Server starts without errors
- [ ] WebSocket connection works
- [ ] Canvas renders elements correctly
- [ ] MCP endpoint responds to requests

### Post-release Checklist

After release is published:

- [ ] All three platform builds succeed
- [ ] Tarballs are attached to release
- [ ] Download and test each tarball
- [ ] Update release notes if needed

## Version Scheme

- **Alpha** (`v0.2.0-alpha.1`): Early testing, API may change
- **Beta** (`v0.2.0-beta.1`): Feature complete, stabilizing
- **Release** (`v0.2.0`): Production ready

## Troubleshooting

### Build Fails on Linux

Ensure fontconfig is installed:
```bash
sudo apt-get install libfontconfig1-dev libfreetype6-dev
```

### WASM Build Fails

Ensure wasm-pack is installed:
```bash
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
```

### Binary Won't Run

Check for missing dynamic libraries:
```bash
# macOS
otool -L canvas-server

# Linux
ldd canvas-server
```
