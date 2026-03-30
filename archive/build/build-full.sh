#!/usr/bin/env bash
# Assemble the "full" package: app installer + Node.js portable (with ccusage)
# + end-user prereq installer + README, all zipped together.
#
# Prerequisites: build-app.sh must have run first (lightweight output exists).
#
# Output: outputs/full/{platform}/TokenMonitor-Full-{version}-{platform}.zip
set -euo pipefail
source "$(dirname "$0")/config.sh"

step "Building $APP_NAME v$APP_VERSION — full ($PLATFORM-$ARCH)"

# ── 1. Ensure lightweight build exists ────────────────────────────────────────
LIGHT_DIR="$OUTPUT_DIR/lightweight/$PLATFORM"
if [[ ! -d "$LIGHT_DIR" ]] || [[ -z "$(ls -A "$LIGHT_DIR" 2>/dev/null)" ]]; then
  warn "Lightweight build not found. Building app first..."
  bash "$BUILD_DIR/build-app.sh"
fi

# ── 2. Download Node.js portable ─────────────────────────────────────────────
bash "$BUILD_DIR/download-node.sh" "$PLATFORM" "$ARCH"

# ── 3. Install ccusage into portable Node.js ──────────────────────────────────
bash "$BUILD_DIR/bundle-ccusage.sh" "$CACHE_DIR/node-$PLATFORM-$ARCH"

# ── 4. Assemble staging directory ─────────────────────────────────────────────
STAGE="$CACHE_DIR/full-stage-$PLATFORM"
rm -rf "$STAGE"
mkdir -p "$STAGE/prereqs/node-portable"

# Copy app installer
case "$PLATFORM" in
  macos)
    cp "$LIGHT_DIR"/*.dmg "$STAGE/" 2>/dev/null || fail "No .dmg found in $LIGHT_DIR"
    ;;
  windows)
    cp "$LIGHT_DIR"/*.exe "$STAGE/" 2>/dev/null || fail "No .exe found in $LIGHT_DIR"
    ;;
  linux)
    cp "$LIGHT_DIR"/*.deb "$STAGE/" 2>/dev/null || fail "No .deb found in $LIGHT_DIR"
    ;;
esac

# Copy portable Node.js + ccusage
cp -r "$CACHE_DIR/node-$PLATFORM-$ARCH"/* "$STAGE/prereqs/node-portable/"
# Remove the version stamp (not needed by end user)
rm -f "$STAGE/prereqs/node-portable/.node-version"

# Copy end-user prereq installer
case "$PLATFORM" in
  windows)
    cp "$BUILD_DIR/prereqs/install-prereqs.bat" "$STAGE/"
    ;;
  macos|linux)
    cp "$BUILD_DIR/prereqs/install-prereqs.sh" "$STAGE/"
    chmod +x "$STAGE/install-prereqs.sh"
    ;;
esac

# Generate README
cat > "$STAGE/README.txt" << 'READMEEOF'
TokenMonitor — Full Installation Package
=========================================

This package includes everything needed to run TokenMonitor, even on
machines without internet access.

Contents:
  - TokenMonitor installer          (the app itself)
  - prereqs/node-portable/          (Node.js + @ccusage/mcp, pre-installed)

READMEEOF

case "$PLATFORM" in
  windows)
    cat >> "$STAGE/README.txt" << 'READMEEOF'
Installation (Windows):
  1. Double-click  install-prereqs.bat   to set up Node.js + ccusage
     (skips automatically if already installed)
  2. Double-click  TokenMonitor_*_x64-setup.exe  to install the app
READMEEOF
    ;;
  macos)
    cat >> "$STAGE/README.txt" << 'READMEEOF'
Installation (macOS):
  1. Open Terminal in this folder and run:  bash install-prereqs.sh
     (skips automatically if already installed)
  2. Double-click the .dmg file and drag TokenMonitor to Applications
READMEEOF
    ;;
  linux)
    cat >> "$STAGE/README.txt" << 'READMEEOF'
Installation (Linux):
  1. Open a terminal in this folder and run:  bash install-prereqs.sh
     (skips automatically if already installed)
  2. Install the .deb package:  sudo dpkg -i token-monitor_*.deb
READMEEOF
    ;;
esac

# ── 5. Zip it ─────────────────────────────────────────────────────────────────
FULL_DIR="$OUTPUT_DIR/full/$PLATFORM"
mkdir -p "$FULL_DIR"

ZIP_NAME="${APP_NAME}-Full-${APP_VERSION}-${PLATFORM}"
ZIP_PATH="$FULL_DIR/${ZIP_NAME}.zip"

# Rename staging dir for a clean zip root
FINAL_STAGE="$CACHE_DIR/$ZIP_NAME"
rm -rf "$FINAL_STAGE"
mv "$STAGE" "$FINAL_STAGE"

# Create zip (use platform-available tool)
rm -f "$ZIP_PATH"
cd "$CACHE_DIR"
if command -v zip &>/dev/null; then
  zip -r "$ZIP_PATH" "$ZIP_NAME/"
elif command -v 7z &>/dev/null; then
  7z a "$ZIP_PATH" "$ZIP_NAME/"
elif command -v tar &>/dev/null; then
  # Fallback: tar.gz instead of zip
  ZIP_PATH="${ZIP_PATH%.zip}.tar.gz"
  tar -czf "$ZIP_PATH" "$ZIP_NAME/"
else
  fail "No archiver found (zip, 7z, or tar). Install one and retry."
fi
cd "$ROOT_DIR"

# Cleanup staging
rm -rf "$FINAL_STAGE"

info "Full build → $ZIP_PATH"
ls -lh "$ZIP_PATH"
