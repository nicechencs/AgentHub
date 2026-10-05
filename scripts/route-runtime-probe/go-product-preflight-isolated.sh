#!/usr/bin/env bash
# Read-only Product preflight real-store probe. Linux only.

set -euo pipefail
umask 077

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
[[ $# -eq 0 ]] || { echo "FAIL: this probe accepts no arguments" >&2; exit 1; }
[[ "$(uname -s)" == "Linux" ]] || { echo "FAIL: this probe currently requires Linux" >&2; exit 1; }

SCRATCH_INPUT="$(mktemp -d /tmp/agenthub-go-product-preflight.XXXXXXXXXX)"
SCRATCH="$(cd "${SCRATCH_INPUT}" && pwd -P)"
case "${SCRATCH}" in
  /tmp/agenthub-go-product-preflight.*) ;;
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

ADAPTERD_BEFORE="$(pgrep -x agenthub-adapterd 2>/dev/null || true)"
(cd "${ROOT}" && cargo build -p agenthub-core --example go_product_preflight_probe \
  --features go-product-preflight-probe --locked) >"${BUILD_LOG}" 2>&1
BIN="${ROOT}/target/debug/examples/go_product_preflight_probe"
[[ -x "${BIN}" ]] || { echo "FAIL: Product preflight probe binary missing" >&2; exit 1; }
"${BIN}" "${SCRATCH}" >"${RUN_LOG}" 2>&1
ADAPTERD_AFTER="$(pgrep -x agenthub-adapterd 2>/dev/null || true)"
[[ "${ADAPTERD_AFTER}" == "${ADAPTERD_BEFORE}" ]] || {
  echo "FAIL: Product preflight changed adapterd process count" >&2
  exit 1
}

python3 - "${RUN_LOG}" "${EVIDENCE}" <<'PY'
import json, sys
run_log, evidence_path = sys.argv[1:]
with open(run_log, encoding="utf-8") as handle:
    rows = [line.strip() for line in handle if line.strip().startswith("{")]
assert rows, "probe emitted no evidence"
evidence = json.loads(rows[-1])
assert evidence["schema"] == "go-product-preflight-probe.v1", evidence
assert evidence["status"] == "ok", evidence
expected = [
    "no_pool", "missing_saved_port", "zero_saved_port", "multiple_saved_ports",
    "uncovered_legacy_profile", "profile_port_mismatch", "config_incompatible", "eligible",
]
assert [case["reason"] for case in evidence["cases"]] == expected, evidence
for case in evidence["cases"]:
    assert case["db_unchanged"] is True, case
    assert case["socket_count_unchanged"] is True, case
    assert case["adapterd_process_count_unchanged"] is True, case
    assert case["config_present"] is (case["reason"] == "eligible"), case
for key in (
    "db_unchanged", "socket_count_unchanged",
    "adapterd_process_count_unchanged", "summary_secret_scan",
):
    assert evidence[key] is True, (key, evidence)
with open(evidence_path, "w", encoding="utf-8") as handle:
    json.dump(evidence, handle, sort_keys=True)
    handle.write("\n")
PY

if grep -F -q "sk-product-preflight-probe-secret-do-not-use" \
  "${RUN_LOG}" "${BUILD_LOG}"; then
  echo "FAIL: synthetic source secret leaked into probe logs" >&2
  exit 1
fi

cat "${EVIDENCE}"
echo "PASS: read-only Product preflight probe"
