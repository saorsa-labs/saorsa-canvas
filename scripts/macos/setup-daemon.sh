#!/bin/sh
set -eu

# Saorsa Canvas macOS Daemon Setup
# Installs launchd user agent for auto-start

# Configuration (must match install.sh)
INSTALL_DIR="$HOME/.local/bin"
DATA_DIR="$HOME/.local/share/saorsa-canvas"
PLIST_NAME="com.saorsa.canvas.plist"
PLIST_DIR="$HOME/Library/LaunchAgents"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

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

check_macos() {
    if [ "$(uname -s)" != "Darwin" ]; then
        log_error "This script is for macOS only"
        exit 1
    fi
}

check_binary() {
    if [ ! -x "$INSTALL_DIR/saorsa-canvas" ]; then
        log_error "saorsa-canvas not found at $INSTALL_DIR/saorsa-canvas"
        log_error "Please run install.sh first"
        exit 1
    fi
}

stop_service() {
    if launchctl list | grep -q "com.saorsa.canvas"; then
        log_info "Stopping existing service..."
        launchctl unload "$PLIST_DIR/$PLIST_NAME" 2>/dev/null || true
    fi
}

install_plist() {
    log_info "Installing launchd plist..."

    # Create directories
    mkdir -p "$PLIST_DIR"
    mkdir -p "$DATA_DIR/logs"

    # Get the plist template
    if [ -f "$SCRIPT_DIR/$PLIST_NAME" ]; then
        plist_template="$SCRIPT_DIR/$PLIST_NAME"
    elif [ -f "$DATA_DIR/$PLIST_NAME" ]; then
        plist_template="$DATA_DIR/$PLIST_NAME"
    else
        log_error "Plist template not found"
        exit 1
    fi

    # Substitute paths and install
    sed -e "s|__INSTALL_DIR__|$INSTALL_DIR|g" \
        -e "s|__DATA_DIR__|$DATA_DIR|g" \
        "$plist_template" > "$PLIST_DIR/$PLIST_NAME"

    log_success "Installed plist to $PLIST_DIR/$PLIST_NAME"
}

start_service() {
    log_info "Starting service..."
    launchctl load "$PLIST_DIR/$PLIST_NAME"

    # Wait a moment and check if it started
    sleep 1
    if launchctl list | grep -q "com.saorsa.canvas"; then
        log_success "Service started successfully"
    else
        log_warn "Service may have failed to start. Check logs at:"
        log_warn "  $DATA_DIR/logs/stdout.log"
        log_warn "  $DATA_DIR/logs/stderr.log"
    fi
}

show_status() {
    printf "\n"
    if launchctl list | grep -q "com.saorsa.canvas"; then
        printf "${GREEN}Service Status: Running${NC}\n"
    else
        printf "${YELLOW}Service Status: Not Running${NC}\n"
    fi
    printf "\n"
    printf "Manage the service:\n"
    printf "  Start:   launchctl load %s/%s\n" "$PLIST_DIR" "$PLIST_NAME"
    printf "  Stop:    launchctl unload %s/%s\n" "$PLIST_DIR" "$PLIST_NAME"
    printf "  Logs:    tail -f %s/logs/*.log\n" "$DATA_DIR"
    printf "\n"
    printf "Access the canvas at: ${BLUE}http://localhost:9473${NC}\n"
    printf "\n"
}

show_help() {
    printf 'Saorsa Canvas macOS Daemon Setup

Usage: %s [COMMAND]

Commands:
    install     Install and start the daemon (default)
    uninstall   Stop and remove the daemon
    start       Start the daemon
    stop        Stop the daemon
    status      Show daemon status
    help        Show this help message
' "$0"
}

cmd_install() {
    check_macos
    check_binary
    stop_service
    install_plist
    start_service
    show_status
}

cmd_uninstall() {
    check_macos
    log_info "Uninstalling daemon..."
    stop_service

    if [ -f "$PLIST_DIR/$PLIST_NAME" ]; then
        rm "$PLIST_DIR/$PLIST_NAME"
        log_success "Removed plist"
    fi

    log_success "Daemon uninstalled"
    printf "\n"
    printf "Note: Binary and data remain at:\n"
    printf "  Binary: %s/saorsa-canvas\n" "$INSTALL_DIR"
    printf "  Data:   %s\n" "$DATA_DIR"
    printf "\n"
}

cmd_start() {
    check_macos
    if [ ! -f "$PLIST_DIR/$PLIST_NAME" ]; then
        log_error "Daemon not installed. Run: $0 install"
        exit 1
    fi
    start_service
    show_status
}

cmd_stop() {
    check_macos
    stop_service
    log_success "Service stopped"
}

cmd_status() {
    check_macos
    show_status
}

main() {
    command="${1:-install}"

    case "$command" in
        install)
            cmd_install
            ;;
        uninstall)
            cmd_uninstall
            ;;
        start)
            cmd_start
            ;;
        stop)
            cmd_stop
            ;;
        status)
            cmd_status
            ;;
        help|--help|-h)
            show_help
            ;;
        *)
            log_error "Unknown command: $command"
            show_help
            exit 1
            ;;
    esac
}

main "$@"
