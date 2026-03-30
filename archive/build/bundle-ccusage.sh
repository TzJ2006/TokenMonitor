#!/usr/bin/env bash
# Install @ccusage/mcp into a portable Node.js directory so the full package
# ships with ccusage pre-installed (works offline).
#
# Prerequisites: download-node.sh must have run first.
#
# Usage:
#   ./bundle-ccusage.sh                          # auto-detect from cache
#   ./bundle-ccusage.sh build/cache/node-windows-x64   # explicit path
set -euo pipefail
source "$(dirname "$0")/config.sh"

NODE_PORTABLE="${1:-$CACHE_DIR/node-$PLATFORM-$ARCH}"

if [[ ! -d "$NODE_PORTABLE" ]]; then
  fail "Node.js portable not found at $NODE_PORTABLE — run download-node.sh first."
fi

step "Installing $CCUSAGE_PACKAGE into portable Node.js"

# ── Locate the npm binary ────────────────────────────────────────────────────
case "$PLATFORM" in
  windows) NPM_BIN="$NODE_PORTABLE/npm.cmd" ;;
  *)       NPM_BIN="$NODE_PORTABLE/bin/npm" ;;
esac

# Fallback: call npm via the portable node binary directly
if [[ ! -f "$NPM_BIN" ]]; then
  case "$PLATFORM" in
    windows)
      NODE_BIN="$NODE_PORTABLE/node.exe"
      NPM_CLI="$NODE_PORTABLE/node_modules/npm/bin/npm-cli.js"
      ;;
    *)
      NODE_BIN="$NODE_PORTABLE/bin/node"
      NPM_CLI="$NODE_PORTABLE/lib/node_modules/npm/bin/npm-cli.js"
      ;;
  esac
  if [[ -f "$NODE_BIN" && -f "$NPM_CLI" ]]; then
    NPM_BIN="$NODE_BIN $NPM_CLI"
  else
    # Last resort: use system npm with --prefix
    warn "Portable npm not found. Using system npm with --prefix."
    NPM_BIN="npm"
  fi
fi

# ── Install ccusage globally into the portable prefix ─────────────────────────
# --prefix forces npm to install into the portable Node.js tree, so that
# when the end user adds this directory to PATH, ccusage is already available.
$NPM_BIN install -g "$CCUSAGE_PACKAGE" --prefix "$NODE_PORTABLE" 2>&1 \
  || warn "npm install had warnings (may still be usable)"

# ── Verify ────────────────────────────────────────────────────────────────────
case "$PLATFORM" in
  windows) MODULES_DIR="$NODE_PORTABLE/node_modules" ;;
  *)       MODULES_DIR="$NODE_PORTABLE/lib/node_modules" ;;
esac

if [[ -d "$MODULES_DIR/@ccusage" || -d "$MODULES_DIR/ccusage" ]]; then
  info "$CCUSAGE_PACKAGE installed into $NODE_PORTABLE"
else
  # Check if the package name resolved differently
  FOUND=$(ls "$MODULES_DIR/" 2>/dev/null | grep -i ccusage || true)
  if [[ -n "$FOUND" ]]; then
    info "$CCUSAGE_PACKAGE installed as: $FOUND"
  else
    warn "$CCUSAGE_PACKAGE may not have installed correctly. Check $MODULES_DIR/"
  fi
fi

du -sh "$NODE_PORTABLE" | awk '{print "  Total portable size: "$1}'
