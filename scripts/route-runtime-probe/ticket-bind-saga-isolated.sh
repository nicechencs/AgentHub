#!/usr/bin/env bash
# Real Claude config-write and compensation probe, confined to a disposable tree.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
SCRATCH_INPUT="${1:-/tmp/agenthub-ticket-bind-saga/${RUN_ID}}"
REAL_HOME="$(cd "${HOME}" && pwd -P)"

mkdir -p "${SCRATCH_INPUT}"
SCRATCH="$(cd "${SCRATCH_INPUT}" && pwd -P)"
case "${SCRATCH}" in
  /tmp/agenthub-ticket-bind-saga/*|/private/tmp/agenthub-ticket-bind-saga/*|/var/tmp/agenthub-ticket-bind-saga/*) ;;
  *) echo "FAIL: scratch must be under /tmp/agenthub-ticket-bind-saga or /var/tmp/agenthub-ticket-bind-saga" >&2; exit 1 ;;
esac
case "${SCRATCH}" in
  "${REAL_HOME}"|"${REAL_HOME}"/*) echo "FAIL: refusing scratch inside the real user home" >&2; exit 1 ;;
esac

BUILD_LOG="${SCRATCH}/build.log"

scan_secret_logs() {
  local logs=()
  local candidate
  for candidate in \
    "${BUILD_LOG}" \
    "${SCRATCH}/success-unbind.log" \
    "${SCRATCH}/finalize-failure.log"; do
    if [[ -f "${candidate}" ]]; then
      logs+=("${candidate}")
    fi
  done
  if [[ "${#logs[@]}" -eq 0 ]]; then
    return 0
  fi

  local secret
  for secret in \
    "sk-agenthub-probe-kimi-source-do-not-use-000000" \
    "oauth-agenthub-probe-access-do-not-use-000000" \
    "oauth-agenthub-probe-refresh-do-not-use-000000"; do
    if grep -F -q -- "${secret}" "${logs[@]}"; then
      echo "FAIL: synthetic secret appeared in probe logs" >&2
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

(cd "${ROOT}" && cargo build -p agenthub-core --example ticket_bind_saga_probe --locked) >"${BUILD_LOG}" 2>&1
BIN="${ROOT}/target/debug/examples/ticket_bind_saga_probe"
[[ -x "${BIN}" ]] || { echo "FAIL: probe executable missing" >&2; exit 1; }

run_scenario() {
  local scenario="$1"
  local scenario_root="${SCRATCH}/${scenario}"
  local run_log="${SCRATCH}/${scenario}.log"
  mkdir -p \
    "${scenario_root}/data" \
    "${scenario_root}/skills" \
    "${scenario_root}/claude" \
    "${scenario_root}/home" \
    "${scenario_root}/xdg-config" \
    "${scenario_root}/xdg-data"
  AGENTHUB_PROBE_REAL_HOME="${REAL_HOME}" \
  AGENTHUB_HOME="${scenario_root}/data" \
  CLAUDE_CONFIG_DIR="${scenario_root}/claude" \
  HOME="${scenario_root}/home" \
  XDG_CONFIG_HOME="${scenario_root}/xdg-config" \
  XDG_DATA_HOME="${scenario_root}/xdg-data" \
    "${BIN}" "${scenario}" "${scenario_root}" >"${run_log}" 2>&1
  python3 - "${run_log}" "${scenario}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    lines = [line.strip() for line in handle if line.strip()]
assert lines, "probe emitted no evidence"
evidence = json.loads(lines[-1])
assert evidence["schema"] == "ticket-bind-saga-probe.v1", evidence
assert evidence["scenario"] == sys.argv[2], evidence
assert evidence["status"] == "ok", evidence
PY
}

run_scenario success-unbind
run_scenario finalize-failure

python3 - \
  "${SCRATCH}/success-unbind.log" \
  "${SCRATCH}/finalize-failure.log" \
  "${SCRATCH}/evidence.json" <<'PY'
import json
import sys

items = []
for path in sys.argv[1:3]:
    with open(path, encoding="utf-8") as handle:
        lines = [line.strip() for line in handle if line.strip()]
    items.append(json.loads(lines[-1]))
with open(sys.argv[3], "w", encoding="utf-8") as handle:
    json.dump({"status": "ok", "scenarios": items}, handle, indent=2)
    handle.write("\n")
PY

if grep -F -q -- "do-not-use-000000" "${SCRATCH}/evidence.json"; then
  echo "FAIL: synthetic secret appeared in evidence" >&2
  exit 1
fi

echo "PASS: ticket bind saga real-write probe"
echo "evidence: ${SCRATCH}/evidence.json"
