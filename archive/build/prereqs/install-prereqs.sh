#!/usr/bin/env bash
# TokenMonitor Prerequisites Installer (macOS / Linux)
# Installs Node.js portable + @ccusage/mcp if not already present.
# Safe to re-run: detects existing installations and skips.
set -euo pipefail

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
info()  { echo -e "${GREEN}[OK]${NC}  $1"; }
warn()  { echo -e "${YELLOW}[!!]${NC}  $1"; }
fail()  { echo -e "${RED}[ERR]${NC} $1"; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
INSTALL_DIR="$HOME/.tokenmonitor"
NODE_DEST="$INSTALL_DIR/node"
BUNDLED_NODE="$SCRIPT_DIR/prereqs/node-portable"

echo "============================================="
echo "  TokenMonitor Prerequisites Setup"
echo "============================================="
echo ""

# ── Step 1: Check Node.js ────────────────────────────────────────────────────
echo "Checking for Node.js..."
if command -v node &>/dev/null; then
  info "Node.js found: $(node --version)"
else
  warn "Node.js not found. Installing from bundled package..."

  if [[ ! -d "$BUNDLED_NODE" || ! -f "$BUNDLED_NODE/bin/node" ]]; then
    fail "Bundled Node.js not found at $BUNDLED_NODE.\n       Install Node.js manually from https://nodejs.org/"
  fi

  # Copy portable Node.js
  mkdir -p "$NODE_DEST"
  cp -r "$BUNDLED_NODE"/* "$NODE_DEST/"
  chmod +x "$NODE_DEST/bin/"*

  # Add to PATH in current shell
  export PATH="$NODE_DEST/bin:$PATH"

  # Persist PATH addition
  SHELL_RC=""
  case "$(basename "${SHELL:-bash}")" in
    zsh)  SHELL_RC="$HOME/.zshrc" ;;
    bash) SHELL_RC="$HOME/.bashrc" ;;
    fish) SHELL_RC="$HOME/.config/fish/config.fish" ;;
  esac

  if [[ -n "$SHELL_RC" ]]; then
    PATH_LINE="export PATH=\"$NODE_DEST/bin:\$PATH\""
    if ! grep -qF "$NODE_DEST/bin" "$SHELL_RC" 2>/dev/null; then
      echo "" >> "$SHELL_RC"
      echo "# TokenMonitor — Node.js portable" >> "$SHELL_RC"
      echo "$PATH_LINE" >> "$SHELL_RC"
      info "Added Node.js to PATH in $SHELL_RC"
    fi
  fi

  info "Node.js installed to $NODE_DEST"
fi

# ── Step 2: Check ccusage ────────────────────────────────────────────────────
echo ""
echo "Checking for ccusage..."

if command -v ccusage &>/dev/null || command -v ccusage-mcp &>/dev/null; then
  info "ccusage found"
elif [[ -d "$NODE_DEST/lib/node_modules/@ccusage" ]]; then
  info "ccusage found in portable Node.js"
else
  warn "ccusage not found. Installing via npm..."
  if command -v npm &>/dev/null; then
    npm install -g @ccusage/mcp 2>/dev/null \
      && info "ccusage installed" \
      || warn "npm install failed. Install later: npm install -g @ccusage/mcp"
  else
    warn "npm not available. Install ccusage later: npm install -g @ccusage/mcp"
  fi
fi

echo ""
echo "============================================="
echo "  Prerequisites ready!"
echo "  You can now install TokenMonitor."
echo "============================================="
