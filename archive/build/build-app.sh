#!/usr/bin/env bash
# Build the Tauri app installer (lightweight variant).
# Produces platform-native installer in outputs/lightweight/{platform}/
set -euo pipefail
source "$(dirname "$0")/config.sh"

step "Building $APP_NAME v$APP_VERSION — lightweight ($PLATFORM-$ARCH)"

cd "$ROOT_DIR"

# Ensure frontend deps are installed
if [[ ! -d node_modules ]]; then
  step "Installing npm dependencies"
  npm ci
fi

# Run Tauri build
step "Running tauri build"
npx tauri build

# Copy artifacts to outputs/lightweight/<platform>/
DEST="$OUTPUT_DIR/lightweight/$PLATFORM"
mkdir -p "$DEST"

BUNDLE_DIR="$ROOT_DIR/src-tauri/target/release/bundle"

case "$PLATFORM" in
  macos)
    cp -v "$BUNDLE_DIR"/dmg/*.dmg "$DEST/" 2>/dev/null || warn "No .dmg found"
    ;;
  windows)
    cp -v "$BUNDLE_DIR"/nsis/*.exe "$DEST/" 2>/dev/null || warn "No .exe found"
    ;;
  linux)
    cp -v "$BUNDLE_DIR"/deb/*.deb "$DEST/" 2>/dev/null || warn "No .deb found"
    ;;
  *)
    fail "Unsupported platform: $PLATFORM"
    ;;
esac

info "Lightweight build → $DEST/"
ls -lh "$DEST/"
