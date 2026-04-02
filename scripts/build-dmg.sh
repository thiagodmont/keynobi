#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# build-dmg.sh — Build Android Dev Companion as a distributable .dmg
#
# Usage:
#   ./scripts/build-dmg.sh              # Apple Silicon (default)
#   ./scripts/build-dmg.sh --intel      # Intel x86_64
#   ./scripts/build-dmg.sh --universal  # Universal binary (both arches)
#
# The resulting .dmg is placed in:
#   src-tauri/target/<arch>/release/bundle/dmg/
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# ── Colour helpers ────────────────────────────────────────────────────────────
BOLD="\033[1m"
GREEN="\033[0;32m"
YELLOW="\033[0;33m"
RED="\033[0;31m"
RESET="\033[0m"

info()    { echo -e "${BOLD}${GREEN}  ✓${RESET} $*"; }
warning() { echo -e "${BOLD}${YELLOW}  ⚠${RESET} $*"; }
error()   { echo -e "${BOLD}${RED}  ✗${RESET} $*" >&2; }
step()    { echo -e "\n${BOLD}▶ $*${RESET}"; }

# ── Argument parsing ──────────────────────────────────────────────────────────
TARGET_FLAG="--target aarch64-apple-darwin"
TARGET_DIR="aarch64-apple-darwin"
ARCH_LABEL="Apple Silicon (aarch64)"

for arg in "$@"; do
  case $arg in
    --intel)
      TARGET_FLAG="--target x86_64-apple-darwin"
      TARGET_DIR="x86_64-apple-darwin"
      ARCH_LABEL="Intel (x86_64)"
      ;;
    --universal)
      TARGET_FLAG="--target universal-apple-darwin"
      TARGET_DIR="universal-apple-darwin"
      ARCH_LABEL="Universal (Apple Silicon + Intel)"
      ;;
    --help|-h)
      echo "Usage: $0 [--intel | --universal]"
      echo "  (no flag)    Build for Apple Silicon"
      echo "  --intel      Build for Intel x86_64"
      echo "  --universal  Build universal binary (requires both Rust targets)"
      exit 0
      ;;
  esac
done

# ── Prerequisites ─────────────────────────────────────────────────────────────
step "Checking prerequisites"

# Rust
if ! command -v rustc &>/dev/null; then
  # Try the standard cargo env location
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env" 2>/dev/null || true
fi

if ! command -v cargo &>/dev/null; then
  error "Rust/Cargo not found. Install from https://rustup.rs/"
  exit 1
fi
info "Rust $(rustc --version)"

# Node / npm
if ! command -v node &>/dev/null; then
  error "Node.js not found. Install from https://nodejs.org/"
  exit 1
fi
info "Node $(node --version)"

# ── Rust target installation ──────────────────────────────────────────────────
step "Ensuring Rust target(s) installed for $ARCH_LABEL"

install_target() {
  local t="$1"
  if ! rustup target list --installed | grep -q "^${t}$"; then
    warning "Installing Rust target: $t"
    rustup target add "$t"
  else
    info "Target already installed: $t"
  fi
}

case "$TARGET_DIR" in
  universal-apple-darwin)
    install_target "aarch64-apple-darwin"
    install_target "x86_64-apple-darwin"
    ;;
  *)
    install_target "$TARGET_DIR"
    ;;
esac

# ── npm dependencies ──────────────────────────────────────────────────────────
step "Installing npm dependencies"
npm install --silent
info "npm dependencies up to date"

# ── Build ─────────────────────────────────────────────────────────────────────
step "Building Android Dev Companion — $ARCH_LABEL"
echo    "  This compiles the Rust backend in release mode and bundles the frontend."
echo    "  First build takes 3–8 minutes. Subsequent builds are ~30 seconds."
echo

# Unsigned build: set APPLE_SIGNING_IDENTITY to "-" (ad-hoc) so Tauri doesn't
# require a Developer ID certificate. The app will run on your own machine and
# can be shared for testing, but macOS Gatekeeper will warn other users.
# Remove this env var and set bundle.macOS.signingIdentity in tauri.conf.json
# when you have a Developer ID certificate.
export APPLE_SIGNING_IDENTITY="-"

# Run the Tauri build
# shellcheck disable=SC2086
npm run tauri build -- $TARGET_FLAG --bundles dmg 2>&1

# ── Locate the .dmg ───────────────────────────────────────────────────────────
DMG_DIR="$ROOT/src-tauri/target/$TARGET_DIR/release/bundle/dmg"
DMG_FILE=$(find "$DMG_DIR" -name "*.dmg" 2>/dev/null | head -1)

echo

if [[ -z "$DMG_FILE" ]]; then
  error "Build succeeded but no .dmg found in $DMG_DIR"
  error "Check the Tauri build output above for bundle errors."
  exit 1
fi

DMG_SIZE=$(du -sh "$DMG_FILE" | cut -f1)

step "Done!"
info "DMG: $DMG_FILE  ($DMG_SIZE)"

# Open the containing folder in Finder
open -R "$DMG_FILE"

echo
echo -e "${BOLD}To install:${RESET}"
echo "  1. Open the .dmg"
echo "  2. Drag Android Dev Companion → Applications"
echo "  3. On first launch: right-click the app → Open (bypasses Gatekeeper for unsigned builds)"
echo
