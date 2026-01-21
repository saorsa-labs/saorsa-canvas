#!/bin/bash
set -euo pipefail

# Saorsa Canvas Release Bundling Script
# Usage: ./scripts/bundle-release.sh [--target TARGET] [--version VERSION]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Defaults
TARGET="${TARGET:-}"
VERSION="${VERSION:-$(grep '^version' "$PROJECT_ROOT/Cargo.toml" | head -1 | cut -d'"' -f2)}"
OUTPUT_DIR="$PROJECT_ROOT/dist"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --target)
            TARGET="$2"
            shift 2
            ;;
        --version)
            VERSION="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [--target TARGET] [--version VERSION]"
            echo ""
            echo "Options:"
            echo "  --target TARGET    Build target (e.g., x86_64-apple-darwin)"
            echo "  --version VERSION  Version string (default: from Cargo.toml)"
            echo ""
            echo "If --target is not specified, builds for the current platform."
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Detect target if not specified
if [[ -z "$TARGET" ]]; then
    case "$(uname -s)-$(uname -m)" in
        Darwin-arm64)
            TARGET="aarch64-apple-darwin"
            ;;
        Darwin-x86_64)
            TARGET="x86_64-apple-darwin"
            ;;
        Linux-x86_64)
            TARGET="x86_64-unknown-linux-gnu"
            ;;
        *)
            echo "Error: Unsupported platform $(uname -s)-$(uname -m)"
            exit 1
            ;;
    esac
fi

echo "=== Saorsa Canvas Release Builder ==="
echo "Version: $VERSION"
echo "Target:  $TARGET"
echo "Output:  $OUTPUT_DIR"
echo ""

# Create output directory
rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR/staging"

# Step 1: Build server binary
echo "==> Building server binary for $TARGET..."
cd "$PROJECT_ROOT"
cargo build --release --target "$TARGET" -p saorsa-canvas

# Step 2: Build WASM
echo "==> Building WASM..."
cd "$PROJECT_ROOT/canvas-app"
if ! command -v wasm-pack &> /dev/null; then
    echo "Error: wasm-pack not installed. Install with: curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh"
    exit 1
fi
wasm-pack build --target web --release

# Step 3: Copy binary
echo "==> Copying binary..."
cp "$PROJECT_ROOT/target/$TARGET/release/saorsa-canvas" "$OUTPUT_DIR/staging/"

# Step 4: Copy web assets
echo "==> Copying web assets..."
cp -r "$PROJECT_ROOT/web" "$OUTPUT_DIR/staging/"

# Step 5: Copy WASM output
echo "==> Copying WASM build..."
mkdir -p "$OUTPUT_DIR/staging/web/pkg"
cp -r "$PROJECT_ROOT/canvas-app/pkg/"* "$OUTPUT_DIR/staging/web/pkg/"

# Step 6: Create tarball
ARCHIVE_NAME="saorsa-canvas-v$VERSION-$TARGET.tar.gz"
echo "==> Creating $ARCHIVE_NAME..."
cd "$OUTPUT_DIR/staging"
tar -czvf "../$ARCHIVE_NAME" *

# Step 7: Cleanup
rm -rf "$OUTPUT_DIR/staging"

echo ""
echo "=== Release bundle created ==="
echo "Archive: $OUTPUT_DIR/$ARCHIVE_NAME"
echo ""
echo "Test locally:"
echo "  tar -xzf $OUTPUT_DIR/$ARCHIVE_NAME -C /tmp/saorsa-test"
echo "  cd /tmp/saorsa-test && ./saorsa-canvas"
