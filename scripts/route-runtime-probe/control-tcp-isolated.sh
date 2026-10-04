#!/usr/bin/env bash
# Linux real-process probe for the authenticated TCP control transport.

set -euo pipefail
umask 077

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MOD="${ROOT}/go/agenthub-adapterd"
SCRATCH="$(mktemp -d /tmp/agenthub-route-control-tcp.XXXXXXXXXX)"
SCRATCH="$(realpath "${SCRATCH}")"
case "${SCRATCH}" in
  /tmp/agenthub-route-control-tcp.*) ;;
  *) echo "FAIL: mktemp escaped canonical /tmp root" >&2; exit 1 ;;
esac
[[ "$(stat -c '%a' "${SCRATCH}")" == "700" ]] || { echo "FAIL: scratch is not 0700" >&2; exit 1; }

HOME_DIR="${SCRATCH}/home"
BIN="${SCRATCH}/bin/agenthub-adapterd"
FIFO="${SCRATCH}/runtime-config.frames"
STDOUT_LOG="${SCRATCH}/adapterd.stdout.log"
RUNTIME_LOG="${HOME_DIR}/logs/adapterd.log"
MOCK_LOG="${SCRATCH}/mock-upstream.log"
STATUS_LOG="${SCRATCH}/status.ndjson"
EVIDENCE="${SCRATCH}/evidence.json"
PRODUCT_PORT=43121

INITIAL_INGRESS="ahb_tcp_initial_ingress_synthetic"
UPDATED_INGRESS="ahb_tcp_updated_ingress_synthetic"
UPSTREAM_KEY="sk-member-header-messages-synthetic"
INITIAL_MODEL="claude-tcp-initial"
UPDATED_MODEL="claude-tcp-updated"
OWNER_ID="tcp-probe-owner"
CONTROL_TOKEN="$(python3 - <<'PY'
import base64, os
print(base64.urlsafe_b64encode(os.urandom(32)).decode().rstrip("="))
PY
)"
WRONG_TOKEN="$(python3 - <<'PY'
import base64, os
print(base64.urlsafe_b64encode(os.urandom(32)).decode().rstrip("="))
PY
)"

ADAPTERD_PID=""
MOCK_PID=""
SLOW_PID=""
CONFIG_FD=""
CONTROL_ADDRESS=""

mkdir -p "${SCRATCH}/bin" "${HOME_DIR}/run" "${HOME_DIR}/logs"
mkfifo -m 600 "${FIFO}"

pick_port() {
  python3 - <<'PY'
import socket
s=socket.socket(socket.AF_INET, socket.SOCK_STREAM)
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
[[ "${HOME_DIR}" != "${HOME}/.agenthub" && "${HOME_DIR}" != "${HOME}/.agenthub/"* ]] || {
  echo "FAIL: refusing real ~/.agenthub" >&2
  exit 1
}

cleanup() {
  local code=$?
  if [[ -n "${CONFIG_FD}" ]]; then exec {CONFIG_FD}>&- || true; fi
  for pid in "${SLOW_PID}" "${ADAPTERD_PID}" "${MOCK_PID}"; do
    if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
      kill "${pid}" 2>/dev/null || true
      wait "${pid}" 2>/dev/null || true
    fi
  done
  if [[ ${code} -ne 0 ]]; then echo "FAIL: scratch retained at ${SCRATCH}" >&2; fi
}
trap cleanup EXIT

send_config() {
	local secret_input
	secret_input="$(printf '%s\n%s\n%s\n' "${CONFIG_INGRESS}" "${UPSTREAM_KEY}" "${CONFIG_MODEL}")"
  python3 - "${CONFIG_FD}" "${UPSTREAM_PORT}" 3<<<"${secret_input}" <<'PY'
import hashlib, json, os, struct, sys
fd, port = sys.argv[1:]
with os.fdopen(3, "r") as secret_fd:
    ingress, upstream_key, model = secret_fd.read().splitlines()
config={"version":"route-config.v0-isolated","edges":[{
 "id":"tcp-edge","ingress_key":ingress,"ingress_keys":[ingress],"surface":"messages",
 "dialect":"claude","schedule_policy":"priority_failover","fixture_model":model,
 "members":[{"id":"tcp-member","upstream_base_url":f"http://127.0.0.1:{port}/v1",
 "upstream_key":upstream_key,"upstream_auth":"x_api_key","upstream_transport":"anthropic_messages",
 "priority":0,"position":0,"models":[model]}]}]}
raw=json.dumps(config,separators=(",", ":")).encode()
frame=struct.pack(">I",len(raw))+raw
offset=0
while offset<len(frame): offset += os.write(int(fd),frame[offset:])
print(hashlib.sha256(raw).hexdigest())
PY
}

control_http() {
  local mode="$1" body="$2" secret="${CONTROL_TOKEN}"
  [[ "${mode}" != "wrong" ]] || secret="${WRONG_TOKEN}"
  python3 - "${CONTROL_ADDRESS}" "${mode}" "${body}" 3<<<"${secret}" <<'PY'
import http.client, os, sys
address,mode,body=sys.argv[1:]
host,port=address.rsplit(":",1)
token=os.fdopen(3,"r").read().strip()
conn=http.client.HTTPConnection(host,int(port),timeout=2)
conn.putrequest("POST","/control")
conn.putheader("Content-Type","application/json")
conn.putheader("Content-Length",str(len(body.encode())))
if mode in ("auth","wrong","duplicate"):
    conn.putheader("Authorization","Bearer "+token)
if mode == "duplicate":
    conn.putheader("Authorization","Bearer "+token)
conn.endheaders(body.encode())
response=conn.getresponse()
print(response.status)
sys.stdout.write(response.read().decode())
PY
}

post_control() {
  local result status body
  result="$(control_http auth "$1")"
  status="${result%%$'\n'*}"
  body="${result#*$'\n'}"
  [[ "${status}" == "200" ]] || { echo "FAIL: authorized control returned ${status}: ${body}" >&2; exit 1; }
  printf '%s' "${body}"
}

control_health() {
  python3 - "${CONTROL_ADDRESS}" 3<<<"${CONTROL_TOKEN}" <<'PY'
import http.client, os, sys
host,port=sys.argv[1].rsplit(":",1)
token=os.fdopen(3,"r").read().strip()
conn=http.client.HTTPConnection(host,int(port),timeout=1)
conn.request("GET","/healthz",headers={"Authorization":"Bearer "+token})
response=conn.getresponse(); response.read()
raise SystemExit(0 if response.status==200 else 1)
PY
}

scan_live_cmdlines() {
  local secrets
  secrets="$(printf '%s\n%s\n%s\n%s\n%s\n' "${CONTROL_TOKEN}" "${WRONG_TOKEN}" \
    "${INITIAL_INGRESS}" "${UPDATED_INGRESS}" "${UPSTREAM_KEY}")"
  python3 - 3<<<"${secrets}" <<'PY'
import glob, os
secrets=[line.encode() for line in os.fdopen(3,"r").read().splitlines() if line]
leaks=[]
for path in glob.glob("/proc/[0-9]*/cmdline"):
    try:
        raw=open(path,"rb").read()
    except (FileNotFoundError,PermissionError,ProcessLookupError):
        continue
    if any(secret in raw for secret in secrets): leaks.append(path.split("/")[2])
if leaks: raise SystemExit("secret present in process argv for pid(s): "+",".join(leaks))
PY
}

scan_secret_files() {
  local secrets
  secrets="$(printf '%s\n%s\n%s\n%s\n%s\n' "${CONTROL_TOKEN}" "${WRONG_TOKEN}" \
    "${INITIAL_INGRESS}" "${UPDATED_INGRESS}" "${UPSTREAM_KEY}")"
  python3 - "${STDOUT_LOG}" "${RUNTIME_LOG}" "${MOCK_LOG}" "${STATUS_LOG}" \
    "${FIXED_REJECT_LOG}" 3<<<"${secrets}" <<'PY'
import os, sys
secrets=[line.encode() for line in os.fdopen(3,"r").read().splitlines() if line]
for path in sys.argv[1:]:
    try: raw=open(path,"rb").read()
    except FileNotFoundError: continue
    if any(secret in raw for secret in secrets): raise SystemExit("secret leaked to "+path)
PY
}

echo "== build and let the child atomically select its control port =="
(cd "${MOD}" && go build -o "${BIN}" .)
FIXED_REJECT_LOG="${SCRATCH}/fixed-control-port-rejection.log"
if AGENTHUB_ADAPTERD_CONTROL_TOKEN="${CONTROL_TOKEN}" "${BIN}" run --home "${HOME_DIR}" \
  --control-listen "127.0.0.1:${UPSTREAM_PORT}" </dev/null >"${FIXED_REJECT_LOG}" 2>&1; then
  echo "FAIL: product CLI accepted a parent-selected TCP control port" >&2
  exit 1
fi
grep -F -q 'TCP control must let the child bind 127.0.0.1:0' "${FIXED_REJECT_LOG}" || {
  echo "FAIL: fixed TCP control port was not rejected at the CLI boundary" >&2
  exit 1
}
"${BIN}" mock-upstream --listen "127.0.0.1:${UPSTREAM_PORT}" >"${MOCK_LOG}" 2>&1 &
MOCK_PID=$!
AGENTHUB_ADAPTERD_CONTROL_TOKEN="${CONTROL_TOKEN}" AGENTHUB_HOME="${HOME_DIR}" \
  "${BIN}" run --home "${HOME_DIR}" --listen-port "${LISTEN_PORT}" \
  --control-listen 127.0.0.1:0 --runtime-config-stdin-stream \
  <"${FIFO}" >"${STDOUT_LOG}" 2>&1 &
ADAPTERD_PID=$!
exec {CONFIG_FD}>"${FIFO}"
CONFIG_INGRESS="${INITIAL_INGRESS}"
CONFIG_MODEL="${INITIAL_MODEL}"
INITIAL_HASH="$(send_config)"

for _ in $(seq 1 100); do
  line="$(grep -m1 '^agenthub-adapterd control listener: tcp4 ' "${STDOUT_LOG}" 2>/dev/null || true)"
  if [[ -n "${line}" ]]; then CONTROL_ADDRESS="${line##* }"; break; fi
  kill -0 "${ADAPTERD_PID}" 2>/dev/null || { sed -n '1,120p' "${STDOUT_LOG}" >&2; exit 1; }
  sleep 0.05
done
[[ "${CONTROL_ADDRESS}" =~ ^127\.0\.0\.1:[1-9][0-9]*$ ]] || { echo "FAIL: child did not report endpoint" >&2; exit 1; }
[[ "$(stat -c '%a' "${STDOUT_LOG}")" == "600" ]] || { echo "FAIL: stdout channel is not protected" >&2; exit 1; }
for _ in $(seq 1 100); do control_health >/dev/null 2>&1 && break; sleep 0.02; done
control_health

UNAUTH_HS="$(python3 - "${HOME_DIR}" <<'PY'
import json,sys
home=sys.argv[1]
print(json.dumps({"type":"Handshake","request_id":"tcp-auth-guard","app_data_dir":home,"payload":{
 "protocol_version":"route-runtime.v0-isolated","config_format_version":"route-config.v0-isolated",
 "package_version":"0.0.0-isolated","app_data_dir":home}}))
PY
)"

echo "== reject a valid Handshake before body processing or state changes =="
for mode in missing wrong duplicate; do
  result="$(control_http "${mode}" "${UNAUTH_HS}")"
  status="${result%%$'\n'*}"; body="${result#*$'\n'}"
  [[ "${status}" == "401" && "${body}" == "unauthorized" ]] || {
    echo "FAIL: ${mode} auth returned ${status}: ${body}" >&2; exit 1
  }
done

AUTHORIZED_HS="$(python3 - "${HOME_DIR}" <<'PY'
import json,sys
home=sys.argv[1]
# Same request ID but a different, still-valid payload proves rejected requests were not cached.
print(json.dumps({"type":"Handshake","request_id":"tcp-auth-guard","app_data_dir":home,"payload":{
 "protocol_version":"route-runtime.v0-isolated","app_data_dir":home}}))
PY
)"
HS="$(post_control "${AUTHORIZED_HS}")"
read -r EPOCH INSTANCE_ID < <(printf '%s' "${HS}" | python3 -c 'import json,sys; p=json.load(sys.stdin)["payload"]; assert "control.http_loopback.auth.v1" in p["capabilities"]; print(p["instance_epoch"],p["instance_id"])')

ACQUIRE_BODY="$(python3 - "${HOME_DIR}" "${EPOCH}" "${OWNER_ID}" <<'PY'
import json,sys
home,epoch,owner=sys.argv[1:]
print(json.dumps({"type":"AcquireOrRenewOwner","request_id":"tcp-acquire","instance_epoch":epoch,
 "owner_id":owner,"app_data_dir":home,"payload":{"mode":"acquire","lease_budget_ms":120000}}))
PY
)"
ACQUIRE="$(post_control "${ACQUIRE_BODY}")"
TERM="$(printf '%s' "${ACQUIRE}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["ok"],d; print(d["payload"]["owner_term"])')"

control_envelope() {
  python3 - "$1" "$2" "${HOME_DIR}" "${EPOCH}" "${OWNER_ID}" "${TERM}" <<'PY'
import json,sys
kind,rid,home,epoch,owner,term=sys.argv[1:]
print(json.dumps({"type":kind,"request_id":rid,"instance_epoch":epoch,"owner_id":owner,
 "owner_term":int(term),"app_data_dir":home,"payload":{}}))
PY
}

START="$(post_control "$(control_envelope Start tcp-start)")"
python3 - "${START}" "${LISTEN_PORT}" <<'PY'
import json,sys
d=json.loads(sys.argv[1]); assert d["ok"],d
assert d["payload"]["listen_ready"] and d["payload"]["port"]==int(sys.argv[2]),d
PY

get_status() { post_control "$(control_envelope Status "tcp-status-$1-${RANDOM}")"; }
STATUS_INITIAL="$(get_status initial)"
printf '%s\n' "${STATUS_INITIAL}" >>"${STATUS_LOG}"
python3 - "${STATUS_INITIAL}" "${INITIAL_HASH}" <<'PY'
import json,sys
p=json.loads(sys.argv[1])["payload"]
assert p["active_revision"]=="1" and p["active_hash"]==sys.argv[2] and p["last_error"] is None,p
PY

route_request() {
	python3 - "127.0.0.1:${LISTEN_PORT}" "${ROUTE_MODEL}" "${ROUTE_EXPECTED}" \
		3<<<"${ROUTE_INGRESS}" <<'PY' >"${ROUTE_OUTPUT}"
import http.client,json,os,sys
address,model,expected=sys.argv[1:]
host,port=address.rsplit(":",1); key=os.fdopen(3,"r").read().strip()
conn=http.client.HTTPConnection(host,int(port),timeout=2)
body=json.dumps({"model":model,"stream":False})
conn.request("POST","/v1/messages",body=body,headers={"Authorization":"Bearer "+key,"Content-Type":"application/json"})
response=conn.getresponse(); raw=response.read()
if response.status!=int(expected): raise SystemExit(f"route status {response.status}, expected {expected}")
sys.stdout.buffer.write(raw)
PY
}
ROUTE_INGRESS="${INITIAL_INGRESS}"
ROUTE_MODEL="${INITIAL_MODEL}"
ROUTE_EXPECTED=200
ROUTE_OUTPUT="${SCRATCH}/initial-route.json"
route_request
grep -F -q 'isolated-header-messages-ok' "${SCRATCH}/initial-route.json"

echo "== hot reload in the same process and fixed route port =="
CONFIG_INGRESS="${UPDATED_INGRESS}"
CONFIG_MODEL="${UPDATED_MODEL}"
UPDATED_HASH="$(send_config)"
for _ in $(seq 1 100); do
  STATUS_UPDATED="$(get_status updated)"
  if python3 - "${STATUS_UPDATED}" "${UPDATED_HASH}" <<'PY'
import json,sys
p=json.loads(sys.argv[1]).get("payload",{})
raise SystemExit(0 if p.get("active_revision")=="2" and p.get("active_hash")==sys.argv[2] else 1)
PY
  then break; fi
  sleep 0.05
done
python3 - "${STATUS_UPDATED}" "${UPDATED_HASH}" "${INSTANCE_ID}" "${LISTEN_PORT}" <<'PY'
import json,sys
p=json.loads(sys.argv[1])["payload"]
assert p["active_revision"]=="2" and p["active_hash"]==sys.argv[2],p
assert p["instance_id"]==sys.argv[3] and p["port"]==int(sys.argv[4]),p
PY
printf '%s\n' "${STATUS_UPDATED}" >>"${STATUS_LOG}"
ROUTE_INGRESS="${UPDATED_INGRESS}"
ROUTE_MODEL="${UPDATED_MODEL}"
ROUTE_EXPECTED=200
ROUTE_OUTPUT="${SCRATCH}/updated-route.json"
route_request
grep -F -q 'isolated-header-messages-ok' "${SCRATCH}/updated-route.json"
ROUTE_INGRESS="${INITIAL_INGRESS}"
ROUTE_MODEL="${INITIAL_MODEL}"
ROUTE_EXPECTED=401
ROUTE_OUTPUT="${SCRATCH}/retired-route.json"
route_request

echo "== enforce the slow-connection ceiling and recover a slot =="
SLOW_READY="${SCRATCH}/slow-connections.ready"
python3 - "${CONTROL_ADDRESS}" "${SLOW_READY}" 8 <<'PY' &
import socket,sys,time
host,port=sys.argv[1].rsplit(":",1); sockets=[]
for _ in range(int(sys.argv[3])):
    sock=socket.create_connection((host,int(port)),timeout=2)
    sock.sendall(b"GET /healthz HTTP/1.1\r\nHost: 127.0.0.1\r\n")
    sockets.append(sock)
open(sys.argv[2],"w").write("ready\n")
time.sleep(30)
PY
SLOW_PID=$!
for _ in $(seq 1 100); do [[ -f "${SLOW_READY}" ]] && break; sleep 0.02; done
[[ -f "${SLOW_READY}" ]] || { echo "FAIL: slow clients did not connect" >&2; exit 1; }
sleep 0.2
if control_health >/dev/null 2>&1; then echo "FAIL: connection ceiling was not enforced" >&2; exit 1; fi
kill "${SLOW_PID}"; wait "${SLOW_PID}" 2>/dev/null || true; SLOW_PID=""
for _ in $(seq 1 100); do control_health >/dev/null 2>&1 && break; sleep 0.02; done
control_health

kill -0 "${ADAPTERD_PID}"
[[ "$(tr -d '[:space:]' <"${HOME_DIR}/run/adapterd.pid")" == "${ADAPTERD_PID}" ]]
scan_live_cmdlines

echo "== stop and verify child-selected control and route ports are released =="
STOP="$(post_control "$(control_envelope Stop tcp-stop)")"
python3 - "${STOP}" <<'PY'
import json,sys
d=json.loads(sys.argv[1]); assert d["ok"] and d["payload"]["lifecycle"]=="draining",d
PY
for _ in $(seq 1 160); do kill -0 "${ADAPTERD_PID}" 2>/dev/null || break; sleep 0.05; done
if kill -0 "${ADAPTERD_PID}" 2>/dev/null; then echo "FAIL: adapterd did not stop" >&2; exit 1; fi
wait "${ADAPTERD_PID}"; ADAPTERD_PID=""
CONTROL_PORT="${CONTROL_ADDRESS##*:}"
python3 - "${CONTROL_PORT}" "${LISTEN_PORT}" <<'PY'
import socket,sys
for port in map(int,sys.argv[1:]):
    sock=socket.socket(); sock.bind(("127.0.0.1",port)); sock.close()
PY
scan_secret_files

python3 - "${EVIDENCE}" "${INSTANCE_ID}" "${CONTROL_PORT}" "${LISTEN_PORT}" <<'PY'
import json,sys
path,instance,control,route=sys.argv[1:]
with open(path,"w",encoding="utf-8") as f:
 json.dump({"linux_authenticated_tcp_control":"ok","child_atomic_ephemeral_bind":"ok",
  "auth_rejected_before_handshake_state":"ok","handshake_owner_start_status":"ok",
  "route_and_hot_reload":"ok","same_instance":instance,"control_port":int(control),
  "route_port":int(route),"connection_ceiling":"ok","stop_and_port_release":"ok",
  "live_cmdline_and_file_secret_scan":"ok",
  "control_token_delivery":"env_has_same_uid_residual_risk_until_inherited_fd_or_handle",
  "windows_execution":"not_run_cross_compile_only"},f,indent=2)
 f.write("\n")
PY

echo "PASS: Linux authenticated TCP control with child-selected port and bounded connections"
echo "scratch: ${SCRATCH}"
echo "evidence: ${EVIDENCE}"
