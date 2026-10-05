#!/usr/bin/env bash
# Probe-only Rust -> prepared Product Go -> Rust trial. Linux only.

set -euo pipefail
umask 077

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
[[ $# -eq 0 ]] || { echo "FAIL: this probe accepts no arguments" >&2; exit 1; }
[[ "$(uname -s)" == "Linux" ]] || { echo "FAIL: this probe currently requires Linux" >&2; exit 1; }

SCRATCH_INPUT="$(mktemp -d /tmp/agenthub-product-handoff.XXXXXXXXXX)"
SCRATCH="$(cd "${SCRATCH_INPUT}" && pwd -P)"
case "${SCRATCH}" in
  /tmp/agenthub-product-handoff.*) ;;
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
RESPONSES_SOURCE_KEY="sk-product-handoff-responses-do-not-use-000000"
MESSAGES_SOURCE_KEY="sk-product-handoff-messages-do-not-use-000000"
CHAT_SOURCE_KEY="sk-product-handoff-chat-do-not-use-000000"
RESPONSES_FAILURE_REQUEST="handoff-responses-upstream-failure"
RESPONSES_FAILURE_RESPONSE="synthetic_upstream_failure"
CHAT_REQUEST="handoff-chat-sse-request"
CHAT_RESPONSE="handoff-chat-sse-ok"

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

RESPONSES_SOURCE_KEY = "sk-product-handoff-responses-do-not-use-000000"
MESSAGES_SOURCE_KEY = "sk-product-handoff-messages-do-not-use-000000"
CHAT_SOURCE_KEY = "sk-product-handoff-chat-do-not-use-000000"
RESPONSES_REQUEST = "handoff-responses-request"
RESPONSES_FAILURE_REQUEST = "handoff-responses-upstream-failure"
MESSAGES_REQUEST = "handoff-messages-request"
CHAT_REQUEST = "handoff-chat-sse-request"
CHAT_RESPONSE = "handoff-chat-sse-ok"

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

    def send_sse(self, frames):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()
        for frame in frames:
            self.wfile.write(frame)
        self.wfile.flush()

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

        if self.path == "/v1/chat/completions":
            model = payload.get("model") if isinstance(payload, dict) else None
            valid = (
                self.headers.get("Authorization") == "Bearer " + RESPONSES_SOURCE_KEY
                and not self.headers.get("X-API-Key")
                and not self.headers.get("Anthropic-Version")
                and isinstance(model, str) and bool(model)
                and payload == {
                    "model": model,
                    "messages": [{"role": "user", "content": RESPONSES_REQUEST}],
                    "stream": False,
                }
            )
            if valid:
                self.send_json(200, {
                    "id": "chatcmpl_handoff",
                    "object": "chat.completion",
                    "created": 1720000000,
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": "handoff-responses-ok"},
                        "finish_reason": "stop",
                    }],
                    "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
                })
                print(json.dumps({"event":"responses"}), flush=True)
                return

            failure_valid = (
                self.headers.get("Authorization") == "Bearer " + RESPONSES_SOURCE_KEY
                and not self.headers.get("X-API-Key")
                and not self.headers.get("Anthropic-Version")
                and isinstance(model, str) and bool(model)
                and payload == {
                    "model": model,
                    "messages": [{"role": "user", "content": RESPONSES_FAILURE_REQUEST}],
                    "stream": False,
                }
            )
            if failure_valid:
                self.send_json(502, {"error":{"code":"synthetic_upstream_failure"}})
                print(json.dumps({"event":"responses_failure"}), flush=True)
                return

            chat_valid = (
                self.headers.get("Authorization") == "Bearer " + CHAT_SOURCE_KEY
                and self.headers.get("Accept") == "text/event-stream"
                and not self.headers.get("X-API-Key")
                and not self.headers.get("Anthropic-Version")
                and isinstance(model, str) and bool(model)
                and payload == {
                    "model": model,
                    "messages": [{"role": "user", "content": CHAT_REQUEST}],
                    "stream": True,
                }
            )
            if chat_valid:
                first = json.dumps({
                    "id": "chatcmpl_handoff_stream",
                    "object": "chat.completion.chunk",
                    "created": 1720000000,
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "delta": {"role": "assistant", "content": CHAT_RESPONSE},
                        "finish_reason": None,
                    }],
                }, separators=(",", ":")).encode()
                second = json.dumps({
                    "id": "chatcmpl_handoff_stream",
                    "object": "chat.completion.chunk",
                    "created": 1720000000,
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "delta": {},
                        "finish_reason": "stop",
                    }],
                }, separators=(",", ":")).encode()
                self.send_sse((b"data: " + first + b"\n\n", b"data: " + second + b"\n\n", b"data: [DONE]\n\n"))
                print(json.dumps({"event":"chat_sse"}), flush=True)
                return

        if self.path == "/v1/messages":
            model = payload.get("model") if isinstance(payload, dict) else None
            valid = (
                self.headers.get("X-API-Key") == MESSAGES_SOURCE_KEY
                and self.headers.get("Anthropic-Version") == "2023-06-01"
                and not self.headers.get("Authorization")
                and isinstance(model, str) and bool(model)
                and payload == {
                    "model": model,
                    "max_tokens": 16,
                    "messages": [{"role": "user", "content": MESSAGES_REQUEST}],
                    "stream": False,
                }
            )
            if valid:
                self.send_json(200, {
                    "id": "msg_handoff",
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "text", "text": "handoff-messages-ok"}],
                    "model": model,
                    "stop_reason": "end_turn",
                    "stop_sequence": None,
                    "usage": {"input_tokens": 1, "output_tokens": 1},
                })
                print(json.dumps({"event":"messages"}), flush=True)
                return

        self.send_json(400, {"error":"mock_request_rejected"})
        print(json.dumps({"event":"rejected"}), flush=True)
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
(cd "${ROOT}" && cargo build -p agenthub-gui --example route_runtime_product_handoff_probe \
  --features route-runtime-product-handoff-probe,go-route-tcp-control-probe --locked) >"${BUILD_LOG}" 2>&1
BIN="${ROOT}/target/debug/examples/route_runtime_product_handoff_probe"
[[ -x "${BIN}" && -x "${ADAPTERD_BIN}" ]] || { echo "FAIL: probe binaries missing" >&2; exit 1; }

setsid env \
  AGENTHUB_ADAPTERD_BIN="${ADAPTERD_BIN}" \
  AGENTHUB_GO_ROUTE_CONTROL_TRANSPORT=tcp \
  "${BIN}" "${SCRATCH}" "http://127.0.0.1:${UPSTREAM_PORT}/v1" "${RUN_LOG}" >"${RUN_LOG}" 2>&1 &
PROBE_PID=$!
deadline=$((SECONDS + 120))
while kill -0 "${PROBE_PID}" 2>/dev/null; do
  if (( SECONDS >= deadline )); then
    echo "FAIL: Product handoff probe timed out" >&2
    exit 1
  fi
  sleep 0.1
done
if ! wait "${PROBE_PID}"; then
  PROBE_PID=""
  tail -120 "${RUN_LOG}" >&2 || true
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
assert evidence["schema"] == "route-runtime-product-handoff-probe.v1", evidence
assert evidence["status"] == "ok", evidence
assert evidence["port"] == 43121, evidence
assert evidence["rust_entry_count"] == 3, evidence
for key in (
    "detached_caller_drop_compensated", "health_failure_compensated",
    "product_request_failure_compensated",
    "prepared_hash_matched", "rust_stopped_before_go", "rust_mutator_blocked",
    "product_health_ready", "synthetic_protocol_requests_succeeded",
    "chat_completions_sse_succeeded",
    "go_stopped_before_restore", "rust_exact_restored",
    "database_and_selection_unchanged", "final_port_released", "secret_free_evidence",
):
    assert evidence[key] is True, (key, evidence)
with open(evidence_path, "w", encoding="utf-8") as handle:
    json.dump(evidence, handle, sort_keys=True)
    handle.write("\n")
PY

python3 - "${MOCK_LOG}" <<'PY'
import json, sys
events = []
with open(sys.argv[1], encoding="utf-8") as handle:
    for line in handle:
        line = line.strip()
        if line.startswith("{"):
            events.append(json.loads(line).get("event"))
assert events.count("responses") == 1, events
assert events.count("responses_failure") == 1, events
assert events.count("messages") == 1, events
assert events.count("chat_sse") == 1, events
assert "rejected" not in events, events
PY

if grep -F -q \
  -e "${RESPONSES_SOURCE_KEY}" \
  -e "${MESSAGES_SOURCE_KEY}" \
  -e "${CHAT_SOURCE_KEY}" \
  -e "ahb-product-handoff-wrong-bearer" \
  -e "handoff-responses-request" \
  -e "${RESPONSES_FAILURE_REQUEST}" \
  -e "${RESPONSES_FAILURE_RESPONSE}" \
  -e "handoff-messages-request" \
  -e "${CHAT_REQUEST}" \
  -e "handoff-responses-ok" \
  -e "handoff-messages-ok" \
  -e "${CHAT_RESPONSE}" \
  "${RUN_LOG}" "${BUILD_LOG}" "${GO_BUILD_LOG}" "${MOCK_LOG}"; then
  echo "FAIL: synthetic Key or request data leaked into probe logs" >&2
  exit 1
fi

cat "${EVIDENCE}"
echo "PASS: detached Rust/Product Go/Rust handoff trial"
