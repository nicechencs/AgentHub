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
STOP_ONLY=0
RESTART=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --check)
      CHECK_ONLY=1
      shift
      ;;
    --stop)
      STOP_ONLY=1
      shift
      ;;
    --restart)
      RESTART=1
      shift
      ;;
    -h|--help)
      cat <<'EOF'
Usage: ./run.sh [--check] [--stop] [--restart] [--] [tauri args...]

  --check     Verify toolchain + platform native deps, then exit.
              Does not start the desktop app.
  --stop      Stop this repo's leftover desktop/dev processes and free the
              Vite port, then exit.
  --restart   Same as --stop, then start the desktop app.
              Equivalent: ./restart.sh

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

read_dev_port() {
  local file="$SCRIPT_DIR/scripts/dev-runtime.json"
  local port=""
  if [[ -f "$file" ]] && command -v node >/dev/null 2>&1; then
    port="$(node -e 'const r=require(process.argv[1]); process.stdout.write(String(r.port||""))' "$file" 2>/dev/null || true)"
  fi
  if [[ "$port" =~ ^[0-9]+$ ]]; then
    printf '%s\n' "$port"
  else
    printf '5173\n'
  fi
}

is_self_pid() {
  local target="$1" walk="$$"
  local n=0
  while [[ "$walk" =~ ^[0-9]+$ && "$walk" -gt 1 && "$n" -lt 12 ]]; do
    if [[ "$walk" == "$target" ]]; then
      return 0
    fi
    walk="$(ps -o ppid= -p "$walk" 2>/dev/null | tr -d ' ')"
    n=$((n + 1))
  done
  return 1
}

add_pid() {
  local pid="$1"
  [[ "$pid" =~ ^[0-9]+$ ]] || return 0
  is_self_pid "$pid" && return 0
  local existing
  for existing in "${PIDS[@]+"${PIDS[@]}"}"; do
    if [[ "$existing" == "$pid" ]]; then
      return 0
    fi
  done
  PIDS+=("$pid")
}

add_parent_if_launcher() {
  local pid="$1" n=0 ppid cmd
  while [[ "$n" -lt 6 ]]; do
    ppid="$(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ')"
    [[ "$ppid" =~ ^[0-9]+$ && "$ppid" -gt 1 ]] || break
    cmd="$(ps -o command= -p "$ppid" 2>/dev/null || true)"
    case "$cmd" in
      *"pnpm tauri:dev"*|*/tauri.js*dev*)
        add_pid "$ppid"
        pid="$ppid"
        ;;
      *)
        break
        ;;
    esac
    n=$((n + 1))
  done
}

collect_dev_pids() {
  local port="$1" pid cmd
  PIDS=()
  if command -v lsof >/dev/null 2>&1; then
    while read -r pid; do
      add_pid "$pid"
    done < <(lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null || true)
  fi
  while IFS= read -r line; do
    line="${line#"${line%%[![:space:]]*}"}"
    pid="${line%% *}"
    cmd="${line#* }"
    case "$cmd" in
      *"$SCRIPT_DIR/target/debug/agenthub-gui"*|\
      *"$SCRIPT_DIR/target/release/agenthub-gui"*|\
      *"$SCRIPT_DIR"*/vite/bin/vite.js*|\
      *"$SCRIPT_DIR"*/@tauri-apps/cli/tauri.js*|\
      *"$SCRIPT_DIR"*/esbuild*)
        add_pid "$pid"
        add_parent_if_launcher "$pid"
        ;;
    esac
  done < <(ps -ax -o pid= -o command= 2>/dev/null || true)
}

stop_dev_processes() {
  local port pid still
  port="$(read_dev_port)"
  info "Stopping leftover AgentHub desktop/dev processes (port $port)..."
  collect_dev_pids "$port"
  if [[ ${#PIDS[@]} -eq 0 ]]; then
    info "No leftover AgentHub dev process."
    return 0
  fi
  for pid in "${PIDS[@]}"; do
    info "Stopping PID $pid"
    kill "$pid" 2>/dev/null || true
  done
  sleep 0.5
  for pid in "${PIDS[@]}"; do
    if kill -0 "$pid" 2>/dev/null; then
      warn "Force-stopping PID $pid"
      kill -9 "$pid" 2>/dev/null || true
    fi
  done
  sleep 0.2
  if command -v lsof >/dev/null 2>&1; then
    still="$(lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null | tr '\n' ' ' || true)"
    if [[ -n "${still// /}" ]]; then
      fail "Port $port is still busy (PID ${still}). Close it manually, then retry."
    fi
  fi
  info "Port $port is free."
}

HOST="$(uname -s)"
case "$HOST" in
  Darwin|Linux) ;;
  *)
    fail "Unsupported host '$HOST'. AgentHub desktop supports Windows (run.ps1), macOS, and Linux."
    ;;
esac

if [[ "$STOP_ONLY" -eq 1 || "$RESTART" -eq 1 ]]; then
  stop_dev_processes
  if [[ "$STOP_ONLY" -eq 1 ]]; then
    exit 0
  fi
fi

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
