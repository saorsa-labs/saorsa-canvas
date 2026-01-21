#!/bin/sh
set -eu

# Saorsa Canvas Linux Daemon Setup
# Installs systemd user service for auto-start

# Configuration (must match install.sh)
INSTALL_DIR="$HOME/.local/bin"
DATA_DIR="$HOME/.local/share/saorsa-canvas"
SERVICE_NAME="saorsa-canvas.service"
SERVICE_DIR="$HOME/.config/systemd/user"
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

check_linux() {
    if [ "$(uname -s)" != "Linux" ]; then
        log_error "This script is for Linux only"
        exit 1
    fi
}

check_systemd() {
    if ! command -v systemctl >/dev/null 2>&1; then
        log_error "systemd not found. This script requires systemd."
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
    if systemctl --user is-active --quiet saorsa-canvas 2>/dev/null; then
        log_info "Stopping existing service..."
        systemctl --user stop saorsa-canvas || true
    fi
}

install_service() {
    log_info "Installing systemd user service..."

    # Create directories
    mkdir -p "$SERVICE_DIR"
    mkdir -p "$DATA_DIR/logs"

    # Get the service template
    if [ -f "$SCRIPT_DIR/$SERVICE_NAME" ]; then
        service_template="$SCRIPT_DIR/$SERVICE_NAME"
    elif [ -f "$DATA_DIR/$SERVICE_NAME" ]; then
        service_template="$DATA_DIR/$SERVICE_NAME"
    else
        log_error "Service template not found"
        exit 1
    fi

    # Substitute paths and install
    sed -e "s|__INSTALL_DIR__|$INSTALL_DIR|g" \
        -e "s|__DATA_DIR__|$DATA_DIR|g" \
        "$service_template" > "$SERVICE_DIR/$SERVICE_NAME"

    log_success "Installed service to $SERVICE_DIR/$SERVICE_NAME"

    # Reload systemd
    log_info "Reloading systemd..."
    systemctl --user daemon-reload
}

enable_service() {
    log_info "Enabling service for auto-start..."
    systemctl --user enable saorsa-canvas
    log_success "Service enabled"
}

start_service() {
    log_info "Starting service..."
    systemctl --user start saorsa-canvas

    # Wait a moment and check if it started
    sleep 1
    if systemctl --user is-active --quiet saorsa-canvas; then
        log_success "Service started successfully"
    else
        log_warn "Service may have failed to start. Check status with:"
        log_warn "  systemctl --user status saorsa-canvas"
        log_warn "  journalctl --user -u saorsa-canvas"
    fi
}

enable_linger() {
    # Enable linger so user services run without login
    if command -v loginctl >/dev/null 2>&1; then
        log_info "Enabling user linger for background services..."
        loginctl enable-linger "$(whoami)" 2>/dev/null || log_warn "Could not enable linger (may require sudo)"
    fi
}

show_status() {
    printf "\n"
    if systemctl --user is-active --quiet saorsa-canvas 2>/dev/null; then
        printf "${GREEN}Service Status: Running${NC}\n"
    else
        printf "${YELLOW}Service Status: Not Running${NC}\n"
    fi
    printf "\n"
    printf "Manage the service:\n"
    printf "  Start:   systemctl --user start saorsa-canvas\n"
    printf "  Stop:    systemctl --user stop saorsa-canvas\n"
    printf "  Status:  systemctl --user status saorsa-canvas\n"
    printf "  Logs:    journalctl --user -u saorsa-canvas -f\n"
    printf "           tail -f %s/logs/*.log\n" "$DATA_DIR"
    printf "\n"
    printf "Access the canvas at: ${BLUE}http://localhost:9473${NC}\n"
    printf "\n"
}

show_help() {
    printf 'Saorsa Canvas Linux Daemon Setup

Usage: %s [COMMAND]

Commands:
    install     Install, enable, and start the daemon (default)
    uninstall   Stop, disable, and remove the daemon
    start       Start the daemon
    stop        Stop the daemon
    status      Show daemon status
    help        Show this help message
' "$0"
}

cmd_install() {
    check_linux
    check_systemd
    check_binary
    stop_service
    install_service
    enable_service
    enable_linger
    start_service
    show_status
}

cmd_uninstall() {
    check_linux
    check_systemd
    log_info "Uninstalling daemon..."

    # Stop and disable
    stop_service
    if systemctl --user is-enabled --quiet saorsa-canvas 2>/dev/null; then
        systemctl --user disable saorsa-canvas || true
    fi

    # Remove service file
    if [ -f "$SERVICE_DIR/$SERVICE_NAME" ]; then
        rm "$SERVICE_DIR/$SERVICE_NAME"
        log_success "Removed service file"
    fi

    # Reload systemd
    systemctl --user daemon-reload

    log_success "Daemon uninstalled"
    printf "\n"
    printf "Note: Binary and data remain at:\n"
    printf "  Binary: %s/saorsa-canvas\n" "$INSTALL_DIR"
    printf "  Data:   %s\n" "$DATA_DIR"
    printf "\n"
}

cmd_start() {
    check_linux
    check_systemd
    if [ ! -f "$SERVICE_DIR/$SERVICE_NAME" ]; then
        log_error "Daemon not installed. Run: $0 install"
        exit 1
    fi
    start_service
    show_status
}

cmd_stop() {
    check_linux
    check_systemd
    stop_service
    log_success "Service stopped"
}

cmd_status() {
    check_linux
    check_systemd
    show_status
    systemctl --user status saorsa-canvas 2>/dev/null || true
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
