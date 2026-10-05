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

SOURCE_KEY = "sk-product-go-route-probe-do-not-use-000000"
REQUEST_MARKER = "product-usage-request-body"
RESPONSE_MARKER = "product-usage-response-ok"
class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def send_json(self, status, payload):
        raw = json.dumps(payload, separators=(",", ":")).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_GET(self):
        if self.path != "/v1/models":
            self.send_json(404, {"error":"mock_request_rejected"})
            print(json.dumps({"event":"rejected"}), flush=True)
            return
        raw = json.dumps({"object":"list","data":[{"id":"probe-model"}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_POST(self):
        try:
            length = int(self.headers.get("Content-Length", "-1"))
            if length < 0 or length > 8192:
                raise ValueError
            payload = json.loads(self.rfile.read(length))
        except Exception:
            self.send_json(400, {"error":"mock_request_rejected"})
            print(json.dumps({"event":"rejected"}), flush=True)
            return
        model = payload.get("model") if isinstance(payload, dict) else None
        valid = (
            self.path == "/v1/chat/completions"
            and self.headers.get("Authorization") == "Bearer " + SOURCE_KEY
            and not self.headers.get("X-API-Key")
            and not self.headers.get("Anthropic-Version")
            and isinstance(model, str) and bool(model)
            and payload == {
                "model": model,
                "messages": [{"role":"user", "content":REQUEST_MARKER}],
                "stream": False,
            }
        )
        if not valid:
            self.send_json(400, {"error":"mock_request_rejected"})
            print(json.dumps({"event":"rejected"}), flush=True)
            return
        self.send_json(200, {
            "id": "chatcmpl_product_usage",
            "object": "chat.completion",
            "created": 1720000000,
            "model": model,
            "choices": [{
                "index": 0,
                "message": {"role":"assistant", "content":RESPONSE_MARKER},
                "finish_reason": "stop",
            }],
			"usage": {
				"prompt_tokens": 17,
				"completion_tokens": 11,
				"total_tokens": 28,
				"prompt_tokens_details": {"cached_tokens": 5},
				"completion_tokens_details": {"reasoning_tokens": 3},
			},
        })
        print(json.dumps({"event":"responses"}), flush=True)
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
    "startup_control_secret_scan", "runtime_secret_scan", "usage_jsonl_recorded",
    "usage_spool_and_logs_secret_scan", "usage_request_database_and_wal_unchanged", "data_dir_mode_unchanged",
):
    assert evidence[key] is True, (key, evidence)
assert evidence["restart_count"] >= 1, evidence
with open(evidence_path, "w", encoding="utf-8") as handle:
    json.dump(evidence, handle, sort_keys=True)
    handle.write("\n")
PY

python3 - "${MOCK_LOG}" "${RUN_LOG}" "${BUILD_LOG}" "${GO_BUILD_LOG}" \
  "http://127.0.0.1:${UPSTREAM_PORT}/v1" <<'PY'
import pathlib, sys

mock_log, *logs, upstream = sys.argv[1:]
events = [line.strip() for line in pathlib.Path(mock_log).read_text(encoding="utf-8").splitlines() if line.strip()]
assert events == ['{"event": "responses"}'], events
for path in logs + [mock_log]:
    contents = pathlib.Path(path).read_text(encoding="utf-8", errors="replace")
    for value in (
        "sk-product-go-route-probe-do-not-use-000000",
        "ahb-product-probe-wrong-bearer",
        "product-usage-request-body",
        "product-usage-response-ok",
        upstream,
    ):
        assert value not in contents, (path, "synthetic request data leaked into probe logs")
PY

cat "${EVIDENCE}"
echo "PASS: dormant Product-mode Go route real-process probe"
