#!/usr/bin/env bash
# Check AgentHub / Tauri v2 native build dependencies on Linux.
# This script never uses sudo and never mutates a system package manager.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/check-linux-prereqs.sh [--check] [--print-packages]

  --check            Probe the current host (default). Exit 0 if ready, 1 if not.
  --print-packages   Print copyable install commands and other-distro hints, then exit 0.
  -h, --help         Show this help.

Verification: ./scripts/check-linux-prereqs.sh --check
EOF
}

MODE="check"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check) MODE="check"; shift ;;
    --print-packages) MODE="print"; shift ;;
    -h|--help) usage; exit 0 ;;
    *)
      printf '[ERROR] Unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

print_packages() {
  cat <<'EOF'
# Debian / Ubuntu (matches Tauri v2 + this repo's Linux CI)
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  curl \
  wget \
  file \
  pkg-config \
  libwebkit2gtk-4.1-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libxdo-dev

# Fedora
sudo dnf install -y \
  webkit2gtk4.1-devel \
  openssl-devel \
  curl \
  wget \
  file \
  libappindicator-gtk3-devel \
  librsvg2-devel \
  libxdo-devel \
  gcc \
  gcc-c++ \
  pkgconf-pkg-config

# Arch
sudo pacman -S --needed \
  webkit2gtk-4.1 \
  base-devel \
  curl \
  wget \
  file \
  openssl \
  appmenu-gtk-module \
  libappindicator-gtk3 \
  librsvg \
  xdotool \
  pkgconf

# openSUSE / SUSE and Alpine: do not assume apt-get.
# Install equivalent WebKitGTK 4.1 / SSL / AppIndicator / rsvg / xdo packages
# with zypper or apk, or follow https://v2.tauri.app/start/prerequisites/#linux
# Packaged desktop clients: GitHub Releases (.deb or AppImage).
EOF
}

if [[ "$MODE" == "print" ]]; then
  print_packages
  exit 0
fi

if [[ "$(uname -s)" != "Linux" ]]; then
  printf '[ERROR] This checker is for Linux. Detected %s.\n' "$(uname -s)" >&2
  exit 1
fi

info() { printf '[INFO] %s\n' "$*"; }
warn() { printf '[WARN] %s\n' "$*" >&2; }
fail_line() { printf '[ERROR] %s\n' "$*" >&2; }

MISSING=0

require_cmd() {
  local name="$1"
  local hint="$2"
  if command -v "$name" >/dev/null 2>&1; then
    info "found command: $name"
  else
    fail_line "missing command: $name ($hint)"
    MISSING=1
  fi
}

pkg_exists() {
  local name
  for name in "$@"; do
    if pkg-config --exists "$name" 2>/dev/null; then
      printf '%s' "$name"
      return 0
    fi
  done
  return 1
}

require_pkg() {
  local label="$1"
  shift
  local found
  if found="$(pkg_exists "$@")"; then
    info "found pkg-config: $found"
  else
    fail_line "missing library: $label (pkg-config tried: $*)"
    MISSING=1
  fi
}

require_header() {
  local header="$1"
  local hint="$2"
  local cc="${CC:-}"
  if [[ -z "$cc" ]]; then
    if command -v gcc >/dev/null 2>&1; then
      cc=gcc
    elif command -v cc >/dev/null 2>&1; then
      cc=cc
    else
      fail_line "missing header: $header ($hint); no C compiler to probe includes"
      MISSING=1
      return
    fi
  fi
  if printf '#include <%s>\n' "$header" | "$cc" -E - >/dev/null 2>&1; then
    info "found header: $header"
  else
    fail_line "missing header: $header ($hint)"
    MISSING=1
  fi
}

require_cmd gcc "install build-essential / gcc"
if ! command -v g++ >/dev/null 2>&1 && ! command -v c++ >/dev/null 2>&1; then
  fail_line "missing C++ compiler (g++ / c++)"
  MISSING=1
else
  info "found C++ compiler"
fi
require_cmd pkg-config "install pkg-config / pkgconf"
require_cmd curl "needed by rustup and some native installers"
require_cmd file "needed by Tauri Linux bundlers"

if command -v pkg-config >/dev/null 2>&1; then
  require_pkg "webkit2gtk-4.1 (libwebkit2gtk-4.1-dev)" webkit2gtk-4.1
  require_pkg "OpenSSL (libssl-dev / openssl-devel)" libssl openssl
  require_pkg "librsvg (librsvg2-dev)" librsvg-2.0
  require_pkg "AppIndicator (libayatana-appindicator3-dev)" \
    ayatana-appindicator3-0.1 appindicator3-0.1
else
  fail_line "skipping pkg-config library probes because pkg-config is missing"
fi

require_header xdo.h "libxdo-dev / libxdo-devel / xdotool"

if [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]; then
  warn "No DISPLAY or WAYLAND_DISPLAY. The desktop app needs a graphical session."
  warn "Headless --check of libraries can still pass; launching the GUI will fail."
fi

if [[ "$MISSING" -ne 0 ]]; then
  fail_line "Linux native build dependencies are incomplete."
  printf '\nInstall the packages for your distribution, then re-run this check:\n\n' >&2
  print_packages >&2
  printf '\nAlso required (user-level, no sudo from this script):\n' >&2
  printf '  Node.js LTS  https://nodejs.org/\n' >&2
  printf '  Rust/Cargo   https://rustup.rs/\n' >&2
  exit 1
fi

info "Linux native build dependencies look ready."
exit 0
