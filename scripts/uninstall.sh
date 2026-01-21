#!/bin/sh
set -eu

# Saorsa Canvas Uninstaller
# POSIX-compliant uninstallation script

# Configuration (must match install.sh)
INSTALL_DIR="$HOME/.local/bin"
DATA_DIR="$HOME/.local/share/saorsa-canvas"
BINARY_NAME="saorsa-canvas"

# macOS paths
MACOS_PLIST_NAME="com.saorsa.canvas.plist"
MACOS_PLIST_DIR="$HOME/Library/LaunchAgents"

# Linux paths
LINUX_SERVICE_NAME="saorsa-canvas.service"
LINUX_SERVICE_DIR="$HOME/.config/systemd/user"

# Colors
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

stop_macos_daemon() {
    if [ "$(uname -s)" = "Darwin" ]; then
        if launchctl list 2>/dev/null | grep -q "com.saorsa.canvas"; then
            log_info "Stopping macOS daemon..."
            launchctl unload "$MACOS_PLIST_DIR/$MACOS_PLIST_NAME" 2>/dev/null || true
        fi

        if [ -f "$MACOS_PLIST_DIR/$MACOS_PLIST_NAME" ]; then
            log_info "Removing launchd plist..."
            rm "$MACOS_PLIST_DIR/$MACOS_PLIST_NAME"
            log_success "Removed macOS daemon configuration"
        fi
    fi
}

stop_linux_daemon() {
    if [ "$(uname -s)" = "Linux" ] && command -v systemctl >/dev/null 2>&1; then
        if systemctl --user is-active --quiet saorsa-canvas 2>/dev/null; then
            log_info "Stopping Linux daemon..."
            systemctl --user stop saorsa-canvas || true
        fi

        if systemctl --user is-enabled --quiet saorsa-canvas 2>/dev/null; then
            log_info "Disabling Linux daemon..."
            systemctl --user disable saorsa-canvas || true
        fi

        if [ -f "$LINUX_SERVICE_DIR/$LINUX_SERVICE_NAME" ]; then
            log_info "Removing systemd service..."
            rm "$LINUX_SERVICE_DIR/$LINUX_SERVICE_NAME"
            systemctl --user daemon-reload
            log_success "Removed Linux daemon configuration"
        fi
    fi
}

remove_binary() {
    if [ -f "$INSTALL_DIR/$BINARY_NAME" ]; then
        log_info "Removing binary..."
        rm "$INSTALL_DIR/$BINARY_NAME"
        log_success "Removed $INSTALL_DIR/$BINARY_NAME"
    else
        log_warn "Binary not found at $INSTALL_DIR/$BINARY_NAME"
    fi
}

remove_data() {
    if [ -d "$DATA_DIR" ]; then
        log_info "Removing data directory..."
        rm -rf "$DATA_DIR"
        log_success "Removed $DATA_DIR"
    else
        log_warn "Data directory not found at $DATA_DIR"
    fi
}

show_help() {
    printf 'Saorsa Canvas Uninstaller

Usage: %s [OPTIONS]

Options:
    --keep-data    Keep data directory (logs, web assets)
    --help         Show this help message

This will:
1. Stop any running daemon (launchd/systemd)
2. Remove daemon configuration
3. Remove the binary from %s
4. Remove data from %s (unless --keep-data)
' "$0" "$INSTALL_DIR" "$DATA_DIR"
}

main() {
    keep_data=false

    # Parse arguments
    while [ $# -gt 0 ]; do
        case "$1" in
            --keep-data)
                keep_data=true
                shift
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
    printf "${YELLOW}╔═══════════════════════════════════════╗${NC}\n"
    printf "${YELLOW}║     Saorsa Canvas Uninstaller         ║${NC}\n"
    printf "${YELLOW}╚═══════════════════════════════════════╝${NC}\n"
    printf "\n"

    # Check if anything is installed
    if [ ! -f "$INSTALL_DIR/$BINARY_NAME" ] && [ ! -d "$DATA_DIR" ]; then
        log_warn "Saorsa Canvas does not appear to be installed"
        exit 0
    fi

    # Confirm uninstall
    printf "This will remove:\n"
    printf "  - Binary:  %s/%s\n" "$INSTALL_DIR" "$BINARY_NAME"
    if [ "$keep_data" = false ]; then
        printf "  - Data:    %s\n" "$DATA_DIR"
    else
        printf "  - Data:    (keeping)\n"
    fi
    printf "  - Daemons: launchd/systemd configuration\n"
    printf "\n"
    printf "Continue? [y/N] "
    read -r response
    case "$response" in
        [yY][eE][sS]|[yY])
            ;;
        *)
            log_info "Uninstall cancelled"
            exit 0
            ;;
    esac

    printf "\n"

    # Stop and remove daemons
    stop_macos_daemon
    stop_linux_daemon

    # Remove binary
    remove_binary

    # Remove data (unless --keep-data)
    if [ "$keep_data" = false ]; then
        remove_data
    else
        log_info "Keeping data directory at $DATA_DIR"
    fi

    # Success message
    printf "\n"
    printf "${GREEN}╔═══════════════════════════════════════╗${NC}\n"
    printf "${GREEN}║     Uninstall Complete!               ║${NC}\n"
    printf "${GREEN}╚═══════════════════════════════════════╝${NC}\n"
    printf "\n"

    if [ "$keep_data" = true ] && [ -d "$DATA_DIR" ]; then
        printf "Data preserved at: %s\n" "$DATA_DIR"
        printf "\n"
    fi

    printf "Thank you for using Saorsa Canvas!\n"
    printf "\n"
}

main "$@"
