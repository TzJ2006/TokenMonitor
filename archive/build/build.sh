#!/usr/bin/env bash
# TokenMonitor build entry point.
#
# Usage:
#   bash build/build.sh                  # build both variants for current platform
#   bash build/build.sh --lightweight    # app installer only
#   bash build/build.sh --full           # full offline package only
#   bash build/build.sh --all            # both (default)
#
# Environment variables:
#   SKIP_APP_BUILD=1   skip tauri build (reuse existing lightweight output)
set -euo pipefail
source "$(dirname "$0")/config.sh"

VARIANT="${1:---all}"

echo "============================================="
echo "  $APP_NAME v$APP_VERSION Build"
echo "  Platform: $PLATFORM ($ARCH)"
echo "  Variant:  $VARIANT"
echo "============================================="

case "$VARIANT" in
  --lightweight|-l)
    bash "$BUILD_DIR/build-app.sh"
    ;;
  --full|-f)
    if [[ "${SKIP_APP_BUILD:-}" == "1" ]]; then
      info "SKIP_APP_BUILD=1 — reusing existing lightweight output"
    else
      bash "$BUILD_DIR/build-app.sh"
    fi
    bash "$BUILD_DIR/build-full.sh"
    ;;
  --all|-a|*)
    bash "$BUILD_DIR/build-app.sh"
    bash "$BUILD_DIR/build-full.sh"
    ;;
esac

echo ""
step "Build complete"
echo "Outputs:"
find "$OUTPUT_DIR" -type f -name "*.dmg" -o -name "*.exe" -o -name "*.deb" \
     -o -name "*.zip" -o -name "*.tar.gz" 2>/dev/null | while read -r f; do
  SIZE=$(du -h "$f" | awk '{print $1}')
  echo "  $SIZE  $f"
done
