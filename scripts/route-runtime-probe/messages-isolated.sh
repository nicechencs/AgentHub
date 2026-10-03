#!/usr/bin/env bash
# Isolated Go adapterd Messages probe.
# Handshake, Status, process up/down, one synthetic-key Messages JSON (+ SSE).
# Never touches ~/.agenthub or the live/default gateway.
#
# Usage (from repository root):
#   scripts/route-runtime-probe/messages-isolated.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MOD="$ROOT/go/agenthub-adapterd"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
SCRATCH="/tmp/agenthub-route-runtime-probe/${RUN_ID}"
HOME_DIR="${SCRATCH}/home"
BIN="${SCRATCH}/bin/agenthub-adapterd"
SOCK="${HOME_DIR}/run/adapterd.sock"
PID_FILE="${HOME_DIR}/run/adapterd.pid"
LOG_FILE="${HOME_DIR}/logs/adapterd.log"
FIXTURE="${HOME_DIR}/config/probe.json"
EVIDENCE="${SCRATCH}/evidence.json"
MOCK_LOG="${SCRATCH}/mock-upstream.log"
ADAPTERD_LOG_STDOUT="${SCRATCH}/adapterd.stdout.log"
SYNTHETIC_KEY="ahb_probe_isolated_synthetic_not_a_real_login"
KEY_TAIL="${SYNTHETIC_KEY: -4}"
REAL_HOME="${HOME}/.agenthub"
PRODUCT_PORT=43121

mkdir -p "${SCRATCH}/bin" "${HOME_DIR}/config" "${HOME_DIR}/run" "${HOME_DIR}/logs"

pick_port() {
  python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
port = s.getsockname()[1]
s.close()
print(port)
PY
}

MESSAGES_PORT="$(pick_port)"
UPSTREAM_PORT="$(pick_port)"
if [[ "${MESSAGES_PORT}" == "${PRODUCT_PORT}" || "${UPSTREAM_PORT}" == "${PRODUCT_PORT}" ]]; then
  echo "refusing product default port ${PRODUCT_PORT}" >&2
  exit 1
fi

echo "== path inventory (before start) =="
PATHS=(
  "${SCRATCH}"
  "${HOME_DIR}"
  "${HOME_DIR}/config"
  "${HOME_DIR}/run"
  "${HOME_DIR}/logs"
  "${FIXTURE}"
  "${SOCK}"
  "${PID_FILE}"
  "${LOG_FILE}"
  "${BIN}"
  "${EVIDENCE}"
  "${MOCK_LOG}"
  "${ADAPTERD_LOG_STDOUT}"
)
printf 'AGENTHUB_HOME=%s\n' "${HOME_DIR}"
for p in "${PATHS[@]}"; do
  printf '  %s\n' "${p}"
  case "${p}" in
    "${SCRATCH}"|"${SCRATCH}"/*) ;;
    *)
      echo "FAIL: path is not under scratch ${SCRATCH}: ${p}" >&2
      exit 1
      ;;
  esac
  if [[ "${p}" == "${REAL_HOME}" || "${p}" == "${REAL_HOME}"/* ]]; then
    echo "FAIL: path is real ~/.agenthub: ${p}" >&2
    exit 1
  fi
done
if [[ "${HOME_DIR}" == "${REAL_HOME}" || "${HOME_DIR}" == "${REAL_HOME}"/* ]]; then
  echo "FAIL: AGENTHUB_HOME is real ~/.agenthub" >&2
  exit 1
fi
echo "scratch check: all listed paths are under ${SCRATCH}"
echo "real ~/.agenthub untouched: ${REAL_HOME}"

python3 - "${FIXTURE}" "${UPSTREAM_PORT}" "${SYNTHETIC_KEY}" <<'PY'
import json, sys
path, port, key = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, "w", encoding="utf-8") as fh:
    json.dump({
        "ingress_key": key,
        "upstream_base_url": f"http://127.0.0.1:{port}",
        "fixture_model": "claude-probe-fixture",
    }, fh)
    fh.write("\n")
PY
chmod 600 "${FIXTURE}"

echo "== build =="
echo "+ go build -o ${BIN} ."
( cd "${MOD}" && go build -o "${BIN}" . )

echo "== start mock upstream =="
echo "+ ${BIN} mock-upstream --listen 127.0.0.1:${UPSTREAM_PORT}"
"${BIN}" mock-upstream --listen "127.0.0.1:${UPSTREAM_PORT}" >"${MOCK_LOG}" 2>&1 &
MOCK_PID=$!

echo "== start adapterd =="
echo "+ AGENTHUB_HOME=${HOME_DIR} ${BIN} run --home ${HOME_DIR} --listen-port ${MESSAGES_PORT}"
AGENTHUB_HOME="${HOME_DIR}" "${BIN}" run --home "${HOME_DIR}" --listen-port "${MESSAGES_PORT}" \
  >"${ADAPTERD_LOG_STDOUT}" 2>&1 &
ADAPTERD_PID=$!

cleanup() {
  local code=$?
  if kill -0 "${ADAPTERD_PID}" 2>/dev/null; then
    kill "${ADAPTERD_PID}" 2>/dev/null || true
    wait "${ADAPTERD_PID}" 2>/dev/null || true
  fi
  if kill -0 "${MOCK_PID}" 2>/dev/null; then
    kill "${MOCK_PID}" 2>/dev/null || true
    wait "${MOCK_PID}" 2>/dev/null || true
  fi
  if [[ ${code} -ne 0 ]]; then
    echo "FAIL: probe exited ${code}; scratch left at ${SCRATCH}" >&2
  fi
}
trap cleanup EXIT

wait_for_file() {
  local path="$1"
  local i
  for i in $(seq 1 50); do
    if [[ -S "${path}" || -f "${path}" ]]; then
      return 0
    fi
    sleep 0.1
  done
  echo "FAIL: timed out waiting for ${path}" >&2
  echo "---- adapterd stdout ----" >&2
  cat "${ADAPTERD_LOG_STDOUT}" >&2 || true
  echo "---- mock log ----" >&2
  cat "${MOCK_LOG}" >&2 || true
  return 1
}

wait_for_file "${SOCK}"
wait_for_file "${PID_FILE}"

if ! kill -0 "${ADAPTERD_PID}" 2>/dev/null; then
  echo "FAIL: adapterd is not running" >&2
  cat "${ADAPTERD_LOG_STDOUT}" >&2 || true
  exit 1
fi
echo "process up: pid=${ADAPTERD_PID}"

post_control() {
  local body="$1"
  curl -sS --unix-socket "${SOCK}" \
    -H 'Content-Type: application/json' \
    --data-binary "${body}" \
    http://127.0.0.1/control
}

python_get() {
  python3 -c 'import json,sys; data=json.load(sys.stdin); path=sys.argv[1].split(".");
cur=data
for p in path:
    if p.isdigit():
        cur=cur[int(p)]
    else:
        cur=cur[p]
if cur is None:
    print("")
elif isinstance(cur, bool):
    print("true" if cur else "false")
else:
    print(cur)' "$1"
}

echo "== Handshake =="
HS_BODY="$(python3 - "${HOME_DIR}" <<'PY'
import json, sys
home = sys.argv[1]
print(json.dumps({
    "type": "Handshake",
    "request_id": "probe-hs-1",
    "app_data_dir": home,
    "payload": {
        "protocol_version": "route-runtime.v0-isolated",
        "config_format_version": "route-config.v0-isolated",
        "package_version": "0.0.0-isolated",
        "app_data_dir": home,
    },
}))
PY
)"
HS_REPLY="$(post_control "${HS_BODY}")"
echo "${HS_REPLY}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("ok"), d; print("handshake ok")'
EPOCH="$(printf '%s' "${HS_REPLY}" | python_get payload.instance_epoch)"
INSTANCE="$(printf '%s' "${HS_REPLY}" | python_get payload.instance_id)"
if [[ -z "${EPOCH}" || -z "${INSTANCE}" ]]; then
  echo "FAIL: handshake missing instance identity: ${HS_REPLY}" >&2
  exit 1
fi

echo "== Status (process up, not serving) =="
ST_BODY="$(python3 - "${HOME_DIR}" "${EPOCH}" <<'PY'
import json, sys
home, epoch = sys.argv[1], sys.argv[2]
print(json.dumps({
    "type": "Status",
    "request_id": "probe-st-1",
    "instance_epoch": epoch,
    "app_data_dir": home,
    "payload": {},
}))
PY
)"
ST_REPLY="$(post_control "${ST_BODY}")"
echo "${ST_REPLY}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("ok"), d; p=d["payload"]; assert p.get("listen_ready") is False; print("status ok lifecycle="+str(p.get("lifecycle")))'
if printf '%s' "${ST_REPLY}" | grep -F -q "${SYNTHETIC_KEY}"; then
  echo "FAIL: status leaked synthetic key" >&2
  exit 1
fi

echo "== AcquireOrRenewOwner acquire =="
ACQ_BODY="$(python3 - "${HOME_DIR}" "${EPOCH}" <<'PY'
import json, sys
home, epoch = sys.argv[1], sys.argv[2]
print(json.dumps({
    "type": "AcquireOrRenewOwner",
    "request_id": "probe-acq-1",
    "instance_epoch": epoch,
    "owner_id": "probe-owner",
    "app_data_dir": home,
    "payload": {"mode": "acquire", "lease_budget_ms": 60000},
}))
PY
)"
ACQ_REPLY="$(post_control "${ACQ_BODY}")"
echo "${ACQ_REPLY}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("ok"), d; print("acquire ok term="+str(d["payload"]["owner_term"]))'
TERM="$(printf '%s' "${ACQ_REPLY}" | python_get payload.owner_term)"

echo "== Start (isolated; not default gateway) =="
echo "${HS_REPLY}" | python3 -c 'import json,sys; d=json.load(sys.stdin); caps=d.get("payload",{}).get("capabilities") or []; assert "control.start" in caps, caps; print("handshake capability control.start ok")'
ACT_BODY="$(python3 - "${HOME_DIR}" "${EPOCH}" "${TERM}" <<'PY'
import json, sys
home, epoch, term = sys.argv[1], sys.argv[2], int(sys.argv[3])
print(json.dumps({
    "type": "Start",
    "request_id": "probe-start-1",
    "instance_epoch": epoch,
    "owner_id": "probe-owner",
    "owner_term": term,
    "app_data_dir": home,
    "payload": {},
}))
PY
)"
ACT_REPLY="$(post_control "${ACT_BODY}")"
echo "${ACT_REPLY}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("ok"), d; print("start ok (isolated; not default gateway)")'

ST2_BODY="$(python3 - "${HOME_DIR}" "${EPOCH}" "${TERM}" <<'PY'
import json, sys
home, epoch, term = sys.argv[1], sys.argv[2], int(sys.argv[3])
print(json.dumps({
    "type": "Status",
    "request_id": "probe-st-2",
    "instance_epoch": epoch,
    "owner_id": "probe-owner",
    "owner_term": term,
    "app_data_dir": home,
    "payload": {},
}))
PY
)"
ST2_REPLY="$(post_control "${ST2_BODY}")"
echo "${ST2_REPLY}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("ok"), d; p=d["payload"]; assert p.get("listen_ready") is True; assert p.get("lifecycle")=="serving"; print("status serving port="+str(p.get("port")))'
if printf '%s' "${ST2_REPLY}" | grep -F -q "${SYNTHETIC_KEY}"; then
  echo "FAIL: status leaked synthetic key" >&2
  exit 1
fi

echo "== Messages JSON (synthetic key last4=${KEY_TAIL}) =="
MSG_JSON="$(curl -sS -D "${SCRATCH}/messages.headers" \
  -H "Authorization: Bearer ${SYNTHETIC_KEY}" \
  -H 'Content-Type: application/json' \
  --data-binary '{"model":"claude-probe-fixture","max_tokens":16,"stream":false,"messages":[{"role":"user","content":"ping"}]}' \
  "http://127.0.0.1:${MESSAGES_PORT}/v1/messages")"
echo "${MSG_JSON}" | python3 -c 'import json,sys; d=json.load(sys.stdin); texts=[c.get("text","") for c in d.get("content",[]) if isinstance(c,dict)]; assert "isolated-messages-ok" in "".join(texts), d; print("messages json ok")'

echo "== Messages SSE =="
MSG_SSE="$(curl -sS \
  -H "Authorization: Bearer ${SYNTHETIC_KEY}" \
  -H 'Content-Type: application/json' \
  --data-binary '{"model":"claude-probe-fixture","max_tokens":16,"stream":true,"messages":[{"role":"user","content":"ping"}]}' \
  "http://127.0.0.1:${MESSAGES_PORT}/v1/messages")"
if ! printf '%s' "${MSG_SSE}" | grep -q 'isolated-messages-ok'; then
  echo "FAIL: SSE missing fixture text" >&2
  printf '%s\n' "${MSG_SSE}" >&2
  exit 1
fi
echo "messages sse ok"

echo "== Stop =="
STOP_BODY="$(python3 - "${HOME_DIR}" "${EPOCH}" "${TERM}" <<'PY'
import json, sys
home, epoch, term = sys.argv[1], sys.argv[2], int(sys.argv[3])
print(json.dumps({
    "type": "Stop",
    "request_id": "probe-stop-1",
    "instance_epoch": epoch,
    "owner_id": "probe-owner",
    "owner_term": term,
    "app_data_dir": home,
    "payload": {},
}))
PY
)"
STOP_REPLY="$(post_control "${STOP_BODY}")"
echo "${STOP_REPLY}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("ok"), d; print("stop ok")'

echo "== process down =="
kill "${ADAPTERD_PID}" 2>/dev/null || true
for i in $(seq 1 50); do
  if ! kill -0 "${ADAPTERD_PID}" 2>/dev/null; then
    break
  fi
  sleep 0.1
done
if kill -0 "${ADAPTERD_PID}" 2>/dev/null; then
  echo "FAIL: adapterd still running" >&2
  exit 1
fi
wait "${ADAPTERD_PID}" 2>/dev/null || true
if [[ -S "${SOCK}" ]]; then
  echo "WARN: control socket file still present after process down (expected unlink on clean Stop; SIGTERM may leave a stale inode)"
fi
if curl -sS --unix-socket "${SOCK}" http://127.0.0.1/healthz >/dev/null 2>&1; then
  echo "FAIL: control socket still accepted traffic after process down" >&2
  exit 1
fi
echo "process down observed"

if grep -F -q "${SYNTHETIC_KEY}" "${LOG_FILE}" "${ADAPTERD_LOG_STDOUT}" "${MOCK_LOG}" 2>/dev/null; then
  echo "FAIL: synthetic key found in logs" >&2
  exit 1
fi

python3 - "${EVIDENCE}" "${RUN_ID}" "${HOME_DIR}" "${MESSAGES_PORT}" "${UPSTREAM_PORT}" "${EPOCH}" "${TERM}" "${ROOT}" <<'PY'
import json, os, subprocess, sys
path, run_id, home, mport, uport, epoch, term, root = sys.argv[1:9]
sha = subprocess.check_output(["git", "-C", root, "rev-parse", "HEAD"], text=True).strip()
dirty = subprocess.check_output(["git", "-C", root, "status", "--porcelain"], text=True)
evidence = {
    "build_sha": sha,
    "dirty": bool(dirty.strip()),
    "run_id": run_id,
    "platform": os.uname().sysname,
    "arch": os.uname().machine,
    "agenthub_home": home,
    "messages_port": int(mport),
    "upstream_port": int(uport),
    "instance_epoch": epoch,
    "owner_term": int(term),
    "handshake": "ok",
    "status": "ok",
    "process_up": True,
    "messages_json": "isolated-messages-ok",
    "messages_sse": "isolated-messages-ok",
    "process_down": True,
    "live_gateway_unchanged": True,
    "real_home_untouched": True,
    "note": "isolated synthetic-key Messages slice; Start is not the default gateway",
}
with open(path, "w", encoding="utf-8") as fh:
    json.dump(evidence, fh, indent=2)
    fh.write("\n")
print("evidence written:", path)
PY

echo
echo "PASS: isolated Messages probe"
echo "scratch: ${SCRATCH}"
echo "evidence: ${EVIDENCE}"
