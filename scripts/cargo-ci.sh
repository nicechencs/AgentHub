#!/usr/bin/env bash
# CI helper: retry cargo on ETXTBSY, and run package cargo commands in parallel.
# Bash 3.2 compatible (macOS /bin/bash).
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/cargo-ci.sh retry [--attempts N] [--delay-seconds S] [--] <command> [args...]
  scripts/cargo-ci.sh parallel [--attempts N] [--delay-seconds S] [--] <command-string> [<command-string> ...]

Retries a command only when that attempt's output contains ETXTBSY or
"Text file busy". Other failures exit immediately (no masking of real
test failures). parallel runs each command-string in the background,
waits for all, prints a summary, and fails if any command failed.

Environment (overridden by flags):
  CARGO_CI_ATTEMPTS        default 3
  CARGO_CI_DELAY_SECONDS   default 3
EOF
}

is_positive_int() {
  case "${1:-}" in
    ''|*[!0-9]*|0) return 1 ;;
    *) return 0 ;;
  esac
}

is_non_negative_int() {
  case "${1:-}" in
    ''|*[!0-9]*) return 1 ;;
    *) return 0 ;;
  esac
}

is_etxtbsy_log() {
  grep -qiE 'ETXTBSY|Text file busy' "$1"
}

label_for() {
  local cmd="$1"
  local idx="$2"
  local pkg
  pkg="$(printf '%s\n' "$cmd" | sed -n 's/.*[[:space:]]-p[[:space:]]\{1,\}\([A-Za-z0-9_-]\{1,\}\).*/\1/p')"
  pkg="${pkg%%$'\n'*}"
  if [ -n "$pkg" ]; then
    printf '%s' "$pkg"
  else
    printf 'job-%s' "$idx"
  fi
}

retry_cmd() {
  local attempts="${CARGO_CI_ATTEMPTS:-3}"
  local delay="${CARGO_CI_DELAY_SECONDS:-3}"
  local n=1
  local log status
  log="$(mktemp "${TMPDIR:-/tmp}/cargo-ci.XXXXXX")"
  while :; do
    : >"$log"
    set +e
    set +o pipefail
    "$@" 2>&1 | tee -a "$log"
    status=${PIPESTATUS[0]}
    set -e
    set -o pipefail
    if [ "$status" -eq 0 ]; then
      rm -f "$log"
      return 0
    fi
    if is_etxtbsy_log "$log"; then
      if [ "$n" -ge "$attempts" ]; then
        echo "cargo-ci: still ETXTBSY after ${attempts} attempts; giving up: $*" >&2
        rm -f "$log"
        # return in a conditional so set -e does not abort the caller shell
        if [ "$status" -ne 0 ]; then
          return "$status"
        fi
      fi
      echo "cargo-ci: ETXTBSY/Text file busy (attempt ${n}/${attempts}); retry in ${delay}s: $*" >&2
      n=$((n + 1))
      sleep "$delay"
      continue
    fi
    rm -f "$log"
    if [ "$status" -ne 0 ]; then
      return "$status"
    fi
  done
}

cmd_parallel() {
  if [ "$#" -eq 0 ]; then
    echo "cargo-ci: parallel requires at least one command" >&2
    exit 2
  fi

  local workdir idx cmd label pid pids st fail
  workdir="$(mktemp -d "${TMPDIR:-/tmp}/cargo-ci-parallel.XXXXXX")"
  # Expand the path now: a nested EXIT function would see an unbound local
  # after this function returns. Children must clear the trap so they do
  # not delete the directory while siblings are still running.
  trap "rm -rf -- '$workdir'" EXIT INT TERM

  pids=""
  idx=0
  for cmd in "$@"; do
    idx=$((idx + 1))
    label="$(label_for "$cmd" "$idx")"
    printf '%s\n' "$label" >"$workdir/$idx.label"
    printf '%s\n' "$cmd" >"$workdir/$idx.cmd"
    (
      trap - EXIT INT TERM
      echo "cargo-ci: start [$label] $cmd"
      # Call in || so retry_cmd's set -e + non-zero return cannot abort
      # the subshell before the status file is written.
      st=0
      retry_cmd bash -c "$cmd" || st=$?
      echo "$st" >"$workdir/$idx.status"
      echo "cargo-ci: done [$label] exit $st"
    ) &
    pid=$!
    pids="$pids $pid"
  done

  set +e
  for pid in $pids; do
    wait "$pid"
  done
  set -e

  fail=0
  idx=0
  echo "cargo-ci: parallel summary"
  for cmd in "$@"; do
    idx=$((idx + 1))
    if [ ! -f "$workdir/$idx.status" ]; then
      echo "cargo-ci: summary [$(cat "$workdir/$idx.label")] missing status (command did not finish)" >&2
      fail=1
      continue
    fi
    st="$(cat "$workdir/$idx.status")"
    label="$(cat "$workdir/$idx.label")"
    echo "cargo-ci: summary [$label] exit $st"
    if [ "$st" != "0" ]; then
      fail=1
    fi
  done
  if [ "$fail" -ne 0 ]; then
    echo "cargo-ci: one or more parallel commands failed" >&2
    exit 1
  fi
  rm -rf -- "$workdir"
  trap - EXIT INT TERM
}

main() {
  if [ "$#" -lt 1 ]; then
    usage >&2
    exit 2
  fi

  local subcommand="$1"
  shift

  case "$subcommand" in
    -h|--help)
      usage
      exit 0
      ;;
    retry|parallel) ;;
    *)
      echo "cargo-ci: unknown command: $subcommand" >&2
      usage >&2
      exit 2
      ;;
  esac

  local attempts="${CARGO_CI_ATTEMPTS:-3}"
  local delay="${CARGO_CI_DELAY_SECONDS:-3}"
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --attempts)
        attempts="${2:-}"
        shift 2
        ;;
      --delay-seconds)
        delay="${2:-}"
        shift 2
        ;;
      --)
        shift
        break
        ;;
      -*)
        echo "cargo-ci: unknown option: $1" >&2
        usage >&2
        exit 2
        ;;
      *)
        break
        ;;
    esac
  done

  if ! is_positive_int "$attempts"; then
    echo "cargo-ci: --attempts must be a positive integer" >&2
    exit 2
  fi
  if ! is_non_negative_int "$delay"; then
    echo "cargo-ci: --delay-seconds must be a non-negative integer" >&2
    exit 2
  fi
  export CARGO_CI_ATTEMPTS="$attempts"
  export CARGO_CI_DELAY_SECONDS="$delay"

  case "$subcommand" in
    retry)
      if [ "$#" -eq 0 ]; then
        echo "cargo-ci: retry requires a command" >&2
        usage >&2
        exit 2
      fi
      retry_cmd "$@"
      ;;
    parallel)
      cmd_parallel "$@"
      ;;
  esac
}

main "$@"
