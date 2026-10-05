#!/usr/bin/env bash
# Rust gateway in-memory capture/stop/exact-restore real-listener probe. Linux only.

set -euo pipefail
umask 077

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
[[ $# -eq 0 ]] || { echo "FAIL: this probe accepts no arguments" >&2; exit 1; }
[[ "$(uname -s)" == "Linux" ]] || { echo "FAIL: this probe currently requires Linux" >&2; exit 1; }

SCRATCH_INPUT="$(mktemp -d /tmp/agenthub-gateway-snapshot.XXXXXXXXXX)"
SCRATCH="$(cd "${SCRATCH_INPUT}" && pwd -P)"
case "${SCRATCH}" in
  /tmp/agenthub-gateway-snapshot.*) ;;
  *) echo "FAIL: unexpected scratch root" >&2; exit 1 ;;
esac
BUILD_LOG="${SCRATCH}/build.log"
RUN_LOG="${SCRATCH}/run.log"
EVIDENCE="${SCRATCH}/evidence.json"

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  if [[ ${status} -eq 0 ]]; then
    rm -rf -- "${SCRATCH}"
  else
    echo "probe evidence retained at ${SCRATCH}" >&2
  fi
  exit "${status}"
}
trap cleanup EXIT INT TERM

(cd "${ROOT}" && cargo build -p agenthub-core --example gateway_snapshot_probe \
  --features gateway-snapshot-probe --locked) >"${BUILD_LOG}" 2>&1
BIN="${ROOT}/target/debug/examples/gateway_snapshot_probe"
[[ -x "${BIN}" ]] || { echo "FAIL: gateway snapshot probe binary missing" >&2; exit 1; }
"${BIN}" "${SCRATCH}" >"${RUN_LOG}" 2>&1

python3 - "${RUN_LOG}" "${EVIDENCE}" <<'PY'
import json, sys
run_log, evidence_path = sys.argv[1:]
with open(run_log, encoding="utf-8") as handle:
    rows = [line.strip() for line in handle if line.strip().startswith("{")]
assert rows, "probe emitted no evidence"
evidence = json.loads(rows[-1])
assert evidence["schema"] == "gateway-snapshot-probe.v1", evidence
assert evidence["status"] == "ok", evidence
for key in (
    "same_port_restore", "occupied_port_bind_error", "no_partial_entry",
    "retry_same_snapshot", "authorized_health", "unauthorized_health",
    "db_wal_unchanged", "port_released", "secret_scan",
    "captured_drop_releases", "stopped_drop_fail_closed",
    "explicit_commit_releases", "mutator_gate",
    "observed_stop_error_restores", "second_entry_cleanup",
    "health_failure_cleanup",
):
    assert evidence[key] is True, (key, evidence)
assert evidence["entry_count"] == 2, evidence
with open(evidence_path, "w", encoding="utf-8") as handle:
    json.dump(evidence, handle, sort_keys=True)
    handle.write("\n")
PY

if grep -F -e "ahb-gateway-snapshot-local-a-secret" \
  -e "ahb-gateway-snapshot-local-b-secret" \
  -e "ahb-gateway-snapshot-extra-secret" \
  -e "sk-gateway-snapshot-upstream-secret" \
  "${RUN_LOG}" "${BUILD_LOG}"; then
  echo "FAIL: synthetic secret leaked into probe logs" >&2
  exit 1
fi

cat "${EVIDENCE}"
echo "PASS: Rust gateway snapshot exact restore probe"
