#!/usr/bin/env bash
# Real-process HTTP safety probe for the isolated Go route runtime.
# Uses only synthetic secrets, a scratch home, ephemeral ports, and a malicious
# loopback fixture. It never touches the product port or the real ~/.agenthub.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MOD="${ROOT}/go/agenthub-adapterd"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
SCRATCH="/tmp/agenthub-route-http-safety/${RUN_ID}"
HOME_DIR="${SCRATCH}/home"
BIN="${SCRATCH}/bin/agenthub-adapterd"
SOCK="${HOME_DIR}/run/adapterd.sock"
ADAPTERD_STDOUT="${SCRATCH}/adapterd.stdout.log"
UPSTREAM_LOG="${SCRATCH}/upstream.log"
HITS="${SCRATCH}/upstream-hits.log"
RELEASE_STALL="${SCRATCH}/release-stall"
RELEASE_BODY_STALL="${RELEASE_STALL}-body"
RELEASE_SSE_IDLE="${RELEASE_STALL}-sse"
EVIDENCE="${SCRATCH}/evidence.json"
ENTRY_KEY="ahb_http_safety_probe_entry_synthetic"
UPSTREAM_KEY="probe-http-safety-upstream-key"
SECRET_MARKER="http-safety-upstream-secret-must-not-escape"
PRODUCT_PORT=43121
stall_pids=()

mkdir -p "${SCRATCH}/bin" "${HOME_DIR}/run" "${HOME_DIR}/logs" "${HOME_DIR}/config"

pick_port() {
  python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
}

LISTEN_PORT="$(pick_port)"
UPSTREAM_PORT="$(pick_port)"
if [[ "${LISTEN_PORT}" == "${PRODUCT_PORT}" || "${UPSTREAM_PORT}" == "${PRODUCT_PORT}" ]]; then
  echo "FAIL: selected product default port" >&2
  exit 1
fi
case "${HOME_DIR}" in
  "${SCRATCH}"/*) ;;
  *) echo "FAIL: scratch home escaped run root" >&2; exit 1 ;;
esac
if [[ "${HOME_DIR}" == "${HOME}/.agenthub" || "${HOME_DIR}" == "${HOME}/.agenthub/"* ]]; then
  echo "FAIL: refusing real ~/.agenthub" >&2
  exit 1
fi

(cd "${MOD}" && go build -o "${BIN}" .)

python3 -u - "${UPSTREAM_PORT}" "${HITS}" "${RELEASE_STALL}" "${SECRET_MARKER}" <<'PY' >"${UPSTREAM_LOG}" 2>&1 &
import json, os, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

port, hits, release, secret = int(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
release_body_stall = release + "-body"
release_sse_idle = release + "-sse"

class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *_):
        pass
    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length)
        try:
            model = json.loads(raw).get("model", "")
        except Exception:
            model = "invalid"
        with open(hits, "a", encoding="utf-8") as fh:
            fh.write(model + "\n")
        if self.path == "/redirect-target":
            with open(hits, "a", encoding="utf-8") as fh:
                fh.write("redirect-target-hit\n")
            self.send_response(204)
            self.end_headers()
            return
        if model == "error":
            body = secret.encode()
            self.send_response(400)
            self.send_header("Content-Type", "text/plain")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Retry-After", "120")
            self.send_header("Set-Cookie", "session=" + secret)
            self.send_header("Location", "http://127.0.0.1:%d/redirect-target" % port)
            self.send_header("WWW-Authenticate", "Bearer " + secret)
            self.end_headers()
            self.wfile.write(body)
            return
        if model == "redirect":
            body = secret.encode()
            self.send_response(307)
            self.send_header("Content-Type", "text/plain")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Location", "http://127.0.0.1:%d/redirect-target" % port)
            self.end_headers()
            self.wfile.write(body)
            return
        if model == "oversize":
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str((32 << 20) + 1))
            self.end_headers()
            try:
                self.wfile.write(("{" + secret).encode())
                self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError):
                pass
            return
        if model == "sse-wrong":
            body = json.dumps({"secret": secret}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if model == "sse-oversize":
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.send_header("Set-Cookie", "session=" + secret)
            self.end_headers()
            chunk = b"data: " + (b"x" * 4089) + b"\n"
            try:
                for _ in range(8193):
                    self.wfile.write(chunk)
                self.wfile.write(("data: " + secret + "\n\n").encode())
                self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError):
                pass
            return
        if model == "sse-idle":
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(b"data: first\n\n")
            self.wfile.flush()
            while not os.path.exists(release_sse_idle):
                time.sleep(0.02)
            return
        if model == "body-stall":
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", "11")
            self.end_headers()
            self.wfile.flush()
            while not os.path.exists(release_body_stall):
                time.sleep(0.02)
            try:
                self.wfile.write(b'{"ok":true}')
                self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError):
                pass
            return
        if model == "slow-drip":
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", "1000")
            self.end_headers()
            try:
                for byte in b"{" + (b" " * 999):
                    self.wfile.write(bytes((byte,)))
                    self.wfile.flush()
                    time.sleep(5)
            except (BrokenPipeError, ConnectionResetError):
                pass
            return
        if model == "stall":
            while not os.path.exists(release):
                time.sleep(0.02)
            body = b'{"ok":true}'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass
            return
        body = b'{"ok":true}'
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
server.daemon_threads = True
server.serve_forever()
PY
UPSTREAM_PID=$!

AGENTHUB_HOME="${HOME_DIR}" "${BIN}" run --home "${HOME_DIR}" --listen-port "${LISTEN_PORT}" --runtime-config-stdin \
  < <(python3 - <<PY
import json
models = ["ok", "error", "redirect", "oversize", "sse-wrong", "sse-oversize", "sse-idle", "body-stall", "slow-drip", "stall"]
print(json.dumps({"version":"route-config.v0-isolated","edges":[{
  "id":"http-safety","ingress_key":"${ENTRY_KEY}","surface":"messages","dialect":"claude",
  "schedule_policy":"priority_failover","fixture_model":"ok","members":[{
    "id":"malicious-loopback","upstream_base_url":"http://127.0.0.1:${UPSTREAM_PORT}/v1",
    "upstream_key":"${UPSTREAM_KEY}","upstream_auth":"x_api_key","upstream_transport":"anthropic_messages",
    "priority":0,"position":0,"models":models
  }]
}]}))
PY
  ) >"${ADAPTERD_STDOUT}" 2>&1 &
ADAPTERD_PID=$!

cleanup() {
  local code=$?
  touch "${RELEASE_STALL}" 2>/dev/null || true
  touch "${RELEASE_BODY_STALL}" "${RELEASE_SSE_IDLE}" 2>/dev/null || true
  for pid in "${stall_pids[@]}"; do
    if kill -0 "${pid}" 2>/dev/null; then
      kill "${pid}" 2>/dev/null || true
    fi
    wait "${pid}" 2>/dev/null || true
  done
  for pid in "${ADAPTERD_PID}" "${UPSTREAM_PID}"; do
    if kill -0 "${pid}" 2>/dev/null; then
      kill "${pid}" 2>/dev/null || true
      wait "${pid}" 2>/dev/null || true
    fi
  done
  if [[ ${code} -ne 0 ]]; then
    echo "FAIL: probe exited ${code}; scratch retained at ${SCRATCH}" >&2
  fi
}
trap cleanup EXIT

for _ in $(seq 1 100); do
  [[ -S "${SOCK}" ]] && break
  kill -0 "${ADAPTERD_PID}" 2>/dev/null || { cat "${ADAPTERD_STDOUT}" >&2; exit 1; }
  sleep 0.05
done
[[ -S "${SOCK}" ]] || { echo "FAIL: control socket timeout" >&2; exit 1; }

post_control() {
  curl -sS --unix-socket "${SOCK}" -H 'Content-Type: application/json' --data-binary "$1" http://127.0.0.1/control
}
json_get() {
  python3 -c 'import json,sys; d=json.load(sys.stdin); [None for key in []]; path=sys.argv[1].split("."); cur=d; exec("\n".join(["cur=cur[%r]" % key for key in path])); print(cur)' "$1"
}

HS="$(post_control "$(python3 - <<PY
import json
home="${HOME_DIR}"
print(json.dumps({"type":"Handshake","request_id":"safety-hs","app_data_dir":home,"payload":{"protocol_version":"route-runtime.v0-isolated","config_format_version":"route-config.v0-isolated","package_version":"0.0.0-isolated","app_data_dir":home}}))
PY
)")"
EPOCH="$(printf '%s' "${HS}" | json_get payload.instance_epoch)"
ACQ="$(post_control "$(python3 - <<PY
import json
print(json.dumps({"type":"AcquireOrRenewOwner","request_id":"safety-acq","instance_epoch":"${EPOCH}","owner_id":"safety-owner","app_data_dir":"${HOME_DIR}","payload":{"mode":"acquire","lease_budget_ms":300000}}))
PY
)")"
TERM="$(printf '%s' "${ACQ}" | json_get payload.owner_term)"
START="$(post_control "$(python3 - <<PY
import json
print(json.dumps({"type":"Start","request_id":"safety-start","instance_epoch":"${EPOCH}","owner_id":"safety-owner","owner_term":int("${TERM}"),"app_data_dir":"${HOME_DIR}","payload":{}}))
PY
)")"
printf '%s' "${START}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["ok"] and d["payload"]["listen_ready"], d'

request() {
  local model="$1" stream="$2" output="$3" headers="$4"
  curl -sS --max-time 45 -D "${headers}" -o "${output}" -w '%{http_code}' \
    -H "Authorization: Bearer ${ENTRY_KEY}" -H 'Content-Type: application/json' \
    --data-binary "{\"model\":\"${model}\",\"stream\":${stream}}" \
    "http://127.0.0.1:${LISTEN_PORT}/v1/messages"
}

expect_code() {
  local actual="$1" expected="$2" label="$3"
  if [[ "${actual}" != "${expected}" ]]; then
    echo "FAIL: ${label} returned HTTP ${actual}, expected ${expected}" >&2
    exit 1
  fi
}

code="$(request ok false "${SCRATCH}/ok.body" "${SCRATCH}/ok.headers")"
expect_code "${code}" 200 baseline
grep -F -q '"ok":true' "${SCRATCH}/ok.body"

python3 - "${SCRATCH}/request-boundary.body" "${SCRATCH}/request-oversize.body" <<'PY'
import sys
prefix = b'{"model":"ok","stream":false,"padding":"'
suffix = b'"}'
limit = 8 << 20
body = prefix + (b"x" * (limit - len(prefix) - len(suffix))) + suffix
assert len(body) == limit
with open(sys.argv[1], "wb") as fh:
    fh.write(body)
with open(sys.argv[2], "wb") as fh:
    fh.write(body + b" ")
PY

before_hits="$(wc -l <"${HITS}" 2>/dev/null || echo 0)"
code="$(curl -sS -v --http1.1 --max-time 45 -D "${SCRATCH}/request-boundary.headers" \
  -o "${SCRATCH}/request-boundary.response" -w '%{http_code}' \
  -H "Authorization: Bearer ${ENTRY_KEY}" -H 'Content-Type: application/json' \
  -H 'Transfer-Encoding: chunked' -H 'Expect:' --data-binary "@${SCRATCH}/request-boundary.body" \
  "http://127.0.0.1:${LISTEN_PORT}/v1/messages" 2>"${SCRATCH}/request-boundary.curl.log")"
expect_code "${code}" 200 request-boundary-chunked
grep -F -q '"ok":true' "${SCRATCH}/request-boundary.response"
grep -F -q '> Transfer-Encoding: chunked' "${SCRATCH}/request-boundary.curl.log"
if grep -F -q '> Content-Length:' "${SCRATCH}/request-boundary.curl.log"; then
  echo "FAIL: boundary request unexpectedly used Content-Length" >&2
  exit 1
fi
boundary_hits="$(wc -l <"${HITS}")"
(( boundary_hits == before_hits + 1 )) || { echo "FAIL: exact-limit request did not reach upstream exactly once" >&2; exit 1; }

code="$(curl -sS -o "${SCRATCH}/request-oversize.response" -w '%{http_code}' \
  -H "Authorization: Bearer ${ENTRY_KEY}" -H 'Content-Type: application/json' \
  -H 'Transfer-Encoding: chunked' -H 'Expect:' \
  --data-binary "@${SCRATCH}/request-oversize.body" "http://127.0.0.1:${LISTEN_PORT}/v1/messages")"
expect_code "${code}" 413 request-oversize
grep -F -q 'request_too_large' "${SCRATCH}/request-oversize.response"
after_hits="$(wc -l <"${HITS}" 2>/dev/null || echo 0)"
[[ "${boundary_hits}" == "${after_hits}" ]] || { echo "FAIL: oversize request reached upstream" >&2; exit 1; }

python3 - "${SCRATCH}/control-oversize.body" <<'PY'
import sys
with open(sys.argv[1], "wb") as fh:
    fh.write(b"x" * ((1 << 20) + 1))
PY
code="$(curl -sS --unix-socket "${SOCK}" -o "${SCRATCH}/control-oversize.response" -w '%{http_code}' \
  -H 'Content-Type: application/json' --data-binary "@${SCRATCH}/control-oversize.body" http://127.0.0.1/control)"
expect_code "${code}" 413 control-oversize

code="$(request error false "${SCRATCH}/error.body" "${SCRATCH}/error.headers")"
expect_code "${code}" 400 upstream-error
grep -F -q '"code":"upstream_error"' "${SCRATCH}/error.body"
grep -i -q '^Retry-After: 120' "${SCRATCH}/error.headers"
if grep -F -q "${SECRET_MARKER}" "${SCRATCH}/error.body" "${SCRATCH}/error.headers" ||
   grep -E -i -q '^(Set-Cookie|Location|WWW-Authenticate|Authorization|Connection):' "${SCRATCH}/error.headers"; then
  echo "FAIL: unsafe upstream error data escaped" >&2
  exit 1
fi

code="$(request redirect false "${SCRATCH}/redirect.body" "${SCRATCH}/redirect.headers")"
expect_code "${code}" 307 upstream-redirect
if grep -i -q '^Location:' "${SCRATCH}/redirect.headers" || grep -F -q "${SECRET_MARKER}" "${SCRATCH}/redirect.body"; then
  echo "FAIL: upstream redirect escaped" >&2
  exit 1
fi
grep -F -q 'redirect-target-hit' "${HITS}" && { echo "FAIL: gateway followed redirect" >&2; exit 1; }

code="$(request oversize false "${SCRATCH}/response-oversize.body" "${SCRATCH}/response-oversize.headers")"
expect_code "${code}" 502 response-oversize
grep -F -q '"code":"upstream_error"' "${SCRATCH}/response-oversize.body"
grep -F -q "${SECRET_MARKER}" "${SCRATCH}/response-oversize.body" && { echo "FAIL: oversize response leaked body" >&2; exit 1; }
sleep 2.1

code="$(request sse-wrong true "${SCRATCH}/sse-wrong.body" "${SCRATCH}/sse-wrong.headers")"
expect_code "${code}" 502 sse-content-type
grep -F -q '"code":"upstream_error"' "${SCRATCH}/sse-wrong.body"
grep -F -q "${SECRET_MARKER}" "${SCRATCH}/sse-wrong.body" && { echo "FAIL: wrong SSE type leaked body" >&2; exit 1; }
sleep 2.1

code="$(request sse-oversize true "${SCRATCH}/sse-oversize.body" "${SCRATCH}/sse-oversize.headers")"
expect_code "${code}" 200 sse-oversize
grep -F -q 'event: error' "${SCRATCH}/sse-oversize.body"
sse_bytes="$(wc -c <"${SCRATCH}/sse-oversize.body")"
(( sse_bytes <= (32 << 20) + 4096 )) || { echo "FAIL: SSE output exceeded bounded termination allowance" >&2; exit 1; }
grep -F -q "${SECRET_MARKER}" "${SCRATCH}/sse-oversize.body" && { echo "FAIL: SSE limit leaked tail" >&2; exit 1; }
sleep 2.1

started_at="$(date +%s)"
code="$(request body-stall false "${SCRATCH}/body-stall.body" "${SCRATCH}/body-stall.headers")"
body_stall_seconds=$(( $(date +%s) - started_at ))
expect_code "${code}" 502 non-stream-body-idle
grep -F -q '"code":"upstream_error"' "${SCRATCH}/body-stall.body"
if (( body_stall_seconds < 25 || body_stall_seconds > 45 )); then
  echo "FAIL: non-stream body idle returned outside the production timeout window (${body_stall_seconds}s)" >&2
  exit 1
fi
sleep 2.1

started_at="$(date +%s)"
code="$(request sse-idle true "${SCRATCH}/sse-idle.body" "${SCRATCH}/sse-idle.headers")"
sse_idle_seconds=$(( $(date +%s) - started_at ))
expect_code "${code}" 200 sse-idle
grep -F -q 'data: first' "${SCRATCH}/sse-idle.body"
grep -F -q 'event: error' "${SCRATCH}/sse-idle.body"
grep -F -q "${SECRET_MARKER}" "${SCRATCH}/sse-idle.body" && { echo "FAIL: SSE idle termination leaked data" >&2; exit 1; }
if (( sse_idle_seconds < 25 || sse_idle_seconds > 45 )); then
  echo "FAIL: SSE idle returned outside the production timeout window (${sse_idle_seconds}s)" >&2
  exit 1
fi
sleep 2.1

slow_drip_exercised=false
slow_drip_seconds=0
if [[ "${AGENTHUB_HTTP_SAFETY_LONG_PROBE:-0}" == "1" ]]; then
  started_at="$(date +%s)"
  code="$(curl -sS --max-time 150 -D "${SCRATCH}/slow-drip.headers" -o "${SCRATCH}/slow-drip.body" -w '%{http_code}' \
    -H "Authorization: Bearer ${ENTRY_KEY}" -H 'Content-Type: application/json' \
    --data-binary '{"model":"slow-drip","stream":false}' "http://127.0.0.1:${LISTEN_PORT}/v1/messages")"
  slow_drip_seconds=$(( $(date +%s) - started_at ))
  expect_code "${code}" 502 non-stream-total-timeout
  grep -F -q '"code":"upstream_error"' "${SCRATCH}/slow-drip.body"
  if (( slow_drip_seconds < 110 || slow_drip_seconds > 145 )); then
    echo "FAIL: slow-drip returned outside the production total-timeout window (${slow_drip_seconds}s)" >&2
    exit 1
  fi
  slow_drip_exercised=true
  sleep 2.1
fi

for i in $(seq 1 16); do
  curl -sS --max-time 30 -o "${SCRATCH}/stall-${i}.body" \
    -H "Authorization: Bearer ${ENTRY_KEY}" -H 'Content-Type: application/json' \
    --data-binary '{"model":"stall","stream":false}' "http://127.0.0.1:${LISTEN_PORT}/v1/messages" &
  stall_pids+=("$!")
done
for _ in $(seq 1 200); do
  stall_hits="$(grep -c '^stall$' "${HITS}" 2>/dev/null || true)"
  [[ "${stall_hits}" == "16" ]] && break
  sleep 0.05
done
[[ "${stall_hits:-0}" == "16" ]] || { echo "FAIL: did not fill all 16 route slots" >&2; exit 1; }
code="$(request stall false "${SCRATCH}/busy.body" "${SCRATCH}/busy.headers")"
expect_code "${code}" 503 concurrency-overflow
grep -F -q 'route_busy' "${SCRATCH}/busy.body"

cancelled_pid="${stall_pids[0]}"
kill "${cancelled_pid}"
wait "${cancelled_pid}" 2>/dev/null || true
cancel_reuse_code=""
for attempt in $(seq 1 100); do
  cancel_reuse_code="$(request ok false "${SCRATCH}/cancel-reuse.body" "${SCRATCH}/cancel-reuse.headers")"
  if [[ "${cancel_reuse_code}" == "200" ]]; then
    break
  fi
  if [[ "${cancel_reuse_code}" != "503" ]] || ! grep -F -q 'route_busy' "${SCRATCH}/cancel-reuse.body"; then
    echo "FAIL: cancellation slot retry returned HTTP ${cancel_reuse_code}" >&2
    exit 1
  fi
  sleep 0.05
done
expect_code "${cancel_reuse_code}" 200 cancellation-slot-reuse
grep -F -q '"ok":true' "${SCRATCH}/cancel-reuse.body"

touch "${RELEASE_STALL}"
for pid in "${stall_pids[@]}"; do
  [[ "${pid}" == "${cancelled_pid}" ]] && continue
  wait "${pid}"
done

code="$(request ok false "${SCRATCH}/busy-recovery.body" "${SCRATCH}/busy-recovery.headers")"
expect_code "${code}" 200 route-busy-slot-reuse
grep -F -q '"ok":true' "${SCRATCH}/busy-recovery.body"

for log in "${ADAPTERD_STDOUT}" "${HOME_DIR}/logs/adapterd.log"; do
  for secret in "${ENTRY_KEY}" "${UPSTREAM_KEY}" "${SECRET_MARKER}"; do
    if grep -F -q "${secret}" "${log}"; then
      echo "FAIL: adapterd log leaked synthetic secret" >&2
      exit 1
    fi
  done
done

python3 - "${EVIDENCE}" "${LISTEN_PORT}" "${sse_bytes}" "${body_stall_seconds}" "${sse_idle_seconds}" "${slow_drip_exercised}" "${slow_drip_seconds}" <<'PY'
import json, sys
path, port, sse_bytes = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
body_stall_seconds, sse_idle_seconds = int(sys.argv[4]), int(sys.argv[5])
slow_drip_exercised, slow_drip_seconds = sys.argv[6] == "true", int(sys.argv[7])
with open(path, "w", encoding="utf-8") as fh:
    json.dump({
        "ok": True,
        "probe": "http-safety-isolated",
        "listen_port": port,
        "request_chunked_8mib_accepted": True,
        "request_8mib_plus_one_rejected": True,
        "request_8mib_plus_one_did_not_reach_upstream": True,
        "control_1mib_plus_one_rejected": True,
        "response_32mib_plus_one_rejected": True,
        "non_stream_body_idle_rejected": True,
        "non_stream_body_idle_seconds": body_stall_seconds,
        "slow_drip_total_timeout_exercised": slow_drip_exercised,
        "slow_drip_total_timeout_rejected": slow_drip_exercised,
        "slow_drip_total_timeout_seconds": slow_drip_seconds,
        "upstream_error_and_redirect_sanitized": True,
        "sse_wrong_type_rejected": True,
        "sse_32mib_bounded": True,
        "sse_output_bytes": sse_bytes,
        "sse_idle_safely_terminated": True,
        "sse_idle_seconds": sse_idle_seconds,
        "concurrency_limit": 16,
        "concurrency_overflow_rejected": True,
        "slot_reused_after_cancellation": True,
        "slot_reused_after_route_busy": True,
        "adapter_logs_secret_free": True,
    }, fh, indent=2, sort_keys=True)
    fh.write("\n")
PY

echo "PASS: isolated HTTP safety probe"
echo "evidence: ${EVIDENCE}"
