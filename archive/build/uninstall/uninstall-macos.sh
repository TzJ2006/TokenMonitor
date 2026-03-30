#!/usr/bin/env bash
# TokenMonitor Complete Uninstaller — macOS
# Removes: application, app data, prerequisites, autostart, PATH entries.
set -euo pipefail

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
info()  { echo -e "${GREEN}[OK]${NC}  $1"; }
warn()  { echo -e "${YELLOW}[!!]${NC}  $1"; }
fail()  { echo -e "${RED}[ERR]${NC} $1"; exit 1; }

APP_NAME="TokenMonitor"
APP_ID="com.tokenmonitor.app"
INSTALL_DIR="$HOME/.tokenmonitor"
LAUNCH_AGENT="$HOME/Library/LaunchAgents/${APP_ID}.plist"

echo "============================================="
echo "  $APP_NAME Uninstaller — macOS"
echo "============================================="
echo ""

# ── Step 1: Stop running processes ────────────────────────────────────────────
echo "Stopping $APP_NAME..."
pkill -f "$APP_NAME" 2>/dev/null && info "Process stopped" || info "No running process found"

# Unload LaunchAgent (prevents auto-restart)
if [[ -f "$LAUNCH_AGENT" ]]; then
  launchctl unload "$LAUNCH_AGENT" 2>/dev/null || true
  rm -f "$LAUNCH_AGENT"
  info "Removed LaunchAgent"
fi

sleep 1

# ── Step 2: Remove application ───────────────────────────────────────────────
if [[ -d "/Applications/$APP_NAME.app" ]]; then
  rm -rf "/Applications/$APP_NAME.app"
  info "Removed /Applications/$APP_NAME.app"
else
  warn "Application not found in /Applications/"
fi

# ── Step 3: Remove app data ──────────────────────────────────────────────────
APP_SUPPORT="$HOME/Library/Application Support/$APP_ID"
if [[ -d "$APP_SUPPORT" ]]; then
  rm -rf "$APP_SUPPORT"
  info "Removed app data: $APP_SUPPORT"
fi

# Also check for Tauri store data
TAURI_STORE="$HOME/Library/Application Support/$APP_ID"
if [[ -d "$TAURI_STORE" ]]; then
  rm -rf "$TAURI_STORE"
fi

# ── Step 4: Remove prerequisites (portable Node.js + ccusage) ────────────────
if [[ -d "$INSTALL_DIR" ]]; then
  rm -rf "$INSTALL_DIR"
  info "Removed prerequisites: $INSTALL_DIR"
else
  info "No prerequisites directory found"
fi

# ── Step 5: Clean shell RC PATH entries ──────────────────────────────────────
clean_shell_rc() {
  local rc_file="$1"
  if [[ ! -f "$rc_file" ]]; then
    return
  fi

  if grep -q "# BEGIN TokenMonitor PATH" "$rc_file" 2>/dev/null; then
    # Remove marker-delimited block
    sed -i.bak '/# BEGIN TokenMonitor PATH/,/# END TokenMonitor PATH/d' "$rc_file"
    rm -f "${rc_file}.bak"
    info "Cleaned PATH entries from $rc_file"
  elif grep -q "tokenmonitor" "$rc_file" 2>/dev/null || grep -q ".tokenmonitor" "$rc_file" 2>/dev/null; then
    # Legacy: remove lines containing tokenmonitor PATH entries
    sed -i.bak '/\.tokenmonitor\/node\/bin/d' "$rc_file"
    sed -i.bak '/# TokenMonitor/d' "$rc_file"
    rm -f "${rc_file}.bak"
    info "Cleaned legacy PATH entries from $rc_file"
  fi
}

clean_shell_rc "$HOME/.zshrc"
clean_shell_rc "$HOME/.bashrc"
clean_shell_rc "$HOME/.bash_profile"
clean_shell_rc "$HOME/.profile"

# Fish config
FISH_CONFIG="$HOME/.config/fish/config.fish"
if [[ -f "$FISH_CONFIG" ]] && grep -q "tokenmonitor" "$FISH_CONFIG" 2>/dev/null; then
  sed -i.bak '/tokenmonitor/d' "$FISH_CONFIG"
  rm -f "${FISH_CONFIG}.bak"
  info "Cleaned PATH entries from fish config"
fi

echo ""
echo "============================================="
echo "  $APP_NAME has been completely removed."
echo "============================================="
