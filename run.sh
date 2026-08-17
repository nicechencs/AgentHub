#!/usr/bin/env bash
# AgentHub Unix desktop launcher (macOS / Linux).
# This script only installs project dependencies; it never uses sudo or
# mutates a system package manager without an explicit user command.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

info() { printf '[INFO] %s\n' "$*"; }
warn() { printf '[WARN] %s\n' "$*" >&2; }
fail() {
  printf '[ERROR] %s\n' "$*" >&2
  exit 1
}

CHECK_ONLY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)
      CHECK_ONLY=1
      shift
      ;;
    -h|--help)
      cat <<'EOF'
Usage: ./run.sh [--check] [--] [tauri args...]

  --check   Verify toolchain + platform native deps, then exit.
            Does not start the desktop app.

macOS: requires Xcode Command Line Tools, Node.js, and Rust.
Linux: requires Tauri native libraries (see scripts/check-linux-prereqs.sh),
       Node.js, and Rust. Desktop installers also ship on GitHub Releases
       as .deb and AppImage (unsigned is OK).
EOF
      exit 0
      ;;
    --)
      shift
      break
      ;;
    *)
      break
      ;;
  esac
done

HOST="$(uname -s)"
case "$HOST" in
  Darwin|Linux) ;;
  *)
    fail "Unsupported host '$HOST'. AgentHub desktop supports Windows (run.ps1), macOS, and Linux."
    ;;
esac

command -v node >/dev/null 2>&1 || fail "Node.js not found. Install the LTS release: https://nodejs.org/"
command -v cargo >/dev/null 2>&1 || fail "Rust/Cargo not found. Install rustup: https://rustup.rs/"

if [[ "$HOST" == "Darwin" ]]; then
  if ! command -v xcode-select >/dev/null 2>&1 || ! xcode-select -p >/dev/null 2>&1; then
    warn "Xcode Command Line Tools are missing. Install them with: xcode-select --install"
    fail "A working C/C++ linker is required by Tauri/Rust."
  fi
  if ! command -v brew >/dev/null 2>&1; then
    warn "Homebrew was not found. Runtime repair will show official downloads instead."
    warn "Optional install instructions: https://brew.sh/"
  fi
else
  CHECKER="$SCRIPT_DIR/scripts/check-linux-prereqs.sh"
  [[ -f "$CHECKER" ]] || fail "Missing $CHECKER"
  bash "$CHECKER" --check
  if [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]; then
    if [[ "$CHECK_ONLY" -eq 1 ]]; then
      warn "No graphical session detected. GUI launch would fail; library check still ran."
    else
      fail "No DISPLAY or WAYLAND_DISPLAY. Start a graphical session, or use: pnpm dev:mock"
    fi
  fi
fi

if ! command -v pnpm >/dev/null 2>&1; then
  command -v npm >/dev/null 2>&1 || fail "pnpm and npm are missing. Install Node.js first."
  info "pnpm not found; installing it for the current user via npm..."
  npm install --global pnpm || fail "Could not install pnpm. Try: npm install --global pnpm"
fi

if [[ ! -d node_modules ]]; then
  info "Installing dependencies with pnpm install..."
  pnpm install
fi

if [[ "$CHECK_ONLY" -eq 1 ]]; then
  info "Toolchain check passed on $HOST."
  info "node $(node -v) | pnpm $(pnpm -v) | cargo $(cargo --version)"
  if [[ "$HOST" == "Linux" ]]; then
    info "Next: ./run.sh            # start Tauri desktop (real backend)"
    info "  or: pnpm tauri:build:linux  # local unsigned .deb + AppImage"
  else
    info "Next: ./run.sh            # start Tauri desktop (real backend)"
    info "  or: pnpm tauri:build:macos  # local unsigned .app"
  fi
  exit 0
fi

info "Starting Tauri development app (real backend)..."
info "First Rust build may take a while. Press Ctrl+C to stop."
exec pnpm tauri:dev "$@"
