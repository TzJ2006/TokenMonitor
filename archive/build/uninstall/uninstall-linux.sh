#!/usr/bin/env bash
# TokenMonitor Complete Uninstaller — Linux
# Removes: deb package, app data, prerequisites, autostart, PATH entries.
set -euo pipefail

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
info()  { echo -e "${GREEN}[OK]${NC}  $1"; }
warn()  { echo -e "${YELLOW}[!!]${NC}  $1"; }
fail()  { echo -e "${RED}[ERR]${NC} $1"; exit 1; }

APP_NAME="TokenMonitor"
APP_ID="com.tokenmonitor.app"
DEB_PACKAGE="token-monitor"
INSTALL_DIR="$HOME/.tokenmonitor"

echo "============================================="
echo "  $APP_NAME Uninstaller — Linux"
echo "============================================="
echo ""

# ── Step 1: Stop running processes ────────────────────────────────────────────
echo "Stopping $APP_NAME..."
pkill -f "token-monitor" 2>/dev/null && info "Process stopped" || info "No running process found"
sleep 1

# ── Step 2: Remove deb package ───────────────────────────────────────────────
if dpkg -l "$DEB_PACKAGE" &>/dev/null; then
  echo "Removing $DEB_PACKAGE package (may require sudo)..."
  if command -v pkexec &>/dev/null; then
    pkexec apt purge -y "$DEB_PACKAGE" && info "Package removed" || warn "Package removal failed — try: sudo apt purge $DEB_PACKAGE"
  elif [[ $EUID -eq 0 ]]; then
    apt purge -y "$DEB_PACKAGE"
    info "Package removed"
  else
    sudo apt purge -y "$DEB_PACKAGE" && info "Package removed" || warn "Package removal failed — try: sudo apt purge $DEB_PACKAGE"
  fi
else
  info "Package $DEB_PACKAGE not installed via apt"
fi

# ── Step 3: Remove app data ──────────────────────────────────────────────────
CONFIG_DIR="$HOME/.config/$APP_ID"
DATA_DIR="$HOME/.local/share/$APP_ID"

for dir in "$CONFIG_DIR" "$DATA_DIR"; do
  if [[ -d "$dir" ]]; then
    rm -rf "$dir"
    info "Removed: $dir"
  fi
done

# ── Step 4: Remove prerequisites (portable Node.js + ccusage) ────────────────
if [[ -d "$INSTALL_DIR" ]]; then
  rm -rf "$INSTALL_DIR"
  info "Removed prerequisites: $INSTALL_DIR"
else
  info "No prerequisites directory found"
fi

# ── Step 5: Remove XDG autostart entry ───────────────────────────────────────
AUTOSTART="$HOME/.config/autostart/${APP_ID}.desktop"
if [[ -f "$AUTOSTART" ]]; then
  rm -f "$AUTOSTART"
  info "Removed autostart entry"
fi

# Also check for alternative autostart names
for f in "$HOME/.config/autostart/"*okenmonitor* "$HOME/.config/autostart/"*oken-monitor*; do
  if [[ -f "$f" ]]; then
    rm -f "$f"
    info "Removed autostart: $(basename "$f")"
  fi
done

# ── Step 6: Clean shell RC PATH entries ──────────────────────────────────────
clean_shell_rc() {
  local rc_file="$1"
  if [[ ! -f "$rc_file" ]]; then
    return
  fi

  if grep -q "# BEGIN TokenMonitor PATH" "$rc_file" 2>/dev/null; then
    sed -i '/# BEGIN TokenMonitor PATH/,/# END TokenMonitor PATH/d' "$rc_file"
    info "Cleaned PATH entries from $rc_file"
  elif grep -q "tokenmonitor" "$rc_file" 2>/dev/null || grep -q ".tokenmonitor" "$rc_file" 2>/dev/null; then
    sed -i '/\.tokenmonitor\/node\/bin/d' "$rc_file"
    sed -i '/# TokenMonitor/d' "$rc_file"
    info "Cleaned legacy PATH entries from $rc_file"
  fi
}

clean_shell_rc "$HOME/.bashrc"
clean_shell_rc "$HOME/.bash_profile"
clean_shell_rc "$HOME/.profile"
clean_shell_rc "$HOME/.zshrc"

# Fish config
FISH_CONFIG="$HOME/.config/fish/config.fish"
if [[ -f "$FISH_CONFIG" ]] && grep -q "tokenmonitor" "$FISH_CONFIG" 2>/dev/null; then
  sed -i '/tokenmonitor/d' "$FISH_CONFIG"
  info "Cleaned PATH entries from fish config"
fi

echo ""
echo "============================================="
echo "  $APP_NAME has been completely removed."
echo "============================================="
