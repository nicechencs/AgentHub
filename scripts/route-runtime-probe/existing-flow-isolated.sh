#!/usr/bin/env bash
# Runtime-config stdin, multi-edge routing, and graceful drain probe.
# Uses only a scratch AGENTHUB_HOME, synthetic keys, loopback upstream, and an ephemeral port.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MOD="${ROOT}/go/agenthub-adapterd"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
SCRATCH="/tmp/agenthub-route-runtime-existing-flow/${RUN_ID}"
HOME_DIR="${SCRATCH}/home"
BIN="${SCRATCH}/bin/agenthub-adapterd"
SOCK="${HOME_DIR}/run/adapterd.sock"
LOG_FILE="${HOME_DIR}/logs/adapterd.log"
ADAPTERD_STDOUT="${SCRATCH}/adapterd.stdout.log"
MOCK_LOG="${SCRATCH}/mock-upstream.log"
STATUS_LOG="${SCRATCH}/status.json"
EVIDENCE="${SCRATCH}/evidence.json"
PRODUCT_PORT=43121

ENTRY_MESSAGES="ahb_stdin_messages_synthetic"
ENTRY_RESPONSES="ahb_stdin_responses_synthetic"
ENTRY_CHAT="ahb_stdin_chat_synthetic"
ENTRY_DRAIN="ahb_stdin_drain_synthetic"
UPSTREAM_MESSAGES="sk-member-header-messages-synthetic"
UPSTREAM_RESPONSES="sk-member-header-responses-synthetic"
UPSTREAM_CHAT="sk-member-header-chat-synthetic"
UPSTREAM_DRAIN="sk-member-drain-synthetic"
BODY_MARKER="runtime-config-request-body-must-not-be-logged"

mkdir -p "${SCRATCH}/bin" "${HOME_DIR}/config" "${HOME_DIR}/run" "${HOME_DIR}/logs"

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
"${BIN}" mock-upstream --listen "127.0.0.1:${UPSTREAM_PORT}" >"${MOCK_LOG}" 2>&1 &
MOCK_PID=$!

# The sensitive runtime document flows directly to stdin; it is never written to disk.
AGENTHUB_HOME="${HOME_DIR}" "${BIN}" run --home "${HOME_DIR}" --listen-port "${LISTEN_PORT}" --runtime-config-stdin \
  < <(python3 - <<PY
import json
upstream = "http://127.0.0.1:${UPSTREAM_PORT}/v1"
def member(member_id, key, auth, model):
    return {"id": member_id, "upstream_base_url": upstream, "upstream_key": key,
            "upstream_auth": auth, "priority": 0, "position": 0, "models": [model]}
print(json.dumps({"version": "route-config.v0-isolated", "edges": [
    {"id": "messages", "ingress_key": "${ENTRY_MESSAGES}", "surface": "messages",
     "dialect": "claude", "schedule_policy": "priority_failover", "fixture_model": "claude-stdin-model",
     "members": [member("messages-member", "${UPSTREAM_MESSAGES}", "x_api_key", "claude-stdin-model")]},
    {"id": "responses", "ingress_key": "${ENTRY_RESPONSES}", "surface": "responses",
     "dialect": "codex", "schedule_policy": "priority_failover", "fixture_model": "gpt-stdin-response",
     "members": [member("responses-member", "${UPSTREAM_RESPONSES}", "bearer", "gpt-stdin-response")]},
    {"id": "chat", "ingress_key": "${ENTRY_CHAT}", "surface": "chat_completions",
     "dialect": "generic", "schedule_policy": "round_robin", "fixture_model": "gpt-stdin-chat",
     "members": [member("chat-member", "${UPSTREAM_CHAT}", "bearer", "gpt-stdin-chat")]},
    {"id": "drain", "ingress_key": "${ENTRY_DRAIN}", "surface": "messages",
     "dialect": "claude", "schedule_policy": "priority_failover", "fixture_model": "claude-drain-model",
     "members": [member("drain-member", "${UPSTREAM_DRAIN}", "x_api_key", "claude-drain-model")]}
]}))
PY
  ) >"${ADAPTERD_STDOUT}" 2>&1 &
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
    echo "FAIL: scratch retained at ${SCRATCH}" >&2
  fi
}
trap cleanup EXIT

for _ in $(seq 1 100); do
  [[ -S "${SOCK}" ]] && break
  if ! kill -0 "${ADAPTERD_PID}" 2>/dev/null; then
    echo "FAIL: adapterd exited before control socket became ready" >&2
    sed -n '1,120p' "${ADAPTERD_STDOUT}" >&2 || true
    exit 1
  fi
  sleep 0.05
done
[[ -S "${SOCK}" ]] || { echo "FAIL: control socket timeout" >&2; exit 1; }

post_control() {
  curl -sS --unix-socket "${SOCK}" -H 'Content-Type: application/json' --data-binary "$1" http://127.0.0.1/control
}
json_get() {
  python3 -c 'import json,sys
d=json.load(sys.stdin)
for key in sys.argv[1].split("."): d=d[key]
print(d)' "$1"
}

HS="$(post_control "$(python3 - <<PY
import json
home="${HOME_DIR}"
print(json.dumps({"type":"Handshake","request_id":"flow-hs","app_data_dir":home,"payload":{"protocol_version":"route-runtime.v0-isolated","config_format_version":"route-config.v0-isolated","package_version":"0.0.0-isolated","app_data_dir":home}}))
PY
)")"
EPOCH="$(printf '%s' "${HS}" | json_get payload.instance_epoch)"
ACQ="$(post_control "$(python3 - <<PY
import json
print(json.dumps({"type":"AcquireOrRenewOwner","request_id":"flow-acq","instance_epoch":"${EPOCH}","owner_id":"flow-owner","app_data_dir":"${HOME_DIR}","payload":{"mode":"acquire","lease_budget_ms":60000}}))
PY
)")"
TERM="$(printf '%s' "${ACQ}" | json_get payload.owner_term)"
START="$(post_control "$(python3 - <<PY
import json
print(json.dumps({"type":"Start","request_id":"flow-start","instance_epoch":"${EPOCH}","owner_id":"flow-owner","owner_term":int("${TERM}"),"app_data_dir":"${HOME_DIR}","payload":{}}))
PY
)")"
printf '%s' "${START}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["ok"], d; assert d["payload"]["listen_ready"] is True'

request() {
  local path="$1" key="$2" model="$3" output="$4"
  curl -sS -H "Authorization: Bearer ${key}" -H 'Content-Type: application/json' \
    --data-binary "{\"model\":\"${model}\",\"stream\":false,\"marker\":\"${BODY_MARKER}\"}" \
    "http://127.0.0.1:${LISTEN_PORT}${path}" >"${output}"
}

request /v1/messages "${ENTRY_MESSAGES}" claude-stdin-model "${SCRATCH}/messages.json"
grep -F -q 'isolated-header-messages-ok' "${SCRATCH}/messages.json"
request /v1/responses "${ENTRY_RESPONSES}" gpt-stdin-response "${SCRATCH}/responses.json"
grep -F -q 'isolated-header-responses-ok' "${SCRATCH}/responses.json"
request /v1/chat/completions "${ENTRY_CHAT}" gpt-stdin-chat "${SCRATCH}/chat.json"
grep -F -q 'isolated-header-chat-ok' "${SCRATCH}/chat.json"

for row in \
  "${ENTRY_MESSAGES}:claude-stdin-model:gpt-stdin-response" \
  "${ENTRY_RESPONSES}:gpt-stdin-response:gpt-stdin-chat" \
  "${ENTRY_CHAT}:gpt-stdin-chat:claude-stdin-model" \
  "${ENTRY_DRAIN}:claude-drain-model:claude-stdin-model"; do
  IFS=: read -r key expected forbidden <<<"${row}"
  models="$(curl -sS -H "Authorization: Bearer ${key}" "http://127.0.0.1:${LISTEN_PORT}/v1/models")"
  grep -F -q "${expected}" <<<"${models}"
  if grep -F -q "${forbidden}" <<<"${models}"; then
    echo "FAIL: /models crossed edge boundary" >&2
    exit 1
  fi
done

expect_model_isolated() {
  local key="$1" model="$2" output="$3"
  local code
  code="$(curl -sS -o "${output}" -w '%{http_code}' \
    -H "Authorization: Bearer ${key}" -H 'Content-Type: application/json' \
    --data-binary "{\"model\":\"${model}\",\"stream\":false}" \
    "http://127.0.0.1:${LISTEN_PORT}/v1/messages")"
  if [[ "${code}" != "404" ]]; then
    echo "FAIL: same-surface edge accepted another edge model (HTTP ${code})" >&2
    exit 1
  fi
}
expect_model_isolated "${ENTRY_MESSAGES}" claude-drain-model "${SCRATCH}/messages-to-drain.json"
expect_model_isolated "${ENTRY_DRAIN}" claude-stdin-model "${SCRATCH}/drain-to-messages.json"

STATUS="$(post_control "$(python3 - <<PY
import json
print(json.dumps({"type":"Status","request_id":"flow-status","instance_epoch":"${EPOCH}","owner_id":"flow-owner","owner_term":int("${TERM}"),"app_data_dir":"${HOME_DIR}","payload":{}}))
PY
)")"
printf '%s\n' "${STATUS}" >"${STATUS_LOG}"
printf '%s' "${STATUS}" | python3 -c 'import json,sys; p=json.load(sys.stdin)["payload"]; assert p["member_count"] == 4, p; assert p["healthy_member_count"] == 4, p'

request /v1/messages "${ENTRY_DRAIN}" claude-drain-model "${SCRATCH}/drain.json" &
DRAIN_CURL_PID=$!
for _ in $(seq 1 100); do
  CURRENT="$(post_control "$(python3 - <<PY
import json
print(json.dumps({"type":"Status","request_id":"flow-inflight-${RANDOM}","instance_epoch":"${EPOCH}","owner_id":"flow-owner","owner_term":int("${TERM}"),"app_data_dir":"${HOME_DIR}","payload":{}}))
PY
)")"
  if printf '%s' "${CURRENT}" | python3 -c 'import json,sys; raise SystemExit(0 if json.load(sys.stdin)["payload"]["in_flight_count"] == 1 else 1)'; then
    break
  fi
  sleep 0.05
done
printf '%s' "${CURRENT}" | python3 -c 'import json,sys; assert json.load(sys.stdin)["payload"]["in_flight_count"] == 1'

STOP="$(post_control "$(python3 - <<PY
import json
print(json.dumps({"type":"Stop","request_id":"flow-stop","instance_epoch":"${EPOCH}","owner_id":"flow-owner","owner_term":int("${TERM}"),"app_data_dir":"${HOME_DIR}","payload":{}}))
PY
)")"
printf '%s' "${STOP}" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d["ok"], d; assert d["payload"]["lifecycle"] == "draining", d'

set +e
NEW_CODE="$(curl -sS --max-time 1 -o "${SCRATCH}/during-drain.json" -w '%{http_code}' \
  -H "Authorization: Bearer ${ENTRY_MESSAGES}" -H 'Content-Type: application/json' \
  --data-binary '{"model":"claude-stdin-model"}' "http://127.0.0.1:${LISTEN_PORT}/v1/messages")"
NEW_CURL_STATUS=$?
set -e
if [[ ${NEW_CURL_STATUS} -eq 0 && "${NEW_CODE}" != "503" ]]; then
  echo "FAIL: new request accepted during drain (HTTP ${NEW_CODE})" >&2
  exit 1
fi

wait "${DRAIN_CURL_PID}"
grep -F -q 'isolated-messages-drained' "${SCRATCH}/drain.json"
for _ in $(seq 1 100); do
  ! kill -0 "${ADAPTERD_PID}" 2>/dev/null && break
  sleep 0.05
done
if kill -0 "${ADAPTERD_PID}" 2>/dev/null; then
  echo "FAIL: adapterd did not exit after drain" >&2
  exit 1
fi
wait "${ADAPTERD_PID}" 2>/dev/null || true
if curl -sS --max-time 1 "http://127.0.0.1:${LISTEN_PORT}/health" >/dev/null 2>&1; then
  echo "FAIL: listener still accepts requests after Stop" >&2
  exit 1
fi

for secret in "${ENTRY_MESSAGES}" "${ENTRY_RESPONSES}" "${ENTRY_CHAT}" "${ENTRY_DRAIN}" \
  "${UPSTREAM_MESSAGES}" "${UPSTREAM_RESPONSES}" "${UPSTREAM_CHAT}" "${UPSTREAM_DRAIN}" "${BODY_MARKER}"; do
  if grep -F -q -- "${secret}" "${LOG_FILE}" "${ADAPTERD_STDOUT}" "${MOCK_LOG}" "${STATUS_LOG}" 2>/dev/null; then
    echo "FAIL: secret or request body marker found in logs/status" >&2
    exit 1
  fi
done

python3 - <<PY >"${EVIDENCE}"
import json, os, platform, subprocess
print(json.dumps({
  "build_sha": subprocess.check_output(["git", "-C", "${ROOT}", "rev-parse", "HEAD"], text=True).strip(),
  "dirty": bool(subprocess.check_output(["git", "-C", "${ROOT}", "status", "--porcelain"], text=True).strip()),
  "run_id": "${RUN_ID}", "platform": platform.system(), "arch": platform.machine(),
  "runtime_config_stdin": "ok", "edge_count": 4, "messages": "ok", "responses": "ok", "chat_completions": "ok",
  "models_isolated": True, "same_surface_bidirectional_isolation": True,
  "auth_header_contracts": True, "drain": "ok", "secret_scan": "ok", "process_down": True,
  "listen_port": int("${LISTEN_PORT}"), "default_gateway_unchanged": True, "real_home_untouched": True
}, indent=2))
PY

echo "PASS: runtime config stdin, multi-edge routing, and graceful drain"
echo "scratch: ${SCRATCH}"
echo "evidence: ${EVIDENCE}"
