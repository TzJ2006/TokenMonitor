#!/usr/bin/env bash
# Download Node.js portable binary for the specified (or current) platform.
# Result: build/cache/node-{platform}-{arch}/  (self-contained, no system install)
#
# Usage:
#   ./download-node.sh                   # auto-detect platform & arch
#   ./download-node.sh windows x64       # explicit
#   ./download-node.sh linux arm64
set -euo pipefail
source "$(dirname "$0")/config.sh"

TARGET_PLATFORM="${1:-$PLATFORM}"
TARGET_ARCH="${2:-$ARCH}"
NODE_DIR="$CACHE_DIR/node-$TARGET_PLATFORM-$TARGET_ARCH"

# ── Skip if already cached ────────────────────────────────────────────────────
if [[ -d "$NODE_DIR" && -f "$NODE_DIR/.node-version" ]]; then
  CACHED=$(cat "$NODE_DIR/.node-version")
  if [[ "$CACHED" == "$NODE_VERSION" ]]; then
    info "Node.js v$NODE_VERSION already cached → $NODE_DIR"
    exit 0
  fi
  warn "Cached version ($CACHED) differs from target ($NODE_VERSION). Re-downloading."
fi

step "Downloading Node.js v$NODE_VERSION ($TARGET_PLATFORM-$TARGET_ARCH)"
mkdir -p "$CACHE_DIR"

# ── Build download URL ────────────────────────────────────────────────────────
BASE_URL="https://nodejs.org/dist/v${NODE_VERSION}"

case "$TARGET_PLATFORM" in
  windows)
    ARCHIVE="node-v${NODE_VERSION}-win-${TARGET_ARCH}.zip"
    ;;
  macos)
    ARCHIVE="node-v${NODE_VERSION}-darwin-${TARGET_ARCH}.tar.gz"
    ;;
  linux)
    ARCHIVE="node-v${NODE_VERSION}-linux-${TARGET_ARCH}.tar.xz"
    ;;
  *)
    fail "Unsupported platform: $TARGET_PLATFORM"
    ;;
esac

URL="$BASE_URL/$ARCHIVE"
DOWNLOAD_PATH="$CACHE_DIR/$ARCHIVE"

echo "  URL: $URL"
curl -fSL --progress-bar -o "$DOWNLOAD_PATH" "$URL"

# ── Extract ───────────────────────────────────────────────────────────────────
rm -rf "$NODE_DIR"
mkdir -p "$NODE_DIR"

case "$TARGET_PLATFORM" in
  windows)
    TMP_DIR="$CACHE_DIR/_node-extract"
    rm -rf "$TMP_DIR"
    unzip -q "$DOWNLOAD_PATH" -d "$TMP_DIR"
    # The zip contains a single top-level directory; move its contents up.
    mv "$TMP_DIR"/node-*/* "$NODE_DIR/"
    rm -rf "$TMP_DIR"
    ;;
  macos)
    tar -xzf "$DOWNLOAD_PATH" -C "$NODE_DIR" --strip-components=1
    ;;
  linux)
    tar -xJf "$DOWNLOAD_PATH" -C "$NODE_DIR" --strip-components=1
    ;;
esac

rm -f "$DOWNLOAD_PATH"

# ── Stamp ─────────────────────────────────────────────────────────────────────
echo "$NODE_VERSION" > "$NODE_DIR/.node-version"

info "Node.js v$NODE_VERSION → $NODE_DIR"
du -sh "$NODE_DIR" | awk '{print "  Size: "$1}'
