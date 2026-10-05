#!/usr/bin/env bash
# Real-process OAuth refresh control probe for the isolated Go route runtime.
# Uses only scratch paths and loopback ports; never touches the live gateway.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MOD="${ROOT}/go/agenthub-adapterd"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
SCRATCH="/tmp/agenthub-route-oauth-refresh/${RUN_ID}"
HOME_DIR="${SCRATCH}/home"
BIN="${SCRATCH}/bin/agenthub-adapterd"
SOCK="${HOME_DIR}/run/adapterd.sock"
PID_FILE="${HOME_DIR}/run/adapterd.pid"
LOG_FILE="${HOME_DIR}/logs/adapterd.log"
CONFIG_FIFO="${SCRATCH}/runtime-config.frames"
ADAPTERD_STDOUT="${SCRATCH}/adapterd.stdout.log"
UPSTREAM_LOG="${SCRATCH}/mock-upstream.log"
STATUS_LOG="${SCRATCH}/status.ndjson"
EVENT_LOG="${SCRATCH}/event.json"
CONTROL_LOG="${SCRATCH}/control.ndjson"
CLIENT_BODY="${SCRATCH}/client-response.json"
CLIENT_CODE="${SCRATCH}/client-response.code"
EVIDENCE="${SCRATCH}/evidence.json"
PRODUCT_PORT=43121

INGRESS_KEY="ahb_oauth_probe_ingress_synthetic"
OLD_TOKEN="oauth_probe_old_access_synthetic"
NEW_TOKEN="oauth_probe_new_access_synthetic"
MODEL="gpt-oauth-probe"
FORBIDDEN_MODEL="gpt-oauth-forbidden"
SOURCE_ID="oauth-probe-account"
EDGE_ID="oauth-probe-edge"
MEMBER_ID="account:${SOURCE_ID}"
OWNER_ID="oauth-probe-owner"

ADAPTERD_PID=""
UPSTREAM_PID=""
CLIENT_PID=""
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
for port in "${LISTEN_PORT}" "${UPSTREAM_PORT}"; do
  [[ "${port}" != "${PRODUCT_PORT}" ]] || { echo "FAIL: selected product port" >&2; exit 1; }
done
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
  for pid in "${CLIENT_PID}" "${ADAPTERD_PID}" "${UPSTREAM_PID}"; do
    if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
      kill "${pid}" 2>/dev/null || true
      wait "${pid}" 2>/dev/null || true
    fi
  done
  if [[ ${code} -ne 0 ]]; then
    echo "FAIL: scratch retained at ${SCRATCH}" >&2
  fi
}
trap cleanup EXIT

echo "== build and start real processes =="
(cd "${MOD}" && go build -o "${BIN}" .)
python3 - "${UPSTREAM_PORT}" "${OLD_TOKEN}" "${NEW_TOKEN}" "${FORBIDDEN_MODEL}" >"${UPSTREAM_LOG}" 2>&1 <<'PY' &
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

port, old_token, new_token, forbidden_model = sys.argv[1:]

class Handler(BaseHTTPRequestHandler):
    def log_message(self, _format, *_args):
        return

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        try:
            body = json.loads(self.rfile.read(length) or b"{}")
        except Exception:
            body = {}
        if body.get("model") == forbidden_model:
            self.send_response(403)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"error":{"message":"forbidden"}}')
            return
        auth = self.headers.get("Authorization", "")
        if auth == "Bearer " + old_token:
            self.send_response(401)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"error":{"message":"expired"}}')
            return
        if auth == "Bearer " + new_token:
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b'{"id":"oauth_refresh_ok","object":"response","status":"completed"}')
            return
        self.send_response(403)
        self.end_headers()

ThreadingHTTPServer(("127.0.0.1", int(port)), Handler).serve_forever()
PY
UPSTREAM_PID=$!

AGENTHUB_HOME="${HOME_DIR}" "${BIN}" run \
  --home "${HOME_DIR}" \
  --listen-port "${LISTEN_PORT}" \
  --runtime-config-stdin-stream \
  <"${CONFIG_FIFO}" >"${ADAPTERD_STDOUT}" 2>&1 &
ADAPTERD_PID=$!
exec {CONFIG_FD}>"${CONFIG_FIFO}"

send_config() {
  local token="$1"
  python3 - "${CONFIG_FD}" "${token}" "${INGRESS_KEY}" "${EDGE_ID}" "${MEMBER_ID}" \
    "${SOURCE_ID}" "${MODEL}" "${FORBIDDEN_MODEL}" "${UPSTREAM_PORT}" <<'PY'
import hashlib
import json
import os
import struct
import sys

fd, token, ingress, edge_id, member_id, source_id, model, forbidden_model, port = sys.argv[1:]
config = {
    "version": "route-config.v1-usage-spool",
    "edges": [{
        "id": edge_id,
        "ingress_key": ingress,
        "surface": "responses",
        "dialect": "codex",
        "schedule_policy": "priority_failover",
        "fixture_model": model,
        "members": [{
            "id": member_id,
            "source_kind": "account",
            "source_id": source_id,
            "refresh_kind": "codex_oauth",
            "upstream_base_url": f"http://127.0.0.1:{port}",
            "upstream_key": token,
            "upstream_auth": "bearer",
            "upstream_transport": "codex_responses",
            "priority": 0,
            "position": 0,
            "models": [model, forbidden_model],
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

INITIAL_HASH="$(send_config "${OLD_TOKEN}")"

for _ in $(seq 1 100); do
  [[ -S "${SOCK}" && -f "${PID_FILE}" ]] && break
  kill -0 "${ADAPTERD_PID}" 2>/dev/null || { sed -n '1,120p' "${ADAPTERD_STDOUT}" >&2; exit 1; }
  sleep 0.05
done
[[ -S "${SOCK}" && -f "${PID_FILE}" ]] || { echo "FAIL: adapterd startup timeout" >&2; exit 1; }

post_control() {
  curl -sS --unix-socket "${SOCK}" -H 'Content-Type: application/json' \
    --data-binary "$1" http://127.0.0.1/control
}

HS="$(post_control "$(python3 - "${HOME_DIR}" <<'PY'
import json, sys
home = sys.argv[1]
print(json.dumps({"type":"Handshake","request_id":"oauth-hs","app_data_dir":home,"payload":{
    "protocol_version":"route-runtime.v0-isolated","config_format_version":"route-config.v1-usage-spool",
    "package_version":"0.0.0-isolated","app_data_dir":home}}))
PY
)")"
read -r EPOCH CAPABILITY < <(printf '%s' "${HS}" | python3 -c 'import json,sys; p=json.load(sys.stdin)["payload"]; print(p["instance_epoch"], "control.oauth_refresh.v1" in p["capabilities"])')
[[ "${CAPABILITY}" == "True" ]] || { echo "FAIL: OAuth refresh capability missing" >&2; exit 1; }

ACQUIRE="$(post_control "$(python3 - "${HOME_DIR}" "${EPOCH}" "${OWNER_ID}" <<'PY'
import json, sys
home, epoch, owner = sys.argv[1:]
print(json.dumps({"type":"AcquireOrRenewOwner","request_id":"oauth-acquire","instance_epoch":epoch,
  "owner_id":owner,"app_data_dir":home,"payload":{"mode":"acquire","lease_budget_ms":120000}}))
PY
)")"
TERM="$(printf '%s' "${ACQUIRE}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["ok"],d; print(d["payload"]["owner_term"])')"

START="$(post_control "$(python3 - "${HOME_DIR}" "${EPOCH}" "${OWNER_ID}" "${TERM}" <<'PY'
import json, sys
home, epoch, owner, term = sys.argv[1:]
print(json.dumps({"type":"Start","request_id":"oauth-start","instance_epoch":epoch,"owner_id":owner,
  "owner_term":int(term),"app_data_dir":home,"payload":{}}))
PY
)")"
python3 - "${START}" "${LISTEN_PORT}" <<'PY'
import json, sys
d = json.loads(sys.argv[1]); assert d["ok"], d
assert d["payload"]["listen_ready"] is True, d
assert d["payload"]["port"] == int(sys.argv[2]), d
PY

status_request() {
  local request_id="$1"
  post_control "$(python3 - "${HOME_DIR}" "${EPOCH}" "${OWNER_ID}" "${TERM}" "${request_id}" <<'PY'
import json, sys
home, epoch, owner, term, request_id = sys.argv[1:]
print(json.dumps({"type":"Status","request_id":request_id,"instance_epoch":epoch,"owner_id":owner,
  "owner_term":int(term),"app_data_dir":home,"payload":{}}))
PY
)"
}

INITIAL_STATUS="$(status_request initial-status)"
printf '%s\n' "${INITIAL_STATUS}" >>"${STATUS_LOG}"
printf '%s' "${INITIAL_STATUS}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["ok"] and d["payload"]["active_hash"]==sys.argv[1]' "${INITIAL_HASH}"

echo "== old access receives 401 and requests refresh =="
(
  code="$(curl -sS --max-time 20 -o "${CLIENT_BODY}" -w '%{http_code}' \
    -H "Authorization: Bearer ${INGRESS_KEY}" -H 'Content-Type: application/json' \
    --data-binary "{\"model\":\"${MODEL}\",\"input\":\"probe\"}" \
    "http://127.0.0.1:${LISTEN_PORT}/v1/responses")"
  printf '%s\n' "${code}" >"${CLIENT_CODE}"
) &
CLIENT_PID=$!

NEXT="$(post_control "$(python3 - "${HOME_DIR}" "${EPOCH}" "${OWNER_ID}" "${TERM}" <<'PY'
import json, sys
home, epoch, owner, term = sys.argv[1:]
print(json.dumps({"type":"NextOAuthRefresh","request_id":"oauth-next","instance_epoch":epoch,
  "owner_id":owner,"owner_term":int(term),"app_data_dir":home,"payload":{"wait_ms":5000}}))
PY
)")"
printf '%s' "${NEXT}" >"${EVENT_LOG}"
python3 - "${NEXT}" "${EPOCH}" "${TERM}" "${INITIAL_HASH}" "${EDGE_ID}" "${MEMBER_ID}" "${SOURCE_ID}" <<'PY'
import json, sys
d = json.loads(sys.argv[1]); assert d["ok"], d
e = d["payload"]["event"]; assert e is not None, d
assert e["instance_epoch"] == sys.argv[2]
assert e["owner_term"] == int(sys.argv[3])
assert e["active_hash"] == sys.argv[4]
assert e["edge_id"] == sys.argv[5] and e["member_id"] == sys.argv[6]
assert e["source_kind"] == "account" and e["source_id"] == sys.argv[7]
assert e["refresh_kind"] == "codex_oauth"
PY

make_complete() {
  local epoch="$1" owner="$2" request_id="$3" applied_hash="$4"
  python3 - "${EVENT_LOG}" "${HOME_DIR}" "${epoch}" "${owner}" "${TERM}" "${request_id}" "${applied_hash}" <<'PY'
import json, sys
event_path, home, epoch, owner, term, request_id, applied_hash = sys.argv[1:]
event = json.load(open(event_path, encoding="utf-8"))["payload"]["event"]
print(json.dumps({"type":"CompleteOAuthRefresh","request_id":request_id,"instance_epoch":epoch,
  "owner_id":owner,"owner_term":int(term),"app_data_dir":home,"payload":{
    "refresh_id":event["refresh_id"],"active_hash":event["active_hash"],
    "applied_active_hash":applied_hash,"edge_id":event["edge_id"],"member_id":event["member_id"],
    "source_kind":event["source_kind"],"source_id":event["source_id"],
    "refresh_kind":event["refresh_kind"],"outcome":"config_applied"}}))
PY
}

WRONG_OWNER="$(post_control "$(make_complete "${EPOCH}" wrong-owner wrong-owner-complete pending-hash)")"
STALE_EPOCH="$(post_control "$(make_complete stale-epoch "${OWNER_ID}" stale-epoch-complete pending-hash)")"
printf '%s\n%s\n' "${WRONG_OWNER}" "${STALE_EPOCH}" >>"${CONTROL_LOG}"
printf '%s' "${WRONG_OWNER}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert not d["ok"] and d["error"]["code"]=="route.runtime.not_owner"'
printf '%s' "${STALE_EPOCH}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert not d["ok"] and d["error"]["code"]=="route.runtime.stale_epoch"'

echo "== swap config, acknowledge exact new hash, retry original request =="
UPDATED_HASH="$(send_config "${NEW_TOKEN}")"
UPDATED_STATUS=""
for index in $(seq 1 100); do
  UPDATED_STATUS="$(status_request "updated-status-${index}")"
  printf '%s\n' "${UPDATED_STATUS}" >>"${STATUS_LOG}"
  if printf '%s' "${UPDATED_STATUS}" | python3 -c 'import json,sys; d=json.load(sys.stdin); sys.exit(0 if d.get("ok") and d["payload"].get("active_hash")==sys.argv[1] else 1)' "${UPDATED_HASH}"; then
    break
  fi
  sleep 0.05
done
printf '%s' "${UPDATED_STATUS}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["ok"] and d["payload"]["active_hash"]==sys.argv[1] and d["payload"]["listen_ready"]' "${UPDATED_HASH}"

COMPLETE_BODY="$(make_complete "${EPOCH}" "${OWNER_ID}" oauth-complete "${UPDATED_HASH}")"
COMPLETE="$(post_control "${COMPLETE_BODY}")"
printf '%s\n' "${COMPLETE}" >>"${CONTROL_LOG}"
printf '%s' "${COMPLETE}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["ok"] and d["payload"]=={"completed":True,"retry_eligible":True},d'
wait "${CLIENT_PID}"
CLIENT_PID=""
[[ "$(tr -d '[:space:]' <"${CLIENT_CODE}")" == "200" ]] || { echo "FAIL: original request did not recover" >&2; exit 1; }
grep -F -q 'oauth_refresh_ok' "${CLIENT_BODY}" || { echo "FAIL: recovered response missing marker" >&2; exit 1; }

DUPLICATE="$(post_control "$(make_complete "${EPOCH}" "${OWNER_ID}" duplicate-complete "${UPDATED_HASH}")")"
printf '%s\n' "${DUPLICATE}" >>"${CONTROL_LOG}"
printf '%s' "${DUPLICATE}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert not d["ok"] and d["error"]["code"]=="route.runtime.oauth_refresh_mismatch"'

echo "== 403 does not request OAuth refresh =="
FORBIDDEN_CODE="$(curl -sS --max-time 10 -o "${SCRATCH}/forbidden-response.json" -w '%{http_code}' \
  -H "Authorization: Bearer ${INGRESS_KEY}" -H 'Content-Type: application/json' \
  --data-binary "{\"model\":\"${FORBIDDEN_MODEL}\"}" \
  "http://127.0.0.1:${LISTEN_PORT}/v1/responses")"
[[ "${FORBIDDEN_CODE}" == "403" ]] || { echo "FAIL: forbidden request returned ${FORBIDDEN_CODE}" >&2; exit 1; }
EMPTY_NEXT="$(post_control "$(python3 - "${HOME_DIR}" "${EPOCH}" "${OWNER_ID}" "${TERM}" <<'PY'
import json, sys
home, epoch, owner, term = sys.argv[1:]
print(json.dumps({"type":"NextOAuthRefresh","request_id":"oauth-next-empty","instance_epoch":epoch,
  "owner_id":owner,"owner_term":int(term),"app_data_dir":home,"payload":{"wait_ms":0}}))
PY
)")"
printf '%s\n' "${EMPTY_NEXT}" >>"${CONTROL_LOG}"
printf '%s' "${EMPTY_NEXT}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["ok"] and d["payload"]["event"] is None,d'

python3 - "${EVIDENCE}" "${INITIAL_HASH}" "${UPDATED_HASH}" "${LISTEN_PORT}" "${UPSTREAM_PORT}" <<'PY'
import json, sys
path, initial_hash, updated_hash, listen_port, upstream_port = sys.argv[1:]
with open(path, "w", encoding="utf-8") as handle:
    json.dump({
        "ok": True,
        "initial_hash": initial_hash,
        "updated_hash": updated_hash,
        "listen_port": int(listen_port),
        "upstream_port": int(upstream_port),
        "original_request_recovered": True,
        "wrong_owner_rejected": True,
        "stale_epoch_rejected": True,
        "duplicate_completion_rejected": True,
        "forbidden_did_not_refresh": True,
        "secret_scan": "passed",
    }, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY

echo "== secret scan =="
for secret in "${OLD_TOKEN}" "${NEW_TOKEN}" "${INGRESS_KEY}"; do
  if grep -F -l -- "${secret}" "${ADAPTERD_STDOUT}" "${LOG_FILE}" "${UPSTREAM_LOG}" \
    "${STATUS_LOG}" "${EVENT_LOG}" "${CONTROL_LOG}" "${CLIENT_BODY}" "${EVIDENCE}"; then
    echo "FAIL: login information or entry Key leaked into output" >&2
    exit 1
  fi
done

echo "PASS: OAuth refresh isolated real-process probe"
echo "evidence: ${EVIDENCE}"
