#!/usr/bin/env bash
# Isolated Go adapterd connection-pool Messages probe.
# Priority, round_robin, cooldown/failover, models union, client cancel, Stop.
# Never touches ~/.agenthub or the live/default gateway.
#
# Usage (from repository root):
#   scripts/route-runtime-probe/pool-isolated.sh

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

MEMBER_HI="sk-member-hi-synthetic"
MEMBER_LO="sk-member-lo-synthetic"
MEMBER_A="sk-member-a-synthetic"
MEMBER_B="sk-member-b-synthetic"
MEMBER_QUOTA="sk-member-quota-synthetic"
MEMBER_DOWN="sk-member-down-synthetic"
MEMBER_SLOW="sk-member-slow-synthetic"
MEMBER_COMMIT="sk-member-commit-synthetic"

ADAPTERD_PID=""
MOCK_PID=""
MESSAGES_PORT=""
failures=()
leftovers=()

mkdir -p "${SCRATCH}/bin" "${SCRATCH}/resp" "${HOME_DIR}/config" "${HOME_DIR}/run" "${HOME_DIR}/logs"

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

refuse_product_port() {
  local port="$1"
  if [[ "${port}" == "${PRODUCT_PORT}" ]]; then
    echo "refusing product default port ${PRODUCT_PORT}" >&2
    exit 1
  fi
}

UPSTREAM_PORT="$(pick_port)"
refuse_product_port "${UPSTREAM_PORT}"

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

record_fail() {
  local name="$1"
  local detail="${2:-}"
  echo "FAIL: ${name}${detail:+ (${detail})}" >&2
  failures+=("${name}")
}

record_pass() {
  local name="$1"
  echo "PASS: ${name}"
}

note_leftover() {
  leftovers+=("$1")
}

echo "== build =="
echo "+ go build -o ${BIN} ."
( cd "${MOD}" && go build -o "${BIN}" . )

echo "== start mock upstream =="
echo "+ ${BIN} mock-upstream --listen 127.0.0.1:${UPSTREAM_PORT}"
"${BIN}" mock-upstream --listen "127.0.0.1:${UPSTREAM_PORT}" >"${MOCK_LOG}" 2>&1 &
MOCK_PID=$!

cleanup() {
  local code=$?
  if [[ -n "${ADAPTERD_PID}" ]] && kill -0 "${ADAPTERD_PID}" 2>/dev/null; then
    kill "${ADAPTERD_PID}" 2>/dev/null || true
    wait "${ADAPTERD_PID}" 2>/dev/null || true
  fi
  if [[ -n "${MOCK_PID}" ]] && kill -0 "${MOCK_PID}" 2>/dev/null; then
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

wait_for_listen() {
  local port="$1"
  local i
  for i in $(seq 1 50); do
    if python3 - "${port}" <<'PY'
import socket, sys
port = int(sys.argv[1])
s = socket.socket()
s.settimeout(0.2)
try:
    s.connect(("127.0.0.1", port))
except OSError:
    sys.exit(1)
finally:
    s.close()
PY
    then
      return 0
    fi
    sleep 0.1
  done
  echo "FAIL: timed out waiting for 127.0.0.1:${port}" >&2
  return 1
}

wait_for_listen "${UPSTREAM_PORT}"
if ! kill -0 "${MOCK_PID}" 2>/dev/null; then
  echo "FAIL: mock-upstream is not running" >&2
  cat "${MOCK_LOG}" >&2 || true
  exit 1
fi

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

write_fixture() {
  local kind="$1"
  python3 - "${FIXTURE}" "${UPSTREAM_PORT}" "${SYNTHETIC_KEY}" "${kind}" \
    "${MEMBER_HI}" "${MEMBER_LO}" "${MEMBER_A}" "${MEMBER_B}" "${MEMBER_QUOTA}" "${MEMBER_SLOW}" <<'PY'
import json, sys
path, port, key, kind = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
hi, lo, a, b, quota, slow = sys.argv[5:11]
base = f"http://127.0.0.1:{port}"
model = "claude-probe-fixture"

def member(mid, token, priority, position, models):
    return {
        "id": mid,
        "upstream_base_url": base,
        "upstream_key": token,
        "priority": priority,
        "position": position,
        "models": models,
    }

members = {
    "priority_failover": [
        member("m-hi", hi, 0, 0, [model, "claude-probe-hi"]),
        member("m-lo", lo, 1, 1, [model, "claude-probe-lo"]),
    ],
    "round_robin": [
        member("m-a", a, 0, 0, [model]),
        member("m-b", b, 0, 1, [model]),
    ],
    "quota_failover": [
        member("m-quota", quota, 0, 0, [model]),
        member("m-healthy", hi, 1, 1, [model]),
    ],
    "slow": [
        member("m-slow", slow, 0, 0, [model]),
    ],
}[kind]

policy = "round_robin" if kind == "round_robin" else "priority_failover"
doc = {
    "ingress_key": key,
    "upstream_base_url": base,
    "fixture_model": model,
    "schedule_policy": policy,
    "members": members,
}
with open(path, "w", encoding="utf-8") as fh:
    json.dump(doc, fh)
    fh.write("\n")
PY
  chmod 600 "${FIXTURE}"
}

start_adapterd() {
  local port="$1"
  refuse_product_port "${port}"
  rm -f "${PID_FILE}"
  echo "+ AGENTHUB_HOME=${HOME_DIR} ${BIN} run --home ${HOME_DIR} --listen-port ${port}"
  AGENTHUB_HOME="${HOME_DIR}" "${BIN}" run --home "${HOME_DIR}" --listen-port "${port}" \
    >>"${ADAPTERD_LOG_STDOUT}" 2>&1 &
  ADAPTERD_PID=$!
  wait_for_file "${SOCK}"
  wait_for_file "${PID_FILE}"
  if ! kill -0 "${ADAPTERD_PID}" 2>/dev/null; then
    echo "FAIL: adapterd is not running" >&2
    cat "${ADAPTERD_LOG_STDOUT}" >&2 || true
    exit 1
  fi
  echo "process up: pid=${ADAPTERD_PID} messages_port=${port}"
}

wait_adapterd_exit() {
  local i
  for i in $(seq 1 50); do
    if [[ -z "${ADAPTERD_PID}" ]] || ! kill -0 "${ADAPTERD_PID}" 2>/dev/null; then
      wait "${ADAPTERD_PID}" 2>/dev/null || true
      ADAPTERD_PID=""
      return 0
    fi
    sleep 0.1
  done
  echo "WARN: adapterd still running after Stop; sending SIGTERM"
  if [[ -n "${ADAPTERD_PID}" ]]; then
    kill "${ADAPTERD_PID}" 2>/dev/null || true
    wait "${ADAPTERD_PID}" 2>/dev/null || true
  fi
  ADAPTERD_PID=""
}

handshake_acquire_start() {
  local tag="$1"
  local hs_body acq_body act_body st_body
  hs_body="$(python3 - "${HOME_DIR}" <<'PY'
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
  HS_REPLY="$(post_control "${hs_body}")"
  echo "${HS_REPLY}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("ok"), d; print("handshake ok")'
  EPOCH="$(printf '%s' "${HS_REPLY}" | python_get payload.instance_epoch)"
  INSTANCE="$(printf '%s' "${HS_REPLY}" | python_get payload.instance_id)"
  if [[ -z "${EPOCH}" || -z "${INSTANCE}" ]]; then
    echo "FAIL: handshake missing instance identity: ${HS_REPLY}" >&2
    exit 1
  fi
  printf '%s\n' "${HS_REPLY}" >"${SCRATCH}/status-${tag}-handshake.json"

  acq_body="$(python3 - "${HOME_DIR}" "${EPOCH}" <<'PY'
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
  ACQ_REPLY="$(post_control "${acq_body}")"
  echo "${ACQ_REPLY}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("ok"), d; print("acquire ok term="+str(d["payload"]["owner_term"]))'
  TERM="$(printf '%s' "${ACQ_REPLY}" | python_get payload.owner_term)"

  act_body="$(python3 - "${HOME_DIR}" "${EPOCH}" "${TERM}" <<'PY'
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
  ACT_REPLY="$(post_control "${act_body}")"
  echo "${ACT_REPLY}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("ok"), d; print("start ok (isolated; not default gateway)")'
  printf '%s\n' "${ACT_REPLY}" >"${SCRATCH}/status-${tag}-start.json"

  st_body="$(python3 - "${HOME_DIR}" "${EPOCH}" "${TERM}" <<'PY'
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
  ST_REPLY="$(post_control "${st_body}")"
  echo "${ST_REPLY}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("ok"), d; p=d["payload"]; assert p.get("listen_ready") is True; assert p.get("lifecycle")=="serving"; print("status serving port="+str(p.get("port")))'
  printf '%s\n' "${ST_REPLY}" >"${SCRATCH}/status-${tag}.json"
  if printf '%s' "${ST_REPLY}" | grep -F -q "${SYNTHETIC_KEY}"; then
    record_fail "secret_scan_status_${tag}" "status leaked entry key last4=${KEY_TAIL}"
  fi
}

control_stop() {
  local tag="$1"
  local stop_body
  stop_body="$(python3 - "${HOME_DIR}" "${EPOCH}" "${TERM}" <<'PY'
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
  STOP_REPLY="$(post_control "${stop_body}")"
  echo "${STOP_REPLY}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("ok"), d; print("stop ok")'
  printf '%s\n' "${STOP_REPLY}" >"${SCRATCH}/status-${tag}-stop.json"
  wait_adapterd_exit
}

post_messages() {
  local out="$1"
  local hdr="$2"
  local stream="$3"
  local model="$4"
  shift 4 || true
  local extra=("$@")
  local payload
  payload="$(python3 - "${model}" "${stream}" <<'PY'
import json, sys
model, stream = sys.argv[1], sys.argv[2] == "true"
print(json.dumps({
    "model": model,
    "max_tokens": 16,
    "stream": stream,
    "messages": [{"role": "user", "content": "ping"}],
}))
PY
)"
  curl -sS -D "${hdr}" -o "${out}" -w '%{http_code}' \
    "${extra[@]}" \
    -H "Authorization: Bearer ${SYNTHETIC_KEY}" \
    -H 'Content-Type: application/json' \
    --data-binary "${payload}" \
    "http://127.0.0.1:${MESSAGES_PORT}/v1/messages" || true
}

body_has() {
  local path="$1"
  local needle="$2"
  grep -F -q -- "${needle}" "${path}"
}

assert_no_secrets_in() {
  local path="$1"
  local secret
  for secret in \
    "${SYNTHETIC_KEY}" \
    "${MEMBER_HI}" "${MEMBER_LO}" "${MEMBER_A}" "${MEMBER_B}" \
    "${MEMBER_QUOTA}" "${MEMBER_DOWN}" "${MEMBER_SLOW}" "${MEMBER_COMMIT}"
  do
    if grep -F -q -- "${secret}" "${path}" 2>/dev/null; then
      return 1
    fi
  done
  return 0
}

json_has_text() {
  local path="$1"
  local needle="$2"
  python3 - "${path}" "${needle}" <<'PY'
import json, sys
path, needle = sys.argv[1], sys.argv[2]
with open(path, encoding="utf-8") as fh:
    raw = fh.read()
try:
    data = json.loads(raw)
except json.JSONDecodeError:
    sys.exit(1)
texts = []
for block in data.get("content") or []:
    if isinstance(block, dict):
        texts.append(block.get("text") or "")
blob = "".join(texts) + raw
sys.exit(0 if needle in blob else 1)
PY
}

# --- cycle 1: priority_failover hi+lo, models, unknown model, Stop ---
echo "== cycle 1: priority_failover hi+lo =="
write_fixture priority_failover
MESSAGES_PORT="$(pick_port)"
refuse_product_port "${MESSAGES_PORT}"
start_adapterd "${MESSAGES_PORT}"
handshake_acquire_start priority

echo "== check 1: two POSTs served by hi, not lo (entry last4=${KEY_TAIL}) =="
CODE1="$(post_messages "${SCRATCH}/resp/priority-1.json" "${SCRATCH}/resp/priority-1.headers" false claude-probe-fixture)"
CODE2="$(post_messages "${SCRATCH}/resp/priority-2.sse" "${SCRATCH}/resp/priority-2.headers" true claude-probe-fixture)"
if [[ "${CODE1}" == "200" ]] && json_has_text "${SCRATCH}/resp/priority-1.json" "isolated-member-hi" \
  && ! json_has_text "${SCRATCH}/resp/priority-1.json" "isolated-member-lo" \
  && [[ "${CODE2}" == "200" ]] && body_has "${SCRATCH}/resp/priority-2.sse" "isolated-member-hi" \
  && ! body_has "${SCRATCH}/resp/priority-2.sse" "isolated-member-lo"; then
  record_pass "priority_failover"
else
  record_fail "priority_failover" "json=${CODE1} sse=${CODE2}"
  if body_has "${SCRATCH}/resp/priority-1.json" "isolated-messages-ok" || body_has "${SCRATCH}/resp/priority-2.sse" "isolated-messages-ok"; then
    note_leftover "priority_failover: responses used default isolated-messages-ok (member Authorization not dispatched)"
  fi
fi
if ! assert_no_secrets_in "${SCRATCH}/resp/priority-1.json" || ! assert_no_secrets_in "${SCRATCH}/resp/priority-2.sse"; then
  record_fail "priority_failover_secret" "response leaked a member token"
fi

echo "== check 4: GET /v1/models union; unknown model not 200 =="
MODELS_CODE="$(curl -sS -o "${SCRATCH}/resp/models.json" -w '%{http_code}' \
  -H "Authorization: Bearer ${SYNTHETIC_KEY}" \
  "http://127.0.0.1:${MESSAGES_PORT}/v1/models" || true)"
MODELS_ALT_CODE="$(curl -sS -o "${SCRATCH}/resp/models-alt.json" -w '%{http_code}' \
  -H "Authorization: Bearer ${SYNTHETIC_KEY}" \
  "http://127.0.0.1:${MESSAGES_PORT}/models" || true)"
if python3 - "${SCRATCH}/resp/models.json" <<'PY'
import json, sys
path = sys.argv[1]
with open(path, encoding="utf-8") as fh:
    raw = fh.read()
try:
    data = json.loads(raw)
except json.JSONDecodeError:
    sys.exit(1)
if data.get("object") != "list":
    sys.exit(1)
ids = [item.get("id") for item in (data.get("data") or []) if isinstance(item, dict)]
need = {"claude-probe-fixture", "claude-probe-hi", "claude-probe-lo"}
sys.exit(0 if need.issubset(set(ids)) else 1)
PY
then
  record_pass "models_union"
else
  record_fail "models_union" "/v1/models=${MODELS_CODE} /models=${MODELS_ALT_CODE}"
  note_leftover "models_union: GET /v1/models did not list fixture union"
fi

UNKNOWN_CODE="$(post_messages "${SCRATCH}/resp/unknown.json" "${SCRATCH}/resp/unknown.headers" false does-not-exist-xyz)"
if [[ "${UNKNOWN_CODE}" != "200" && "${UNKNOWN_CODE}" != "000" ]]; then
  record_pass "unknown_model"
else
  record_fail "unknown_model" "status=${UNKNOWN_CODE}"
  note_leftover "unknown_model: POST model=does-not-exist-xyz returned ${UNKNOWN_CODE}"
fi

echo "== check 6: after Stop, new POST is not accepted =="
control_stop priority
AFTER_CODE="$(curl -sS -o "${SCRATCH}/resp/after-stop.json" -w '%{http_code}' --max-time 2 \
  -H "Authorization: Bearer ${SYNTHETIC_KEY}" \
  -H 'Content-Type: application/json' \
  --data-binary '{"model":"claude-probe-fixture","max_tokens":16,"stream":false,"messages":[{"role":"user","content":"ping"}]}' \
  "http://127.0.0.1:${MESSAGES_PORT}/v1/messages" || true)"
if [[ "${AFTER_CODE}" == "000" || "${AFTER_CODE}" == "503" ]]; then
  record_pass "after_stop"
else
  record_fail "after_stop" "status=${AFTER_CODE}"
fi

# --- cycle 2: round_robin a+b (new adapterd; not a second Start on the same process) ---
echo "== cycle 2: round_robin a+b =="
write_fixture round_robin
MESSAGES_PORT="$(pick_port)"
refuse_product_port "${MESSAGES_PORT}"
start_adapterd "${MESSAGES_PORT}"
handshake_acquire_start rr

echo "== check 2: consecutive POSTs alternate a then b =="
RR1="$(post_messages "${SCRATCH}/resp/rr-1.json" "${SCRATCH}/resp/rr-1.headers" false claude-probe-fixture)"
RR2="$(post_messages "${SCRATCH}/resp/rr-2.json" "${SCRATCH}/resp/rr-2.headers" false claude-probe-fixture)"
RR_ORDER="unknown"
if json_has_text "${SCRATCH}/resp/rr-1.json" "isolated-member-a" && json_has_text "${SCRATCH}/resp/rr-2.json" "isolated-member-b"; then
  RR_ORDER="a_then_b"
  record_pass "round_robin"
elif json_has_text "${SCRATCH}/resp/rr-1.json" "isolated-member-b" && json_has_text "${SCRATCH}/resp/rr-2.json" "isolated-member-a"; then
  RR_ORDER="b_then_a"
  record_pass "round_robin"
  echo "note: round_robin order was b then a (position/id sort)"
else
  record_fail "round_robin" "json1=${RR1} json2=${RR2}"
  if body_has "${SCRATCH}/resp/rr-1.json" "isolated-messages-ok"; then
    note_leftover "round_robin: default isolated-messages-ok (member Authorization not dispatched)"
  fi
fi
control_stop rr

# --- cycle 3: quota p0 + healthy hi p1 ---
echo "== cycle 3: quota failover to healthy =="
write_fixture quota_failover
MESSAGES_PORT="$(pick_port)"
refuse_product_port "${MESSAGES_PORT}"
start_adapterd "${MESSAGES_PORT}"
handshake_acquire_start quota

echo "== check 3: one POST succeeds via healthy; no quota key leak =="
Q1="$(post_messages "${SCRATCH}/resp/quota-1.json" "${SCRATCH}/resp/quota-1.headers" false claude-probe-fixture)"
quota_ok=0
if [[ "${Q1}" == "200" ]] && json_has_text "${SCRATCH}/resp/quota-1.json" "isolated-member-hi"; then
  quota_ok=1
else
  Q2="$(post_messages "${SCRATCH}/resp/quota-2.json" "${SCRATCH}/resp/quota-2.headers" false claude-probe-fixture)"
  if [[ "${Q2}" == "200" ]] && json_has_text "${SCRATCH}/resp/quota-2.json" "isolated-member-hi"; then
    quota_ok=1
  fi
fi
if [[ "${quota_ok}" -eq 1 ]]; then
  record_pass "quota_failover"
else
  record_fail "quota_failover" "first=${Q1}"
  note_leftover "quota_failover: client did not get isolated-member-hi from healthy member"
fi
if ! assert_no_secrets_in "${SCRATCH}/resp/quota-1.json"; then
  record_fail "quota_failover_secret" "quota member token leaked to client"
fi
control_stop quota

# --- cycle 4: slow member, client cancel, process stays up ---
echo "== cycle 4: slow member client cancel =="
write_fixture slow
MESSAGES_PORT="$(pick_port)"
refuse_product_port "${MESSAGES_PORT}"
start_adapterd "${MESSAGES_PORT}"
handshake_acquire_start slow

echo "== check 5: curl --max-time 1 against slow member; adapterd still running =="
set +e
SLOW_CODE="$(post_messages "${SCRATCH}/resp/slow.json" "${SCRATCH}/resp/slow.headers" false claude-probe-fixture --max-time 1)"
SLOW_RC=$?
set -e
if kill -0 "${ADAPTERD_PID}" 2>/dev/null; then
  if [[ "${SLOW_CODE}" == "200" ]]; then
    record_fail "client_cancel" "curl completed with 200 instead of cancel"
    if body_has "${SCRATCH}/resp/slow.json" "isolated-messages-ok"; then
      note_leftover "client_cancel: default mock success (slow member Authorization not dispatched)"
    fi
  else
    record_pass "client_cancel"
  fi
else
  record_fail "client_cancel" "adapterd exited during cancel (curl_rc=${SLOW_RC} status=${SLOW_CODE})"
fi

control_stop slow

echo "== check 7: logs + status JSON must not contain entry or member tokens =="
secret_scan_fail=0
for secret in \
  "${SYNTHETIC_KEY}" \
  "${MEMBER_HI}" "${MEMBER_LO}" "${MEMBER_A}" "${MEMBER_B}" \
  "${MEMBER_QUOTA}" "${MEMBER_DOWN}" "${MEMBER_SLOW}" "${MEMBER_COMMIT}"
do
  if grep -F -q -- "${secret}" "${LOG_FILE}" "${ADAPTERD_LOG_STDOUT}" "${MOCK_LOG}" \
    "${SCRATCH}"/status-*.json 2>/dev/null; then
    echo "FAIL: token last4=${secret: -4} found in logs or status JSON" >&2
    secret_scan_fail=1
  fi
done
if [[ "${secret_scan_fail}" -eq 0 ]]; then
  record_pass "secret_scan"
else
  record_fail "secret_scan"
fi

python3 - "${EVIDENCE}" "${RUN_ID}" "${HOME_DIR}" "${UPSTREAM_PORT}" "${ROOT}" \
  "${RR_ORDER}" "$(IFS=,; echo "${failures[*]}")" "$(IFS=,; echo "${leftovers[*]}")" <<'PY'
import json, os, subprocess, sys
path, run_id, home, uport, root, rr_order, fails, left = sys.argv[1:9]
sha = subprocess.check_output(["git", "-C", root, "rev-parse", "HEAD"], text=True).strip()
dirty = subprocess.check_output(["git", "-C", root, "status", "--porcelain"], text=True)
fail_list = [x for x in fails.split(",") if x]
left_list = [x for x in left.split(",") if x]
names = [
    "priority_failover",
    "round_robin",
    "quota_failover",
    "models_union",
    "unknown_model",
    "client_cancel",
    "after_stop",
    "secret_scan",
]
checks = {name: (name not in fail_list) for name in names}
evidence = {
    "build_sha": sha,
    "dirty": bool(dirty.strip()),
    "run_id": run_id,
    "platform": os.uname().sysname,
    "arch": os.uname().machine,
    "agenthub_home": home,
    "upstream_port": int(uport),
    "round_robin_order": rr_order,
    "checks": checks,
    "failed_checks": fail_list,
    "leftover": left_list,
    "live_gateway_unchanged": True,
    "real_home_untouched": True,
    "note": "isolated synthetic-key pool Messages slice; Start is not the default gateway",
}
with open(path, "w", encoding="utf-8") as fh:
    json.dump(evidence, fh, indent=2)
    fh.write("\n")
print("evidence written:", path)
PY

if grep -F -q -- "${SYNTHETIC_KEY}" "${EVIDENCE}" \
  || grep -E -q 'sk-member-[a-z]+-synthetic' "${EVIDENCE}"; then
  echo "FAIL: evidence.json contains a secret" >&2
  record_fail "evidence_secret"
fi

echo
echo "scratch: ${SCRATCH}"
echo "evidence: ${EVIDENCE}"
if [[ ${#leftovers[@]} -gt 0 ]]; then
  echo "leftover:"
  for item in "${leftovers[@]}"; do
    printf '  - %s\n' "${item}"
  done
fi
if [[ ${#failures[@]} -gt 0 ]]; then
  echo "FAIL: isolated pool probe (${#failures[@]} checks)"
  exit 1
fi
echo "PASS: isolated pool probe"
