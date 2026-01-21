# Phase 1: Release Workflow

## Objective
Create GitHub Actions workflow that builds cross-platform binaries and attaches them to GitHub Releases.

## Tasks

### Task 1.1: Create Release Workflow File
**File**: `.github/workflows/release.yml`

Create workflow that triggers on version tags (v*):
- Build matrix: macOS-arm64, macOS-x64, Linux-x64
- Install Rust toolchain
- Build release binary
- Bundle web assets
- Create tarball with binary + assets
- Upload to release

### Task 1.2: Asset Bundling Script
**File**: `scripts/bundle-release.sh`

Script to bundle release artifacts:
- Copy binary to staging directory
- Copy web/ directory (excluding pkg/ if not built)
- Build WASM (wasm-pack build)
- Create tarball: saorsa-canvas-{version}-{target}.tar.gz

### Task 1.3: Update Cargo.toml Metadata
**File**: `Cargo.toml` (workspace root)

Ensure metadata is complete:
- description
- repository
- homepage
- license
- keywords
- categories

### Task 1.4: Test Release Workflow
- Create test tag (v0.2.0-alpha.1)
- Verify all 3 targets build
- Verify assets attached to release
- Download and test each tarball

### Task 1.5: Add Release README
**File**: `RELEASE.md`

Document the release process:
- How to cut a release
- Supported targets
- Testing checklist

## Acceptance Criteria
- [ ] Release workflow triggers on v* tags
- [ ] Builds succeed for macOS-arm64, macOS-x64, Linux-x64
- [ ] Tarballs contain binary + web assets
- [ ] Tarballs downloadable from GitHub Release page

## Dependencies
- Existing CI workflow (for reference)
- wasm-pack for WASM build step

## Estimated Complexity
Medium - workflow configuration and scripting
