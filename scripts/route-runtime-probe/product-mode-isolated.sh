#!/usr/bin/env bash
# Dormant Product-mode supervisor real-process probe. Linux only.

set -euo pipefail
umask 077

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
[[ $# -eq 0 ]] || { echo "FAIL: this probe accepts no arguments" >&2; exit 1; }
[[ "$(uname -s)" == "Linux" ]] || { echo "FAIL: this probe currently requires Linux" >&2; exit 1; }

SCRATCH_INPUT="$(mktemp -d /tmp/agenthub-product-go-route.XXXXXXXXXX)"
SCRATCH="$(cd "${SCRATCH_INPUT}" && pwd -P)"
case "${SCRATCH}" in
  /tmp/agenthub-product-go-route.*) ;;
  *) echo "FAIL: unexpected scratch root" >&2; exit 1 ;;
esac

BUILD_LOG="${SCRATCH}/build.log"
GO_BUILD_LOG="${SCRATCH}/go-build.log"
RUN_LOG="${SCRATCH}/run.log"
MOCK_LOG="${SCRATCH}/mock.log"
EVIDENCE="${SCRATCH}/evidence.json"
ADAPTERD_BIN="${SCRATCH}/agenthub-adapterd"
MOCK_PID=""
PROBE_PID=""

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  if [[ -n "${PROBE_PID}" ]] && kill -0 "${PROBE_PID}" 2>/dev/null; then
    kill -TERM -- "-${PROBE_PID}" 2>/dev/null || true
    wait "${PROBE_PID}" 2>/dev/null || true
  fi
  if [[ -n "${MOCK_PID}" ]] && kill -0 "${MOCK_PID}" 2>/dev/null; then
    kill "${MOCK_PID}" 2>/dev/null || true
    wait "${MOCK_PID}" 2>/dev/null || true
  fi
  if [[ ${status} -eq 0 ]]; then
    rm -rf -- "${SCRATCH}"
  else
    echo "probe evidence retained at ${SCRATCH}" >&2
  fi
  exit "${status}"
}
trap cleanup EXIT INT TERM

python3 - <<'PY'
import socket
s = socket.socket()
try:
    s.bind(("127.0.0.1", 43121))
except OSError as exc:
    raise SystemExit(f"FAIL: saved Product port 43121 is unavailable: {exc}")
finally:
    s.close()
PY

UPSTREAM_PORT="$(python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
)"

python3 -u - "${UPSTREAM_PORT}" <<'PY' >"${MOCK_LOG}" 2>&1 &
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass
    def do_GET(self):
        raw = json.dumps({"object":"list","data":[{"id":"probe-model"}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)
ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
PY
MOCK_PID=$!

for _ in $(seq 1 100); do
  python3 - "${UPSTREAM_PORT}" <<'PY' && break || true
import socket, sys
try:
    with socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=.1):
        pass
except OSError:
    raise SystemExit(1)
PY
  sleep 0.05
done
kill -0 "${MOCK_PID}" 2>/dev/null || { echo "FAIL: loopback upstream did not start" >&2; exit 1; }

(cd "${ROOT}/go/agenthub-adapterd" && go build -trimpath -buildvcs=false -o "${ADAPTERD_BIN}" .) >"${GO_BUILD_LOG}" 2>&1
(cd "${ROOT}" && cargo build -p agenthub-gui --example go_route_product_e2e_probe \
  --features go-route-product-probe,go-route-tcp-control-probe --locked) >"${BUILD_LOG}" 2>&1
BIN="${ROOT}/target/debug/examples/go_route_product_e2e_probe"
[[ -x "${BIN}" && -x "${ADAPTERD_BIN}" ]] || { echo "FAIL: probe binaries missing" >&2; exit 1; }

setsid env \
  AGENTHUB_ADAPTERD_BIN="${ADAPTERD_BIN}" \
  AGENTHUB_GO_ROUTE_CONTROL_TRANSPORT=tcp \
  "${BIN}" "${SCRATCH}" "http://127.0.0.1:${UPSTREAM_PORT}/v1" >"${RUN_LOG}" 2>&1 &
PROBE_PID=$!
deadline=$((SECONDS + 90))
while kill -0 "${PROBE_PID}" 2>/dev/null; do
  if (( SECONDS >= deadline )); then
    echo "FAIL: Product-mode probe timed out" >&2
    exit 1
  fi
  sleep 0.1
done
if ! wait "${PROBE_PID}"; then
  PROBE_PID=""
  tail -100 "${RUN_LOG}" >&2 || true
  exit 1
fi
PROBE_PID=""

python3 - "${RUN_LOG}" "${EVIDENCE}" <<'PY'
import json, sys
run_log, evidence_path = sys.argv[1:]
with open(run_log, encoding="utf-8") as handle:
    rows = [line.strip() for line in handle if line.strip().startswith("{")]
assert rows, "probe emitted no evidence"
evidence = json.loads(rows[-1])
assert evidence["schema"] == "go-route-product-e2e-probe.v1", evidence
assert evidence["status"] == "ok", evidence
assert evidence["port"] == 43121, evidence
for key in (
    "saved_port_preserved", "reload_committed", "same_port_recovered",
    "product_home_preserved", "staging_cleaned", "port_released",
    "startup_control_secret_scan", "runtime_secret_scan", "data_dir_mode_unchanged",
):
    assert evidence[key] is True, (key, evidence)
assert evidence["restart_count"] >= 1, evidence
with open(evidence_path, "w", encoding="utf-8") as handle:
    json.dump(evidence, handle, sort_keys=True)
    handle.write("\n")
PY

if grep -F -q "sk-product-go-route-probe-do-not-use-000000" \
  "${RUN_LOG}" "${BUILD_LOG}" "${GO_BUILD_LOG}" "${MOCK_LOG}"; then
  echo "FAIL: synthetic upstream key leaked into probe logs" >&2
  exit 1
fi

cat "${EVIDENCE}"
echo "PASS: dormant Product-mode Go route real-process probe"
