#!/usr/bin/env bash
# Real-process runtime-config stream probe.
# Sends length-framed initial, replacement, and rejected configs to one adapterd
# process while keeping its synthetic loopback listener on one fixed port.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MOD="${ROOT}/go/agenthub-adapterd"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
SCRATCH="/tmp/agenthub-route-runtime-config-stream/${RUN_ID}"
HOME_DIR="${SCRATCH}/home"
BIN="${SCRATCH}/bin/agenthub-adapterd"
SOCK="${HOME_DIR}/run/adapterd.sock"
PID_FILE="${HOME_DIR}/run/adapterd.pid"
LOG_FILE="${HOME_DIR}/logs/adapterd.log"
CONFIG_FIFO="${SCRATCH}/runtime-config.frames"
ADAPTERD_STDOUT="${SCRATCH}/adapterd.stdout.log"
MOCK_LOG="${SCRATCH}/mock-upstream.log"
STATUS_LOG="${SCRATCH}/status.ndjson"
EVIDENCE="${SCRATCH}/evidence.json"
PRODUCT_PORT=43121

INITIAL_INGRESS="ahb_stream_initial_ingress_synthetic"
UPDATED_INGRESS="ahb_stream_updated_ingress_synthetic"
INVALID_INGRESS="ahb_stream_invalid_ingress_must_not_log"
INITIAL_UPSTREAM="sk-member-header-messages-synthetic"
UPDATED_UPSTREAM="sk-member-hi-synthetic"
INVALID_UPSTREAM="sk-stream-invalid-upstream-must-not-log"
INITIAL_MODEL="claude-stream-initial"
UPDATED_MODEL="claude-stream-updated"

ADAPTERD_PID=""
MOCK_PID=""
CONFIG_FD=""

mkdir -p "${SCRATCH}/bin" "${HOME_DIR}/run" "${HOME_DIR}/logs"
mkfifo -m 600 "${CONFIG_FIFO}"

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
  echo "FAIL: selected product default port ${PRODUCT_PORT}" >&2
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

cleanup() {
  local code=$?
  if [[ -n "${CONFIG_FD}" ]]; then
    exec {CONFIG_FD}>&- || true
  fi
  if [[ -n "${ADAPTERD_PID}" ]] && kill -0 "${ADAPTERD_PID}" 2>/dev/null; then
    kill "${ADAPTERD_PID}" 2>/dev/null || true
    wait "${ADAPTERD_PID}" 2>/dev/null || true
  fi
  if [[ -n "${MOCK_PID}" ]] && kill -0 "${MOCK_PID}" 2>/dev/null; then
    kill "${MOCK_PID}" 2>/dev/null || true
    wait "${MOCK_PID}" 2>/dev/null || true
  fi
  if [[ ${code} -ne 0 ]]; then
    echo "FAIL: scratch retained at ${SCRATCH}" >&2
  fi
}
trap cleanup EXIT

echo "== build and start real processes =="
(cd "${MOD}" && go build -o "${BIN}" .)
"${BIN}" mock-upstream --listen "127.0.0.1:${UPSTREAM_PORT}" >"${MOCK_LOG}" 2>&1 &
MOCK_PID=$!
AGENTHUB_HOME="${HOME_DIR}" "${BIN}" run \
  --home "${HOME_DIR}" \
  --listen-port "${LISTEN_PORT}" \
  --runtime-config-stdin-stream \
  <"${CONFIG_FIFO}" >"${ADAPTERD_STDOUT}" 2>&1 &
ADAPTERD_PID=$!

# Keep one writer open for the lifetime of the process. Each Python invocation
# below writes exactly one big-endian uint32 length followed by one JSON value.
exec {CONFIG_FD}>"${CONFIG_FIFO}"

send_config_frame() {
  local kind="$1" ingress="$2" upstream_key="$3" model="$4"
  python3 - "${CONFIG_FD}" "${kind}" "${ingress}" "${upstream_key}" "${model}" "${UPSTREAM_PORT}" <<'PY'
import hashlib
import json
import os
import struct
import sys

fd, kind, ingress, upstream_key, model, port = sys.argv[1:]
config = {
    "version": "route-config.v0-isolated",
    "edges": [{
        "id": f"stream-{kind}",
        "ingress_key": ingress,
        "surface": "messages" if kind != "invalid" else "invalid-surface",
        "dialect": "claude",
        "schedule_policy": "priority_failover",
        "fixture_model": model,
        "members": [{
            "id": f"{kind}-member",
            "upstream_base_url": f"http://127.0.0.1:{port}/v1",
            "upstream_key": upstream_key,
            "upstream_auth": "x_api_key",
            "upstream_transport": "anthropic_messages",
            "priority": 0,
            "position": 0,
            "models": [model],
        }],
    }],
}
payload = json.dumps(config, separators=(",", ":")).encode()
frame = struct.pack(">I", len(payload)) + payload
offset = 0
while offset < len(frame):
    offset += os.write(int(fd), frame[offset:])
print(hashlib.sha256(payload).hexdigest())
PY
}

echo "== send initial full config =="
INITIAL_HASH="$(send_config_frame initial "${INITIAL_INGRESS}" "${INITIAL_UPSTREAM}" "${INITIAL_MODEL}")"

for _ in $(seq 1 100); do
  [[ -S "${SOCK}" && -f "${PID_FILE}" ]] && break
  if ! kill -0 "${ADAPTERD_PID}" 2>/dev/null; then
    echo "FAIL: adapterd exited before control socket became ready" >&2
    sed -n '1,120p' "${ADAPTERD_STDOUT}" >&2 || true
    exit 1
  fi
  sleep 0.05
done
[[ -S "${SOCK}" && -f "${PID_FILE}" ]] || { echo "FAIL: adapterd startup timeout" >&2; exit 1; }
[[ "$(tr -d '[:space:]' <"${PID_FILE}")" == "${ADAPTERD_PID}" ]] || {
  echo "FAIL: pid file does not identify started adapterd" >&2
  exit 1
}

post_control() {
  curl -sS --unix-socket "${SOCK}" -H 'Content-Type: application/json' \
    --data-binary "$1" http://127.0.0.1/control
}

HS="$(post_control "$(python3 - "${HOME_DIR}" <<'PY'
import json, sys
home = sys.argv[1]
print(json.dumps({"type":"Handshake", "request_id":"stream-hs", "app_data_dir":home,
  "payload":{"protocol_version":"route-runtime.v0-isolated",
  "config_format_version":"route-config.v0-isolated", "package_version":"0.0.0-isolated",
  "app_data_dir":home}}))
PY
)")"
read -r EPOCH INSTANCE_ID < <(printf '%s' "${HS}" | python3 -c 'import json,sys; p=json.load(sys.stdin)["payload"]; print(p["instance_epoch"], p["instance_id"])')

ACQ="$(post_control "$(python3 - "${HOME_DIR}" "${EPOCH}" <<'PY'
import json, sys
home, epoch = sys.argv[1:]
print(json.dumps({"type":"AcquireOrRenewOwner", "request_id":"stream-acquire",
  "instance_epoch":epoch, "owner_id":"stream-owner", "app_data_dir":home,
  "payload":{"mode":"acquire", "lease_budget_ms":120000}}))
PY
)")"
TERM="$(printf '%s' "${ACQ}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["ok"], d; print(d["payload"]["owner_term"])')"

START="$(post_control "$(python3 - "${HOME_DIR}" "${EPOCH}" "${TERM}" <<'PY'
import json, sys
home, epoch, term = sys.argv[1:]
print(json.dumps({"type":"Start", "request_id":"stream-start", "instance_epoch":epoch,
  "owner_id":"stream-owner", "owner_term":int(term), "app_data_dir":home, "payload":{}}))
PY
)")"
python3 - "${START}" "${LISTEN_PORT}" <<'PY'
import json, sys
d = json.loads(sys.argv[1])
assert d["ok"], d
assert d["payload"]["listen_ready"] is True, d
assert d["payload"]["port"] == int(sys.argv[2]), d
PY

get_status() {
  local label="$1"
  post_control "$(python3 - "${HOME_DIR}" "${EPOCH}" "${TERM}" "${label}-${RANDOM}" <<'PY'
import json, sys
home, epoch, term, request_id = sys.argv[1:]
print(json.dumps({"type":"Status", "request_id":request_id, "instance_epoch":epoch,
  "owner_id":"stream-owner", "owner_term":int(term), "app_data_dir":home, "payload":{}}))
PY
)"
}

assert_status() {
  local status="$1" revision="$2" hash="$3" last_error="$4"
  python3 - "${status}" "${revision}" "${hash}" "${LISTEN_PORT}" \
    "${INSTANCE_ID}" "${last_error}" <<'PY'
import json, sys
raw, revision, digest, port, instance_id, last_error = sys.argv[1:]
d = json.loads(raw)
assert d["ok"], d
p = d["payload"]
assert p["active_revision"] == revision, p
assert p["active_hash"] == digest, p
assert p["listen_ready"] is True, p
assert p["port"] == int(port), p
assert p["instance_id"] == instance_id, p
if last_error == "none":
    assert p["last_error"] is None, p
else:
    assert p["last_error"]["code"] == "route.runtime.config_format_mismatch", p
PY
}

request_messages() {
  local ingress="$1" model="$2" marker="$3" output="$4" code
  code="$(curl -sS -o "${output}" -w '%{http_code}' \
    -H "Authorization: Bearer ${ingress}" -H 'Content-Type: application/json' \
    --data-binary "{\"model\":\"${model}\",\"stream\":false}" \
    "http://127.0.0.1:${LISTEN_PORT}/v1/messages")"
  [[ "${code}" == "200" ]] || { echo "FAIL: Messages returned HTTP ${code}" >&2; exit 1; }
  grep -F -q -- "${marker}" "${output}" || { echo "FAIL: unexpected Messages response" >&2; exit 1; }
}

echo "== verify initial config =="
STATUS_INITIAL="$(get_status stream-status-initial)"
assert_status "${STATUS_INITIAL}" 1 "${INITIAL_HASH}" none
printf '%s\n' "${STATUS_INITIAL}" >>"${STATUS_LOG}"
request_messages "${INITIAL_INGRESS}" "${INITIAL_MODEL}" \
  isolated-header-messages-ok "${SCRATCH}/initial-response.json"

echo "== send and verify valid hot update =="
UPDATED_HASH="$(send_config_frame updated "${UPDATED_INGRESS}" "${UPDATED_UPSTREAM}" "${UPDATED_MODEL}")"
for _ in $(seq 1 100); do
  STATUS_UPDATED="$(get_status stream-status-updated)"
  if python3 - "${STATUS_UPDATED}" "${UPDATED_HASH}" <<'PY'
import json, sys
p = json.loads(sys.argv[1]).get("payload", {})
raise SystemExit(0 if p.get("active_revision") == "2" and p.get("active_hash") == sys.argv[2] else 1)
PY
  then
    break
  fi
  sleep 0.05
done
assert_status "${STATUS_UPDATED}" 2 "${UPDATED_HASH}" none
printf '%s\n' "${STATUS_UPDATED}" >>"${STATUS_LOG}"
request_messages "${UPDATED_INGRESS}" "${UPDATED_MODEL}" \
  isolated-member-hi "${SCRATCH}/updated-response.json"

echo "== send invalid update and verify last good config survives =="
INVALID_HASH="$(send_config_frame invalid "${INVALID_INGRESS}" "${INVALID_UPSTREAM}" invalid-stream-model)"
for _ in $(seq 1 100); do
  STATUS_REJECTED="$(get_status stream-status-rejected)"
  if python3 - "${STATUS_REJECTED}" <<'PY'
import json, sys
p = json.loads(sys.argv[1]).get("payload", {})
error = p.get("last_error") or {}
raise SystemExit(0 if error.get("code") == "route.runtime.config_format_mismatch" else 1)
PY
  then
    break
  fi
  sleep 0.05
done
assert_status "${STATUS_REJECTED}" 2 "${UPDATED_HASH}" rejected
printf '%s\n' "${STATUS_REJECTED}" >>"${STATUS_LOG}"
request_messages "${UPDATED_INGRESS}" "${UPDATED_MODEL}" \
  isolated-member-hi "${SCRATCH}/after-rejection-response.json"

kill -0 "${ADAPTERD_PID}"
[[ "$(tr -d '[:space:]' <"${PID_FILE}")" == "${ADAPTERD_PID}" ]] || {
  echo "FAIL: adapterd PID changed" >&2
  exit 1
}

for secret in "${INITIAL_INGRESS}" "${UPDATED_INGRESS}" "${INVALID_INGRESS}" \
  "${INITIAL_UPSTREAM}" "${UPDATED_UPSTREAM}" "${INVALID_UPSTREAM}"; do
  if grep -F -q -- "${secret}" "${LOG_FILE}" "${ADAPTERD_STDOUT}" "${MOCK_LOG}" "${STATUS_LOG}" 2>/dev/null; then
    echo "FAIL: API Key leaked to logs or Status" >&2
    exit 1
  fi
done

python3 - "${EVIDENCE}" "${ADAPTERD_PID}" "${LISTEN_PORT}" "${INITIAL_HASH}" \
  "${UPDATED_HASH}" "${INVALID_HASH}" <<'PY'
import json, sys
path, pid, port, initial_hash, updated_hash, invalid_hash = sys.argv[1:]
with open(path, "w", encoding="utf-8") as fh:
    json.dump({
        "runtime_config_stdin_stream": "ok",
        "pid": int(pid),
        "fixed_listen_port": int(port),
        "initial_revision": "1",
        "updated_revision": "2",
        "initial_hash": initial_hash,
        "updated_hash": updated_hash,
        "rejected_hash": invalid_hash,
        "rejected_config_became_active": False,
        "last_good_config_after_rejection": "ok",
        "api_key_scan": "ok",
    }, fh, indent=2)
    fh.write("\n")
PY

echo "PASS: runtime config stream hot update and rejection preserve one process and port"
echo "pid: ${ADAPTERD_PID}"
echo "listen port: ${LISTEN_PORT}"
echo "scratch: ${SCRATCH}"
echo "evidence: ${EVIDENCE}"
