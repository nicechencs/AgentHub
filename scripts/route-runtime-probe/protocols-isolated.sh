#!/usr/bin/env bash
# Isolated Go adapterd Responses then Chat Completions probe.
# JSON success, tool-call SSE, cancel, quota failover, commit-then-stop, 405/401.
# Never touches ~/.agenthub or the live/default gateway. Does not bind 43121.
#
# Usage (from repository root):
#   scripts/route-runtime-probe/protocols-isolated.sh

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
WRONG_KEY="ahb_probe_wrong_bearer_not_ingress"
REAL_HOME="${HOME}/.agenthub"
PRODUCT_PORT=43121
MODEL="gpt-5-probe"
RESPONSES_DENY_MESSAGE='This endpoint only accepts POST /v1/responses. 本机该路径只接受 POST /v1/responses。'
CHAT_DENY_MESSAGE='This endpoint only accepts POST /v1/chat/completions. 本机该路径只接受 POST /v1/chat/completions。'
CHAT_ALIAS_DENY_MESSAGE='This endpoint only accepts POST /chat/completions. 本机该路径只接受 POST /chat/completions。'

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
RESPONSES_PORT=""
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
    "${MEMBER_HI}" "${MEMBER_QUOTA}" "${MEMBER_SLOW}" "${MEMBER_COMMIT}" "${MODEL}" <<'PY'
import json, sys
path, port, key, kind = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
hi, quota, slow, commit, model = sys.argv[5:10]
base = f"http://127.0.0.1:{port}"

def member(mid, token, priority, position):
    return {
        "id": mid,
        "upstream_base_url": base,
        "upstream_key": token,
        "priority": priority,
        "position": position,
        "models": [model],
    }

members = {
    "hi": [member("m-hi", hi, 0, 0)],
    "slow": [member("m-slow", slow, 0, 0)],
    "quota": [
        member("m-quota", quota, 0, 0),
        member("m-hi", hi, 1, 1),
    ],
    "commit": [
        member("m-commit", commit, 0, 0),
        member("m-hi", hi, 1, 1),
    ],
}[kind]
doc = {
    "ingress_key": key,
    "upstream_base_url": base,
    "fixture_model": model,
    "schedule_policy": "priority_failover",
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
  echo "process up: pid=${ADAPTERD_PID} responses_port=${port}"
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

post_responses() {
  local out="$1"
  local hdr="$2"
  local stream="$3"
  local tools="$4"
  shift 4 || true
  local extra=("$@")
  local payload
  payload="$(python3 - "${MODEL}" "${stream}" "${tools}" <<'PY'
import json, sys
model, stream, tools = sys.argv[1], sys.argv[2] == "true", sys.argv[3] == "true"
body = {"model": model, "input": "ping", "stream": stream}
if tools:
    body["tools"] = [{"type": "function", "name": "weather"}]
print(json.dumps(body))
PY
)"
  curl -sS -D "${hdr}" -o "${out}" -w '%{http_code}' \
    "${extra[@]}" \
    -H "Authorization: Bearer ${SYNTHETIC_KEY}" \
    -H 'Content-Type: application/json' \
    --data-binary "${payload}" \
    "http://127.0.0.1:${RESPONSES_PORT}/v1/responses" || true
}

body_has() {
  local path="$1"
  local needle="$2"
  [[ -f "${path}" ]] && grep -F -q -- "${needle}" "${path}"
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

responses_json_ok() {
  python3 - "$1" "$2" <<'PY'
import json, sys
body_path, hdr_path = sys.argv[1], sys.argv[2]
hdr = open(hdr_path, encoding="utf-8", errors="replace").read().lower()
if "application/json" not in hdr:
    sys.exit(2)
with open(body_path, encoding="utf-8") as fh:
    raw = fh.read()
try:
    data = json.loads(raw)
except json.JSONDecodeError:
    sys.exit(1)
if data.get("object") != "response" or data.get("status") != "completed":
    sys.exit(3)
output = data.get("output")
if not isinstance(output, list) or not output:
    sys.exit(4)
sys.exit(0 if "isolated-responses-ok" in raw else 5)
PY
}

responses_tool_sse_ok() {
  python3 - "$1" "$2" <<'PY'
import json, sys
body_path, hdr_path = sys.argv[1], sys.argv[2]
hdr = open(hdr_path, encoding="utf-8", errors="replace").read().lower()
if "text/event-stream" not in hdr or "no-cache" not in hdr:
    print("missing sse headers", file=sys.stderr)
    sys.exit(2)
raw = open(body_path, encoding="utf-8", errors="replace").read()
events = []
event = None
data_lines = []

def flush():
    global event, data_lines
    if event is None and not data_lines:
        return
    blob = "\n".join(data_lines).strip()
    payload = json.loads(blob) if blob else None
    events.append((event, payload))
    event = None
    data_lines = []

for line in raw.splitlines():
    if line.startswith("event:"):
        event = line[len("event:"):].strip()
    elif line.startswith("data:"):
        data_lines.append(line[len("data:"):].strip())
    elif line.strip() == "":
        flush()
flush()
names = [item[0] for item in events]
want = [
    "response.created",
    "response.output_item.added",
    "response.function_call_arguments.delta",
    "response.function_call_arguments.done",
    "response.output_item.done",
    "response.completed",
]
if names != want:
    print("event order: " + ",".join(str(n) for n in names), file=sys.stderr)
    sys.exit(3)
added = events[1][1] or {}
item = added.get("item") or {}
if item.get("type") != "function_call" or item.get("name") != "weather" or not item.get("call_id"):
    sys.exit(4)
done = events[3][1] or {}
args = done.get("arguments")
if not isinstance(args, str) or "city" not in args:
    sys.exit(5)
completed = events[5][1] or {}
output = (completed.get("response") or {}).get("output") or []
if not output or not isinstance(output[0], dict) or output[0].get("type") != "function_call":
    sys.exit(6)
PY
}

method_not_allowed_ok() {
  method_not_allowed_msg "$1" "$2" "${RESPONSES_DENY_MESSAGE}"
}

method_not_allowed_msg() {
  python3 - "$1" "$2" "$3" <<'PY'
import json, sys
body_path, hdr_path, want = sys.argv[1], sys.argv[2], sys.argv[3]
hdr = open(hdr_path, encoding="utf-8", errors="replace").read()
allow = ""
for line in hdr.splitlines():
    if line.lower().startswith("allow:"):
        allow = line.split(":", 1)[1].strip()
methods = [part.strip().upper() for part in allow.split(",") if part.strip()]
if "POST" not in methods:
    sys.exit(2)
with open(body_path, encoding="utf-8") as fh:
    data = json.load(fh)
err = data.get("error") or {}
if err.get("code") != "method_not_allowed":
    sys.exit(3)
if err.get("message") != want:
    sys.exit(4)
PY
}

invalid_key_ok() {
  python3 - "$1" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as fh:
    data = json.load(fh)
err = data.get("error") or {}
sys.exit(0 if err.get("code") == "invalid_api_key" else 1)
PY
}

# --- cycle 1: hi member, JSON + tool SSE + 405 + 401 ---
echo "== cycle 1: responses JSON and tool SSE (entry last4=${KEY_TAIL}) =="
write_fixture hi
RESPONSES_PORT="$(pick_port)"
refuse_product_port "${RESPONSES_PORT}"
start_adapterd "${RESPONSES_PORT}"
handshake_acquire_start responses

echo "== check 1: JSON stream false is a completed response =="
CODE_JSON="$(post_responses "${SCRATCH}/resp/responses.json" "${SCRATCH}/resp/responses.headers" false false)"
if [[ "${CODE_JSON}" == "200" ]] && responses_json_ok "${SCRATCH}/resp/responses.json" "${SCRATCH}/resp/responses.headers"; then
  record_pass "responses_json"
else
  record_fail "responses_json" "status=${CODE_JSON}"
  if [[ "${CODE_JSON}" == "404" ]]; then
    note_leftover "responses_json: POST /v1/responses returned 404"
  fi
fi
if ! assert_no_secrets_in "${SCRATCH}/resp/responses.json"; then
  record_fail "responses_json_secret" "response leaked a member token"
fi

echo "== check 2: tool SSE event order, weather call, city arguments =="
CODE_SSE="$(post_responses "${SCRATCH}/resp/responses-tools.sse" "${SCRATCH}/resp/responses-tools.headers" true true)"
if [[ "${CODE_SSE}" == "200" ]] && responses_tool_sse_ok "${SCRATCH}/resp/responses-tools.sse" "${SCRATCH}/resp/responses-tools.headers"; then
  record_pass "responses_tool_sse"
else
  record_fail "responses_tool_sse" "status=${CODE_SSE}"
  if [[ "${CODE_SSE}" == "404" ]]; then
    note_leftover "responses_tool_sse: POST /v1/responses returned 404"
  fi
fi

echo "== check 6: GET is 405; wrong bearer is 401 =="
CODE_GET="$(curl -sS -D "${SCRATCH}/resp/responses-get.headers" -o "${SCRATCH}/resp/responses-get.json" -w '%{http_code}' \
  -H "Authorization: Bearer ${SYNTHETIC_KEY}" \
  "http://127.0.0.1:${RESPONSES_PORT}/v1/responses" || true)"
if [[ "${CODE_GET}" == "405" ]] && method_not_allowed_ok "${SCRATCH}/resp/responses-get.json" "${SCRATCH}/resp/responses-get.headers"; then
  record_pass "method_not_allowed"
else
  record_fail "method_not_allowed" "status=${CODE_GET}"
  if [[ "${CODE_GET}" == "404" ]]; then
    note_leftover "method_not_allowed: GET /v1/responses returned 404"
  fi
fi

CODE_AUTH="$(curl -sS -D "${SCRATCH}/resp/responses-auth.headers" -o "${SCRATCH}/resp/responses-auth.json" -w '%{http_code}' \
  -H "Authorization: Bearer ${WRONG_KEY}" \
  -H 'Content-Type: application/json' \
  --data-binary "{\"model\":\"${MODEL}\",\"input\":\"ping\",\"stream\":false}" \
  "http://127.0.0.1:${RESPONSES_PORT}/v1/responses" || true)"
if [[ "${CODE_AUTH}" == "401" ]] && invalid_key_ok "${SCRATCH}/resp/responses-auth.json"; then
  record_pass "invalid_api_key"
else
  record_fail "invalid_api_key" "status=${CODE_AUTH}"
fi
control_stop responses

# --- cycle 2: slow member, client cancel ---
echo "== cycle 2: slow member client cancel =="
write_fixture slow
RESPONSES_PORT="$(pick_port)"
refuse_product_port "${RESPONSES_PORT}"
start_adapterd "${RESPONSES_PORT}"
handshake_acquire_start slow

echo "== check 3: curl --max-time 1 against slow member; adapterd still running =="
SLOW_CODE="$(post_responses "${SCRATCH}/resp/slow.json" "${SCRATCH}/resp/slow.headers" false false --max-time 1)"
if ! kill -0 "${ADAPTERD_PID}" 2>/dev/null; then
  record_fail "client_cancel" "adapterd exited during cancel (status=${SLOW_CODE})"
elif body_has "${SCRATCH}/resp/slow.json" "isolated-responses-ok" || body_has "${SCRATCH}/resp/slow.json" "isolated-member-slow"; then
  record_fail "client_cancel" "slow member returned success text status=${SLOW_CODE}"
elif [[ "${SLOW_CODE}" == "000" || -z "${SLOW_CODE}" ]]; then
  record_pass "client_cancel"
else
  record_fail "client_cancel" "status=${SLOW_CODE}"
  if [[ "${SLOW_CODE}" == "404" ]]; then
    note_leftover "client_cancel: POST /v1/responses returned 404"
  fi
fi
control_stop slow

# --- cycle 3: quota before any client byte, failover to hi ---
echo "== cycle 3: quota failover to hi =="
write_fixture quota
RESPONSES_PORT="$(pick_port)"
refuse_product_port "${RESPONSES_PORT}"
start_adapterd "${RESPONSES_PORT}"
handshake_acquire_start quota

echo "== check 4: one POST returns isolated-responses-ok, not the quota error =="
CODE_QUOTA="$(post_responses "${SCRATCH}/resp/quota.json" "${SCRATCH}/resp/quota.headers" false false)"
if [[ "${CODE_QUOTA}" == "200" ]] && responses_json_ok "${SCRATCH}/resp/quota.json" "${SCRATCH}/resp/quota.headers"; then
  record_pass "quota_failover"
else
  record_fail "quota_failover" "status=${CODE_QUOTA}"
  note_leftover "quota_failover: client did not get isolated-responses-ok from the healthy member"
fi
if ! assert_no_secrets_in "${SCRATCH}/resp/quota.json"; then
  record_fail "quota_failover_secret" "quota member token leaked to client"
fi
control_stop quota

# --- cycle 4: commit frame then close; do not replay the healthy member ---
echo "== cycle 4: after first byte, no failover replay =="
write_fixture commit
RESPONSES_PORT="$(pick_port)"
refuse_product_port "${RESPONSES_PORT}"
start_adapterd "${RESPONSES_PORT}"
handshake_acquire_start commit

echo "== check 5: stream body has response.created and not isolated-responses-ok =="
CODE_COMMIT="$(post_responses "${SCRATCH}/resp/commit.sse" "${SCRATCH}/resp/commit.headers" true false)"
if [[ "${CODE_COMMIT}" == "200" ]] \
  && body_has "${SCRATCH}/resp/commit.sse" "response.created" \
  && ! body_has "${SCRATCH}/resp/commit.sse" "isolated-responses-ok"; then
  record_pass "commit_no_replay"
else
  record_fail "commit_no_replay" "status=${CODE_COMMIT}"
  if body_has "${SCRATCH}/resp/commit.sse" "isolated-responses-ok"; then
    note_leftover "commit_no_replay: healthy member text appeared after the first byte"
  fi
fi
control_stop commit

post_chat() {
  local out="$1"
  local hdr="$2"
  local path="$3"
  local stream="$4"
  local tools="$5"
  shift 5 || true
  local extra=("$@")
  local payload
  payload="$(python3 - "${MODEL}" "${stream}" "${tools}" <<'PY'
import json, sys
model, stream, tools = sys.argv[1], sys.argv[2] == "true", sys.argv[3] == "true"
body = {
    "model": model,
    "messages": [{"role": "user", "content": "ping"}],
    "stream": stream,
}
if tools:
    body["tools"] = [{"type": "function", "function": {"name": "weather"}}]
print(json.dumps(body))
PY
)"
  curl -sS -D "${hdr}" -o "${out}" -w '%{http_code}' \
    "${extra[@]}" \
    -H "Authorization: Bearer ${SYNTHETIC_KEY}" \
    -H 'Content-Type: application/json' \
    --data-binary "${payload}" \
    "http://127.0.0.1:${RESPONSES_PORT}${path}" || true
}

chat_json_ok() {
  python3 - "$1" "$2" <<'PY'
import json, sys
body_path, hdr_path = sys.argv[1], sys.argv[2]
hdr = open(hdr_path, encoding="utf-8", errors="replace").read().lower()
if "application/json" not in hdr:
    sys.exit(2)
with open(body_path, encoding="utf-8") as fh:
    raw = fh.read()
data = json.loads(raw)
if data.get("object") != "chat.completion":
    sys.exit(3)
choices = data.get("choices") or []
if not choices or not isinstance(choices[0].get("message"), dict):
    sys.exit(4)
sys.exit(0 if "isolated-chat-ok" in raw else 5)
PY
}

chat_tool_sse_ok() {
  python3 - "$1" "$2" <<'PY'
import json, sys
body_path, hdr_path = sys.argv[1], sys.argv[2]
hdr = open(hdr_path, encoding="utf-8", errors="replace").read().lower()
if "text/event-stream" not in hdr:
    sys.exit(2)
raw = open(body_path, encoding="utf-8", errors="replace").read()
if "chat.completion.chunk" not in raw or "[DONE]" not in raw:
    sys.exit(3)
chunks = []
for line in raw.splitlines():
    if not line.startswith("data:"):
        continue
    blob = line[len("data:"):].strip()
    if blob == "[DONE]":
        continue
    chunks.append(json.loads(blob))
if not chunks:
    sys.exit(4)
first = chunks[0]
delta = ((first.get("choices") or [{}])[0].get("delta") or {})
calls = delta.get("tool_calls") or []
name = ""
if calls:
    name = ((calls[0].get("function") or {}).get("name") or "")
if name != "weather":
    sys.exit(5)
joined = raw
if "tool_calls" not in joined or "city" not in joined:
    sys.exit(6)
finish = None
for chunk in chunks:
    finish = ((chunk.get("choices") or [{}])[0].get("finish_reason")) or finish
if finish != "tool_calls":
    sys.exit(7)
PY
}

echo "== cycle 5: chat completions JSON, alias, tool SSE, 405 =="
write_fixture hi
RESPONSES_PORT="$(pick_port)"
refuse_product_port "${RESPONSES_PORT}"
start_adapterd "${RESPONSES_PORT}"
handshake_acquire_start chat-json

echo "== check 8: chat JSON and alias =="
CODE_CHAT="$(post_chat "${SCRATCH}/resp/chat.json" "${SCRATCH}/resp/chat.headers" /v1/chat/completions false false)"
CODE_ALIAS="$(post_chat "${SCRATCH}/resp/chat-alias.json" "${SCRATCH}/resp/chat-alias.headers" /chat/completions false false)"
if [[ "${CODE_CHAT}" == "200" ]] && chat_json_ok "${SCRATCH}/resp/chat.json" "${SCRATCH}/resp/chat.headers" \
  && [[ "${CODE_ALIAS}" == "200" ]] && chat_json_ok "${SCRATCH}/resp/chat-alias.json" "${SCRATCH}/resp/chat-alias.headers"; then
  record_pass "chat_json"
else
  record_fail "chat_json" "v1=${CODE_CHAT} alias=${CODE_ALIAS}"
fi

echo "== check 9: chat tool SSE =="
CODE_CHAT_SSE="$(post_chat "${SCRATCH}/resp/chat-tool.sse" "${SCRATCH}/resp/chat-tool.headers" /v1/chat/completions true true)"
if [[ "${CODE_CHAT_SSE}" == "200" ]] && chat_tool_sse_ok "${SCRATCH}/resp/chat-tool.sse" "${SCRATCH}/resp/chat-tool.headers"; then
  record_pass "chat_tool_sse"
else
  record_fail "chat_tool_sse" "status=${CODE_CHAT_SSE}"
fi

echo "== check 10: chat 405 uses the request path =="
CHAT_GET="$(curl -sS -D "${SCRATCH}/resp/chat-get.headers" -o "${SCRATCH}/resp/chat-get.json" -w '%{http_code}' \
  -H "Authorization: Bearer ${SYNTHETIC_KEY}" \
  "http://127.0.0.1:${RESPONSES_PORT}/v1/chat/completions" || true)"
ALIAS_GET="$(curl -sS -D "${SCRATCH}/resp/chat-alias-get.headers" -o "${SCRATCH}/resp/chat-alias-get.json" -w '%{http_code}' \
  -H "Authorization: Bearer ${SYNTHETIC_KEY}" \
  "http://127.0.0.1:${RESPONSES_PORT}/chat/completions" || true)"
CHAT_BAD="$(curl -sS -D "${SCRATCH}/resp/chat-bad.headers" -o "${SCRATCH}/resp/chat-bad.json" -w '%{http_code}' \
  -H "Authorization: Bearer ${WRONG_KEY}" \
  -H 'Content-Type: application/json' \
  --data-binary '{"model":"gpt-5-probe","messages":[{"role":"user","content":"ping"}]}' \
  "http://127.0.0.1:${RESPONSES_PORT}/v1/chat/completions" || true)"
if [[ "${CHAT_GET}" == "405" ]] && method_not_allowed_msg "${SCRATCH}/resp/chat-get.json" "${SCRATCH}/resp/chat-get.headers" "${CHAT_DENY_MESSAGE}" \
  && [[ "${ALIAS_GET}" == "405" ]] && method_not_allowed_msg "${SCRATCH}/resp/chat-alias-get.json" "${SCRATCH}/resp/chat-alias-get.headers" "${CHAT_ALIAS_DENY_MESSAGE}" \
  && [[ "${CHAT_BAD}" == "401" ]] && invalid_key_ok "${SCRATCH}/resp/chat-bad.json"; then
  record_pass "chat_method_and_auth"
else
  record_fail "chat_method_and_auth" "get=${CHAT_GET} alias=${ALIAS_GET} bad=${CHAT_BAD}"
fi
control_stop chat-json

echo "== cycle 6: chat quota failover =="
write_fixture quota
RESPONSES_PORT="$(pick_port)"
refuse_product_port "${RESPONSES_PORT}"
start_adapterd "${RESPONSES_PORT}"
handshake_acquire_start chat-quota
CODE_CHAT_Q="$(post_chat "${SCRATCH}/resp/chat-quota.json" "${SCRATCH}/resp/chat-quota.headers" /v1/chat/completions false false)"
if [[ "${CODE_CHAT_Q}" == "200" ]] && body_has "${SCRATCH}/resp/chat-quota.json" "isolated-chat-ok"; then
  record_pass "chat_quota_failover"
else
  record_fail "chat_quota_failover" "status=${CODE_CHAT_Q}"
fi
control_stop chat-quota

echo "== cycle 7: chat commit does not replay =="
write_fixture commit
RESPONSES_PORT="$(pick_port)"
refuse_product_port "${RESPONSES_PORT}"
start_adapterd "${RESPONSES_PORT}"
handshake_acquire_start chat-commit
CODE_CHAT_C="$(post_chat "${SCRATCH}/resp/chat-commit.sse" "${SCRATCH}/resp/chat-commit.headers" /v1/chat/completions true false)"
if [[ "${CODE_CHAT_C}" == "200" ]] \
  && body_has "${SCRATCH}/resp/chat-commit.sse" "data:" \
  && ! body_has "${SCRATCH}/resp/chat-commit.sse" "isolated-chat-ok"; then
  record_pass "chat_commit_no_replay"
else
  record_fail "chat_commit_no_replay" "status=${CODE_CHAT_C}"
fi
control_stop chat-commit

echo "== cycle 8: chat client cancel =="
write_fixture slow
RESPONSES_PORT="$(pick_port)"
refuse_product_port "${RESPONSES_PORT}"
start_adapterd "${RESPONSES_PORT}"
handshake_acquire_start chat-slow
set +e
curl -sS --max-time 1 -o "${SCRATCH}/resp/chat-cancel.body" \
  -H "Authorization: Bearer ${SYNTHETIC_KEY}" \
  -H 'Content-Type: application/json' \
  --data-binary "{\"model\":\"${MODEL}\",\"messages\":[{\"role\":\"user\",\"content\":\"ping\"}],\"stream\":false}" \
  "http://127.0.0.1:${RESPONSES_PORT}/v1/chat/completions" >/dev/null
set -e
if kill -0 "${ADAPTERD_PID}" 2>/dev/null; then
  record_pass "chat_client_cancel"
else
  record_fail "chat_client_cancel" "adapterd exited"
fi
control_stop chat-slow

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
  "$(IFS=,; echo "${failures[*]-}")" "$(IFS=,; echo "${leftovers[*]-}")" <<'PY'
import json, os, subprocess, sys
path, run_id, home, uport, root, fails, left = sys.argv[1:8]
sha = subprocess.check_output(["git", "-C", root, "rev-parse", "HEAD"], text=True).strip()
dirty = subprocess.check_output(["git", "-C", root, "status", "--porcelain"], text=True)
fail_list = [x for x in fails.split(",") if x]
left_list = [x for x in left.split(",") if x]
names = [
    "responses_json",
    "responses_tool_sse",
    "client_cancel",
    "quota_failover",
    "commit_no_replay",
    "method_not_allowed",
    "invalid_api_key",
    "chat_json",
    "chat_tool_sse",
    "chat_method_and_auth",
    "chat_quota_failover",
    "chat_commit_no_replay",
    "chat_client_cancel",
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
    "phase": "responses+chat",
    "checks": checks,
    "failed_checks": fail_list,
    "leftover": left_list,
    "live_gateway_unchanged": True,
    "real_home_untouched": True,
    "note": "isolated synthetic-key Responses and Chat Completions; Start is not the default gateway",
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
  echo "FAIL: isolated responses probe (${#failures[@]} checks)"
  exit 1
fi
echo "PASS: isolated responses probe"
