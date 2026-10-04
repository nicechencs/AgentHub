#!/usr/bin/env bash
# Real RoutePool entry-key transaction probe, confined to a disposable tree.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
SCRATCH_INPUT="${1:-/tmp/agenthub-local-token-saga/${RUN_ID}}"
REAL_HOME="$(cd "${HOME}" && pwd -P)"

mkdir -p "${SCRATCH_INPUT}"
SCRATCH="$(cd "${SCRATCH_INPUT}" && pwd -P)"
case "${SCRATCH}" in
  /tmp/agenthub-local-token-saga/*|/private/tmp/agenthub-local-token-saga/*|/var/tmp/agenthub-local-token-saga/*) ;;
  *) echo "FAIL: scratch must be under /tmp/agenthub-local-token-saga or /var/tmp/agenthub-local-token-saga" >&2; exit 1 ;;
esac
case "${SCRATCH}" in
  "${REAL_HOME}"|"${REAL_HOME}"/*) echo "FAIL: refusing scratch inside the real user home" >&2; exit 1 ;;
esac

mkdir -p \
  "${SCRATCH}/data" \
  "${SCRATCH}/skills" \
  "${SCRATCH}/home" \
  "${SCRATCH}/xdg-config" \
  "${SCRATCH}/xdg-data" \
  "${SCRATCH}/codex"

BUILD_LOG="${SCRATCH}/build.log"
RUN_LOG="${SCRATCH}/run.log"
EVIDENCE="${SCRATCH}/evidence.json"

scan_secret_logs() {
  local candidate
  for candidate in "${BUILD_LOG}" "${RUN_LOG}" "${EVIDENCE}"; do
    if [[ -f "${candidate}" ]] && grep -F -q -- "ahb_probe_" "${candidate}"; then
      echo "FAIL: synthetic entry key appeared in probe output" >&2
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

(cd "${ROOT}" && cargo build -p agenthub-core --example local_token_saga_probe --locked) >"${BUILD_LOG}" 2>&1
BIN="${ROOT}/target/debug/examples/local_token_saga_probe"
[[ -x "${BIN}" ]] || { echo "FAIL: probe executable missing" >&2; exit 1; }

AGENTHUB_PROBE_REAL_HOME="${REAL_HOME}" \
AGENTHUB_HOME="${SCRATCH}/data" \
HOME="${SCRATCH}/home" \
XDG_CONFIG_HOME="${SCRATCH}/xdg-config" \
XDG_DATA_HOME="${SCRATCH}/xdg-data" \
CODEX_HOME="${SCRATCH}/codex" \
  "${BIN}" "${SCRATCH}" >"${RUN_LOG}" 2>&1

python3 - "${RUN_LOG}" "${EVIDENCE}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    lines = [line.strip() for line in handle if line.strip()]
assert lines, "probe emitted no evidence"
evidence = json.loads(lines[-1])
assert evidence["schema"] == "local-token-saga-probe.v1", evidence
assert evidence["status"] == "ok", evidence
for field in (
    "provider_converged",
    "historical_alias_removed",
    "extra_created",
    "primary_duplicate_rolled_back",
    "extra_duplicate_rolled_back",
    "foreign_alias_duplicate_rolled_back",
):
    assert evidence[field] is True, evidence
assert evidence["primary_duplicate_error_code"] == "invalid_arg", evidence
assert evidence["extra_duplicate_error_code"] == "invalid_arg", evidence
assert evidence["foreign_alias_duplicate_error_code"] == "invalid_arg", evidence
assert evidence["listed_key_count"] == 3, evidence
assert evidence["accepted_bearer_count"] == 4, evidence
with open(sys.argv[2], "w", encoding="utf-8") as handle:
    json.dump(evidence, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY

echo "PASS: local token saga real-store probe"
echo "evidence: ${EVIDENCE}"
