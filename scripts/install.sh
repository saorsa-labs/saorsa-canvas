#!/bin/sh
set -eu

# Saorsa Canvas Installer
# POSIX-compliant installation script

# Configuration
REPO="saorsa-labs/saorsa-canvas"
INSTALL_DIR="$HOME/.local/bin"
DATA_DIR="$HOME/.local/share/saorsa-canvas"
BINARY_NAME="saorsa-canvas"
PORT="9473"

# Colors (if terminal supports them)
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    BLUE='\033[0;34m'
    NC='\033[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    NC=''
fi

log_info() {
    printf "${BLUE}[INFO]${NC} %s\n" "$1"
}

log_success() {
    printf "${GREEN}[OK]${NC} %s\n" "$1"
}

log_warn() {
    printf "${YELLOW}[WARN]${NC} %s\n" "$1"
}

log_error() {
    printf "${RED}[ERROR]${NC} %s\n" "$1" >&2
}

detect_os() {
    os="$(uname -s)"
    case "$os" in
        Darwin)
            echo "darwin"
            ;;
        Linux)
            echo "linux"
            ;;
        *)
            log_error "Unsupported operating system: $os"
            exit 1
            ;;
    esac
}

detect_arch() {
    arch="$(uname -m)"
    case "$arch" in
        arm64|aarch64)
            echo "aarch64"
            ;;
        x86_64|amd64)
            echo "x86_64"
            ;;
        *)
            log_error "Unsupported architecture: $arch"
            exit 1
            ;;
    esac
}

get_target() {
    os="$1"
    arch="$2"
    
    case "${os}-${arch}" in
        darwin-aarch64)
            echo "aarch64-apple-darwin"
            ;;
        darwin-x86_64)
            echo "x86_64-apple-darwin"
            ;;
        linux-x86_64)
            echo "x86_64-unknown-linux-gnu"
            ;;
        linux-aarch64)
            echo "aarch64-unknown-linux-gnu"
            ;;
        *)
            log_error "Unsupported platform: ${os}-${arch}"
            exit 1
            ;;
    esac
}

get_latest_version() {
    api_url="https://api.github.com/repos/${REPO}/releases/latest"
    
    if command -v curl >/dev/null 2>&1; then
        version="$(curl -fsSL "$api_url" 2>/dev/null | grep '"tag_name"' | sed -E 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')"
    elif command -v wget >/dev/null 2>&1; then
        version="$(wget -qO- "$api_url" 2>/dev/null | grep '"tag_name"' | sed -E 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')"
    else
        log_error "Neither curl nor wget found. Please install one of them."
        exit 1
    fi
    
    if [ -z "$version" ]; then
        log_error "Failed to fetch latest version from GitHub API"
        exit 1
    fi
    
    echo "$version"
}

download() {
    url="$1"
    output="$2"
    
    log_info "Downloading from $url"
    
    if command -v curl >/dev/null 2>&1; then
        if ! curl -fsSL -o "$output" "$url"; then
            log_error "Download failed"
            exit 1
        fi
    elif command -v wget >/dev/null 2>&1; then
        if ! wget -q -O "$output" "$url"; then
            log_error "Download failed"
            exit 1
        fi
    else
        log_error "Neither curl nor wget found"
        exit 1
    fi
    
    # Verify download
    if [ ! -f "$output" ] || [ ! -s "$output" ]; then
        log_error "Downloaded file is empty or missing"
        exit 1
    fi
    
    log_success "Download complete"
}

install_binary() {
    tarball="$1"
    version="$2"
    
    # Create directories
    log_info "Creating installation directories"
    mkdir -p "$INSTALL_DIR"
    mkdir -p "$DATA_DIR"
    
    # Create temp extraction directory
    extract_dir="$(mktemp -d)"
    trap 'rm -rf "$extract_dir"' EXIT
    
    # Extract tarball
    log_info "Extracting archive"
    tar -xzf "$tarball" -C "$extract_dir"
    
    # Find and install binary
    if [ -f "$extract_dir/$BINARY_NAME" ]; then
        mv "$extract_dir/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
    elif [ -f "$extract_dir/bin/$BINARY_NAME" ]; then
        mv "$extract_dir/bin/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
    else
        # Search for binary
        binary_path="$(find "$extract_dir" -name "$BINARY_NAME" -type f 2>/dev/null | head -1)"
        if [ -n "$binary_path" ]; then
            mv "$binary_path" "$INSTALL_DIR/$BINARY_NAME"
        else
            log_error "Binary not found in archive"
            exit 1
        fi
    fi
    
    chmod +x "$INSTALL_DIR/$BINARY_NAME"
    log_success "Installed binary to $INSTALL_DIR/$BINARY_NAME"
    
    # Install web assets if present
    if [ -d "$extract_dir/web" ]; then
        rm -rf "$DATA_DIR/web"
        mv "$extract_dir/web" "$DATA_DIR/web"
        log_success "Installed web assets to $DATA_DIR/web"
    fi
}

check_path() {
    case ":$PATH:" in
        *":$INSTALL_DIR:"*)
            return 0
            ;;
    esac
    
    log_warn "$INSTALL_DIR is not in your PATH"
    printf "\n"
    printf "Add it to your shell configuration:\n"
    printf "\n"
    
    # Detect shell
    shell_name="$(basename "${SHELL:-/bin/sh}")"
    case "$shell_name" in
        zsh)
            printf "  echo 'export PATH=\"\\\$HOME/.local/bin:\\\$PATH\"' >> ~/.zshrc\n"
            printf "  source ~/.zshrc\n"
            ;;
        bash)
            printf "  echo 'export PATH=\"\\\$HOME/.local/bin:\\\$PATH\"' >> ~/.bashrc\n"
            printf "  source ~/.bashrc\n"
            ;;
        *)
            printf "  export PATH=\"\\\$HOME/.local/bin:\\\$PATH\"\n"
            ;;
    esac
    printf "\n"
    return 1
}

show_help() {
    printf 'Saorsa Canvas Installer\n\nUsage: %s [OPTIONS]\n\nOptions:\n    --version VERSION   Install specific version (e.g., v0.2.0-alpha.1)\n    --help              Show this help message\n\nExamples:\n    %s                          Install latest version\n    %s --version v0.2.0-alpha.1 Install specific version\n' "$0" "$0" "$0"
}

main() {
    version=""
    
    # Parse arguments
    while [ $# -gt 0 ]; do
        case "$1" in
            --version)
                if [ $# -lt 2 ]; then
                    log_error "--version requires a value"
                    exit 1
                fi
                version="$2"
                shift 2
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                exit 1
                ;;
        esac
    done
    
    printf "\n"
    printf "${BLUE}"=[Saorsa Canvas Installer]="${NC}\n"
    printf "\n"
    
    # Detect platform
    os="$(detect_os)"
    arch="$(detect_arch)"
    target="$(get_target "$os" "$arch")"
    
    log_info "Detected platform: $os-$arch"
    log_info "Target triple: $target"
    
    # Get version
    if [ -z "$version" ]; then
        log_info "Fetching latest version..."
        version="$(get_latest_version)"
    fi
    log_info "Installing version: $version"
    
    # Build download URL
    tarball_name="saorsa-canvas-${version}-${target}.tar.gz"
    download_url="https://github.com/${REPO}/releases/download/${version}/${tarball_name}"
    
    # Download to temp file
    tmp_dir="$(mktemp -d)"
    tmp_tarball="$tmp_dir/$tarball_name"
    trap 'rm -rf "$tmp_dir"' EXIT
    
    download "$download_url" "$tmp_tarball"
    
    # Install
    install_binary "$tmp_tarball" "$version"
    
    # Check PATH
    printf "\n"
    path_ok=true
    if ! check_path; then
        path_ok=false
    fi
    
    # Success message
    printf "\n"
    printf "${GREEN}"=[Installation Complete!]="${NC}\n"
    printf "\n"
    printf "  Version:  %s\n" "$version"
    printf "  Binary:   %s/%s\n" "$INSTALL_DIR" "$BINARY_NAME"
    printf "  Data:     %s\n" "$DATA_DIR"
    printf "\n"
    
    if [ "$path_ok" = true ]; then
        printf "Start the server:\n"
        printf "\n"
        printf "  %s\n" "$BINARY_NAME"
    else
        printf "After updating your PATH, start the server:\n"
        printf "\n"
        printf "  %s\n" "$BINARY_NAME"
    fi
    
    printf "\n"
    printf "Then open: %shttp://localhost:%s%s\n" "$BLUE" "$PORT" "$NC"
    printf "\n"
}

main "$@"
