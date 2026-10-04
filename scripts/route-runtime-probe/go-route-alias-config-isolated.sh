#!/usr/bin/env bash
# Real Rust-store -> Go runtime-config alias probe in a disposable tree.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
SCRATCH_INPUT="${1:-/tmp/agenthub-go-route-alias-config/${RUN_ID}}"
REAL_HOME="$(cd "${HOME}" && pwd -P)"

mkdir -p "${SCRATCH_INPUT}"
SCRATCH="$(cd "${SCRATCH_INPUT}" && pwd -P)"
case "${SCRATCH}" in
  /tmp/agenthub-go-route-alias-config/*|/private/tmp/agenthub-go-route-alias-config/*|/var/tmp/agenthub-go-route-alias-config/*) ;;
  *) echo "FAIL: scratch must be under the Go route alias probe temp tree" >&2; exit 1 ;;
esac
case "${SCRATCH}" in
  "${REAL_HOME}"|"${REAL_HOME}"/*) echo "FAIL: refusing scratch inside the real user home" >&2; exit 1 ;;
esac

mkdir -p \
  "${SCRATCH}/data" \
  "${SCRATCH}/skills" \
  "${SCRATCH}/home" \
  "${SCRATCH}/xdg-config" \
  "${SCRATCH}/xdg-data"

BUILD_LOG="${SCRATCH}/build.log"
RUN_LOG="${SCRATCH}/run.log"
EVIDENCE="${SCRATCH}/evidence.json"

scan_secret_logs() {
  local candidate
  for candidate in "${BUILD_LOG}" "${RUN_LOG}" "${EVIDENCE}"; do
    if [[ -f "${candidate}" ]] && grep -E -q -- 'ahb_probe_|sk_probe_' "${candidate}"; then
      echo "FAIL: synthetic key appeared in probe output" >&2
      return 1
    fi
  done
}

on_exit() {
  local status=$?
  trap - EXIT
  set +e
  if ! scan_secret_logs && [[ "${status}" -eq 0 ]]; then
    status=1
  fi
  exit "${status}"
}
trap on_exit EXIT

(cd "${ROOT}" && cargo build -p agenthub-core --example go_route_alias_config_probe --locked) >"${BUILD_LOG}" 2>&1
BIN="${ROOT}/target/debug/examples/go_route_alias_config_probe"
[[ -x "${BIN}" ]] || { echo "FAIL: probe executable missing" >&2; exit 1; }

AGENTHUB_HOME="${SCRATCH}/data" \
HOME="${SCRATCH}/home" \
XDG_CONFIG_HOME="${SCRATCH}/xdg-config" \
XDG_DATA_HOME="${SCRATCH}/xdg-data" \
  "${BIN}" "${SCRATCH}" >"${RUN_LOG}" 2>&1

python3 - "${RUN_LOG}" "${EVIDENCE}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    lines = [line.strip() for line in handle if line.strip()]
assert lines, "probe emitted no evidence"
evidence = json.loads(lines[-1])
assert evidence["schema"] == "go-route-alias-config-probe.v1", evidence
assert evidence["status"] == "ok", evidence
assert evidence["edge_count"] == 1, evidence
assert evidence["ingress_key_count"] == 3, evidence
for field in (
    "primary_separate",
    "aliases_deduplicated",
    "config_hash_changes_with_keys",
    "deterministic_serialization",
):
    assert evidence[field] is True, evidence
with open(sys.argv[2], "w", encoding="utf-8") as handle:
    json.dump(evidence, handle, ensure_ascii=False, sort_keys=True)
    handle.write("\n")
PY

scan_secret_logs
echo "PASS: Go route config includes primary, extra, and historical same-pool keys without leaking them"
echo "Evidence: ${EVIDENCE}"
