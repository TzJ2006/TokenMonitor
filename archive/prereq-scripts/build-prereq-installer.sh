#!/usr/bin/env bash
# Build the TokenMonitor prerequisites installer for Windows (.exe)
# Requires NSIS (auto-downloaded by Tauri build, or install manually)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NSI_FILE="$SCRIPT_DIR/setup-prereqs.nsi"

# Find NSIS: check Tauri's download location first, then PATH
if [[ -f "$LOCALAPPDATA/tauri/NSIS/makensis.exe" ]]; then
  MAKENSIS="$LOCALAPPDATA/tauri/NSIS/makensis.exe"
elif command -v makensis &>/dev/null; then
  MAKENSIS="makensis"
else
  echo "Error: makensis not found. Run 'npx tauri build' once to download NSIS, or install NSIS manually."
  exit 1
fi

echo "Building TokenMonitor-PrereqSetup.exe..."
cd "$SCRIPT_DIR"
"$MAKENSIS" "$NSI_FILE"

echo ""
echo "Done: $SCRIPT_DIR/TokenMonitor-PrereqSetup.exe"
