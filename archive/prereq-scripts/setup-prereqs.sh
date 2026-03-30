#!/usr/bin/env bash
# TokenMonitor Prerequisites Installer (macOS / Linux)
# Installs Node.js (if missing) and @ccusage/mcp

set -euo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

info()  { echo -e "${GREEN}[OK]${NC} $1"; }
warn()  { echo -e "${YELLOW}[!!]${NC} $1"; }
fail()  { echo -e "${RED}[ERR]${NC} $1"; exit 1; }

echo "=== TokenMonitor Prerequisites Setup ==="
echo ""

# ── Step 1: Check Node.js ──
if command -v node &>/dev/null; then
  info "Node.js found: $(node --version)"
else
  warn "Node.js not found. Attempting install..."

  if [[ "$(uname)" == "Darwin" ]]; then
    # macOS: try brew, then direct installer
    if command -v brew &>/dev/null; then
      echo "Installing Node.js via Homebrew..."
      brew install node
    else
      fail "Homebrew not found. Install Node.js from https://nodejs.org/ or install Homebrew first:\n  /bin/bash -c \"\$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
    fi
  else
    # Linux: try apt, dnf, then nvm
    if command -v apt-get &>/dev/null; then
      echo "Installing Node.js via apt..."
      NODESOURCE_SCRIPT=$(mktemp)
      curl -fsSL https://deb.nodesource.com/setup_lts.x -o "$NODESOURCE_SCRIPT"
      sudo -E bash "$NODESOURCE_SCRIPT"
      rm -f "$NODESOURCE_SCRIPT"
      sudo apt-get install -y nodejs
    elif command -v dnf &>/dev/null; then
      echo "Installing Node.js via dnf..."
      sudo dnf install -y nodejs npm
    elif command -v pacman &>/dev/null; then
      echo "Installing Node.js via pacman..."
      sudo pacman -S --noconfirm nodejs npm
    else
      fail "No supported package manager found. Install Node.js from https://nodejs.org/"
    fi
  fi

  # Verify
  if command -v node &>/dev/null; then
    info "Node.js installed: $(node --version)"
  else
    fail "Node.js installation failed. Install manually from https://nodejs.org/"
  fi
fi

# ── Step 2: Check npm ──
if command -v npm &>/dev/null; then
  info "npm found: $(npm --version)"
else
  fail "npm not found. Reinstall Node.js from https://nodejs.org/"
fi

# ── Step 3: Install @ccusage/mcp ──
if npm list -g @ccusage/mcp &>/dev/null; then
  info "@ccusage/mcp already installed globally"
else
  echo "Installing @ccusage/mcp globally..."
  npm install -g @ccusage/mcp
  if npm list -g @ccusage/mcp &>/dev/null; then
    info "@ccusage/mcp installed successfully"
  else
    warn "@ccusage/mcp global install may have failed, but npx will auto-download on first use"
  fi
fi

echo ""
info "All prerequisites are ready! You can now launch TokenMonitor."
