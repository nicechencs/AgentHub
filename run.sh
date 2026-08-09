#!/usr/bin/env bash
# AgentHub macOS/Unix desktop launcher.
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

if [[ "$(uname -s)" != "Darwin" ]]; then
  warn "run.sh is intended for macOS; continuing with the detected host."
fi

command -v node >/dev/null 2>&1 || fail "Node.js not found. Install the LTS release: https://nodejs.org/"
command -v cargo >/dev/null 2>&1 || fail "Rust/Cargo not found. Install rustup: https://rustup.rs/"

if ! command -v xcode-select >/dev/null 2>&1 || ! xcode-select -p >/dev/null 2>&1; then
  warn "Xcode Command Line Tools are missing. Install them with: xcode-select --install"
  fail "A working C/C++ linker is required by Tauri/Rust."
fi

if ! command -v brew >/dev/null 2>&1; then
  warn "Homebrew was not found. Runtime repair will show official downloads instead."
  warn "Optional install instructions: https://brew.sh/"
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

info "Starting Tauri development app (real backend)..."
info "First Rust build may take a while. Press Ctrl+C to stop."
exec pnpm tauri:dev "$@"
