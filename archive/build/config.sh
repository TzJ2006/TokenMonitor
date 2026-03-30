#!/usr/bin/env bash
# Shared build configuration for TokenMonitor
# Sourced by all build/*.sh scripts — not executed directly.

# ── Versions ──────────────────────────────────────────────────────────────────
NODE_VERSION="22.14.0"
CCUSAGE_PACKAGE="@ccusage/mcp"

# ── Project info (derived) ────────────────────────────────────────────────────
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="TokenMonitor"
APP_VERSION=$(cd "$ROOT_DIR" && node -p "require('./package.json').version" 2>/dev/null || echo "0.0.0")

# ── Directories ───────────────────────────────────────────────────────────────
BUILD_DIR="$ROOT_DIR/build"
CACHE_DIR="$BUILD_DIR/cache"
OUTPUT_DIR="$ROOT_DIR/outputs"

# ── Platform detection ────────────────────────────────────────────────────────
detect_platform() {
  case "$(uname -s)" in
    Darwin)                       echo "macos"   ;;
    Linux)                        echo "linux"   ;;
    MINGW*|MSYS*|CYGWIN*|Windows_NT) echo "windows" ;;
    *)                            echo "unknown" ;;
  esac
}

detect_arch() {
  case "$(uname -m)" in
    x86_64|amd64)  echo "x64"   ;;
    arm64|aarch64) echo "arm64" ;;
    *)             echo "x64"   ;;
  esac
}

PLATFORM=$(detect_platform)
ARCH=$(detect_arch)

# ── Helpers ───────────────────────────────────────────────────────────────────
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

info()  { echo -e "${GREEN}[OK]${NC}  $1"; }
warn()  { echo -e "${YELLOW}[!!]${NC}  $1"; }
fail()  { echo -e "${RED}[ERR]${NC} $1"; exit 1; }
step()  { echo -e "\n=== $1 ==="; }
