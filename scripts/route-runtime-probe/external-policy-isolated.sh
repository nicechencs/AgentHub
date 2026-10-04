#!/usr/bin/env bash
# Core-generated external target policy -> real Go runtime validation probe.
# It never sends a route request, resolves an upstream, or calls an external service.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
MOD="${ROOT}/go/agenthub-adapterd"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
SCRATCH_INPUT="${1:-/tmp/agenthub-route-external-policy/${RUN_ID}}"
REAL_HOME="$(cd "${HOME}" && pwd -P)"
PRODUCT_PORT=43121

mkdir -p "${SCRATCH_INPUT}"
SCRATCH="$(cd "${SCRATCH_INPUT}" && pwd -P)"
case "${SCRATCH}" in
  /tmp/agenthub-route-external-policy/*|/private/tmp/agenthub-route-external-policy/*|/var/tmp/agenthub-route-external-policy/*) ;;
  *) echo "FAIL: scratch must be under the external policy probe temp tree" >&2; exit 1 ;;
esac
case "${SCRATCH}" in
  "${REAL_HOME}"|"${REAL_HOME}"/*) echo "FAIL: refusing scratch inside the real user home" >&2; exit 1 ;;
esac

BIN_DIR="${SCRATCH}/bin"
CORE_BIN="${ROOT}/target/debug/examples/go_route_external_config_probe"
GO_BIN="${BIN_DIR}/agenthub-adapterd"
BUILD_LOG="${SCRATCH}/build.log"
ACCEPTED_ROWS="${SCRATCH}/accepted.tsv"
DENIED_ROWS="${SCRATCH}/denied.txt"
EVIDENCE="${SCRATCH}/evidence.json"
mkdir -p "${BIN_DIR}"
: >"${BUILD_LOG}"
: >"${ACCEPTED_ROWS}"
: >"${DENIED_ROWS}"

ALLOWED_CASES=(
  anthropic_api_key
  openai_api_key
  openai_chat_api_key
  kimi_api_key
  kimi_chat_api_key
  codex_official_login
  codex_official_login_to_grok
  grok_official_login
)
DENIED_CASES=(
  codex_official_login_missing_account_id
  codex_official_login_to_grok_flag_off
  grok_official_login_flag_off
  kimi_oauth
  custom_relay
  moonshot_api_key
  xai_api_key
  anthropic_evil_host
  anthropic_wrong_port
  openai_evil_host
  openai_query
)
SYNTHETIC_SECRETS=(
  sk_probe_external_policy_do_not_use
  oauth_probe_external_policy_access_do_not_use
  oauth_probe_external_policy_refresh_do_not_use
  acct_probe_external_policy_do_not_use
)
ACTIVE_PIDS=()

cleanup() {
  local code=$? pid
  trap - EXIT
  set +e
  for pid in "${ACTIVE_PIDS[@]}"; do
    [[ -n "${pid}" ]] || continue
    if kill -0 "${pid}" 2>/dev/null; then
      kill "${pid}" 2>/dev/null || true
      wait "${pid}" 2>/dev/null || true
    fi
  done
  if [[ ${code} -ne 0 ]]; then
    echo "FAIL: external policy probe exited ${code}; scratch retained at ${SCRATCH}" >&2
  fi
  exit "${code}"
}
trap cleanup EXIT

source_fingerprint() {
  python3 - "${ROOT}" <<'PY'
import hashlib
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
paths = [
    root / "crates/agenthub-core/src/services/adapter_bridge_service/go_route_config.rs",
    root / "crates/agenthub-core/src/services/adapter_secret_resolver/validate.rs",
    root / "crates/agenthub-core/src/services/account_quota.rs",
    root / "crates/agenthub-core/examples/go_route_external_config_probe.rs",
    root / "go/agenthub-adapterd/go.mod",
    root / "scripts/route-runtime-probe/external-policy-isolated.sh",
]
paths.extend(sorted((root / "go/agenthub-adapterd").glob("*.go")))
digest = hashlib.sha256()
for path in paths:
    relative = path.relative_to(root).as_posix().encode()
    digest.update(len(relative).to_bytes(4, "big"))
    digest.update(relative)
    data = path.read_bytes()
    digest.update(len(data).to_bytes(8, "big"))
    digest.update(data)
print(digest.hexdigest())
PY
}

pick_port() {
  python3 - <<'PY'
import socket
sock = socket.socket()
sock.bind(("127.0.0.1", 0))
print(sock.getsockname()[1])
sock.close()
PY
}

scan_outputs() {
  local secret candidate
  for secret in "${SYNTHETIC_SECRETS[@]}"; do
    while IFS= read -r -d '' candidate; do
      if grep -aF -q -- "${secret}" "${candidate}"; then
        echo "FAIL: synthetic login information appeared in probe output" >&2
        return 1
      fi
    done < <(find "${SCRATCH}" -type f \( -name '*.log' -o -name '*.json' -o -name '*.tsv' -o -name '*.txt' -o -name '*.sha256' \) -print0)
  done
}

inspect_config() {
  local case_id="$1" case_root="$2"
  mkdir -p "${case_root}/home" "${case_root}/xdg-config" "${case_root}/xdg-data"
  AGENTHUB_HOME="${case_root}/core/data" \
  HOME="${case_root}/home" \
  XDG_CONFIG_HOME="${case_root}/xdg-config" \
  XDG_DATA_HOME="${case_root}/xdg-data" \
    "${CORE_BIN}" "${case_id}" "${case_root}/core" |
    python3 -c '
import json, sys
config = json.load(sys.stdin)
assert config["version"] == "route-config.v0-isolated", config
assert len(config["edges"]) == 1, config
edge = config["edges"][0]
assert len(edge["members"]) == 1, edge
member = edge["members"][0]
assert edge.get("codex_ingress_grok_upstream", False) is (sys.argv[1] == "grok_official_login"), edge
assert edge.get("grok_ingress_codex_upstream", False) is (sys.argv[1] == "codex_official_login_to_grok"), edge
if sys.argv[1] in {"codex_official_login", "codex_official_login_to_grok"}:
    assert member.get("official_account_id") == "acct_probe_external_policy_do_not_use", member
else:
    assert "official_account_id" not in member, member
safe = [
    sys.argv[1], member["upstream_target"], member["credential_class"],
    member["upstream_transport"], edge["surface"], member["upstream_base_url"],
]
assert all("\t" not in value and "\n" not in value for value in safe)
print("\t".join(safe))
' "${case_id}"
}

post_control() {
  local socket_path="$1" document="$2"
  curl -sS --unix-socket "${socket_path}" \
    -H 'Content-Type: application/json' \
    --data-binary "${document}" \
    http://127.0.0.1/control
}

run_allowed_case() {
  local case_id="$1"
  local short_id
  case "${case_id}" in
    anthropic_api_key) short_id=a ;;
    openai_api_key) short_id=o ;;
    openai_chat_api_key) short_id=c ;;
    kimi_api_key) short_id=k ;;
    kimi_chat_api_key) short_id=m ;;
    codex_official_login) short_id=x ;;
    codex_official_login_to_grok) short_id=y ;;
    grok_official_login) short_id=g ;;
    *) echo "FAIL: unknown allowed case ${case_id}" >&2; return 1 ;;
  esac
  # Unix control sockets have a small path limit, so active process paths stay
  # deliberately short even when the caller chose a longer scratch root.
  local inspect_root="${SCRATCH}/i/${short_id}"
  local runtime_root="${SCRATCH}/r/${short_id}"
  local home_dir="${runtime_root}/h"
  local socket_path="${home_dir}/run/adapterd.sock"
  local runtime_log="${SCRATCH}/${short_id}.runtime.log"
  local config_hash_file="${SCRATCH}/${short_id}.config.sha256"
  local port pid epoch term start status stop expected_hash

  mkdir -p \
    "${home_dir}/config" "${home_dir}/run" "${home_dir}/logs" \
    "${runtime_root}/home" "${runtime_root}/xdg-config" "${runtime_root}/xdg-data"
  inspect_config "${case_id}" "${inspect_root}" >>"${ACCEPTED_ROWS}"

  port="$(pick_port)"
  [[ "${port}" != "${PRODUCT_PORT}" ]] || { echo "FAIL: selected product default port" >&2; return 1; }

  AGENTHUB_HOME="${runtime_root}/core/data" \
  HOME="${runtime_root}/home" \
  XDG_CONFIG_HOME="${runtime_root}/xdg-config" \
  XDG_DATA_HOME="${runtime_root}/xdg-data" \
    "${CORE_BIN}" "${case_id}" "${runtime_root}/core" 2>>"${runtime_log}" |
    python3 -c '
import hashlib, pathlib, sys
raw = sys.stdin.buffer.read()
if not raw:
    raise SystemExit("core emitted no config")
pathlib.Path(sys.argv[1]).write_text(hashlib.sha256(raw).hexdigest() + "\n", encoding="ascii")
sys.stdout.buffer.write(raw)
' "${config_hash_file}" |
    AGENTHUB_HOME="${home_dir}" \
    HOME="${runtime_root}/home" \
    XDG_CONFIG_HOME="${runtime_root}/xdg-config" \
    XDG_DATA_HOME="${runtime_root}/xdg-data" \
      "${GO_BIN}" run --home "${home_dir}" --listen-port "${port}" --runtime-config-stdin \
      >"${runtime_log}" 2>&1 &
  pid=$!
  ACTIVE_PIDS+=("${pid}")

  for _ in $(seq 1 120); do
    [[ -S "${socket_path}" ]] && break
    if ! kill -0 "${pid}" 2>/dev/null; then
      echo "FAIL: ${case_id} Go runtime exited before control became ready" >&2
      sed -n '1,120p' "${runtime_log}" >&2 || true
      return 1
    fi
    sleep 0.05
  done
  [[ -S "${socket_path}" ]] || { echo "FAIL: ${case_id} control socket timeout" >&2; return 1; }
  [[ -s "${config_hash_file}" ]] || { echo "FAIL: ${case_id} config hash missing" >&2; return 1; }
  expected_hash="$(tr -d '\r\n' <"${config_hash_file}")"

  epoch="$(post_control "${socket_path}" "$(python3 - "${home_dir}" <<'PY'
import json, sys
home = sys.argv[1]
print(json.dumps({"type":"Handshake","request_id":"external-policy-handshake","app_data_dir":home,"payload":{"protocol_version":"route-runtime.v0-isolated","config_format_version":"route-config.v0-isolated","package_version":"0.0.0-isolated","app_data_dir":home}}))
PY
)" | python3 -c 'import json,sys; reply=json.load(sys.stdin); assert reply["ok"], reply; print(reply["payload"]["instance_epoch"])')"
  term="$(post_control "${socket_path}" "$(python3 - "${epoch}" "${home_dir}" <<'PY'
import json, sys
epoch, home = sys.argv[1:]
print(json.dumps({"type":"AcquireOrRenewOwner","request_id":"external-policy-acquire","instance_epoch":epoch,"owner_id":"external-policy-owner","app_data_dir":home,"payload":{"mode":"acquire","lease_budget_ms":60000}}))
PY
)" | python3 -c 'import json,sys; reply=json.load(sys.stdin); assert reply["ok"], reply; print(reply["payload"]["owner_term"])')"
  start="$(post_control "${socket_path}" "$(python3 - "${epoch}" "${term}" "${home_dir}" <<'PY'
import json, sys
epoch, term, home = sys.argv[1:]
print(json.dumps({"type":"Start","request_id":"external-policy-start","instance_epoch":epoch,"owner_id":"external-policy-owner","owner_term":int(term),"app_data_dir":home,"payload":{}}))
PY
)" )"
  printf '%s' "${start}" | python3 -c 'import json,sys; reply=json.load(sys.stdin); assert reply["ok"], reply; assert reply["payload"]["listen_ready"] is True, reply'

  status="$(post_control "${socket_path}" "$(python3 - "${epoch}" "${term}" "${home_dir}" <<'PY'
import json, sys
epoch, term, home = sys.argv[1:]
print(json.dumps({"type":"Status","request_id":"external-policy-status","instance_epoch":epoch,"owner_id":"external-policy-owner","owner_term":int(term),"app_data_dir":home,"payload":{}}))
PY
)" )"
  printf '%s' "${status}" | python3 -c '
import json, sys
reply = json.load(sys.stdin)
assert reply["ok"], reply
payload = reply["payload"]
assert payload["listen_ready"] is True, payload
assert payload["member_count"] == 1, payload
assert payload["healthy_member_count"] == 1, payload
assert payload["active_hash"] == sys.argv[1], payload
' "${expected_hash}"

  # Start only binds the local listener. No route request is sent, so no
  # external URL is resolved or dispatched during this validation probe.
  stop="$(post_control "${socket_path}" "$(python3 - "${epoch}" "${term}" "${home_dir}" <<'PY'
import json, sys
epoch, term, home = sys.argv[1:]
print(json.dumps({"type":"Stop","request_id":"external-policy-stop","instance_epoch":epoch,"owner_id":"external-policy-owner","owner_term":int(term),"app_data_dir":home,"payload":{}}))
PY
)" )"
  printf '%s' "${stop}" | python3 -c 'import json,sys; reply=json.load(sys.stdin); assert reply["ok"], reply'

  for _ in $(seq 1 120); do
    ! kill -0 "${pid}" 2>/dev/null && break
    sleep 0.05
  done
  if kill -0 "${pid}" 2>/dev/null; then
    echo "FAIL: ${case_id} Go runtime did not stop" >&2
    return 1
  fi
  wait "${pid}"
  local index
  for index in "${!ACTIVE_PIDS[@]}"; do
    if [[ "${ACTIVE_PIDS[${index}]}" == "${pid}" ]]; then
      unset "ACTIVE_PIDS[${index}]"
      break
    fi
  done
  python3 - "${port}" <<'PY'
import socket, sys
sock = socket.socket()
sock.bind(("127.0.0.1", int(sys.argv[1])))
sock.close()
PY
  [[ ! -e "${socket_path}" ]] || { echo "FAIL: ${case_id} control socket remained after stop" >&2; return 1; }
}

run_denied_case() {
  local case_id="$1"
  local case_root="${SCRATCH}/denied/${case_id}"
  local stderr_log="${case_root}/rejection.log"
  local output status
  mkdir -p "${case_root}/home" "${case_root}/xdg-config" "${case_root}/xdg-data"
  set +e
  output="$(
    AGENTHUB_HOME="${case_root}/core/data" \
    HOME="${case_root}/home" \
    XDG_CONFIG_HOME="${case_root}/xdg-config" \
    XDG_DATA_HOME="${case_root}/xdg-data" \
      "${CORE_BIN}" "${case_id}" "${case_root}/core" 2>"${stderr_log}"
  )"
  status=$?
  set -e
  [[ ${status} -eq 3 ]] || { echo "FAIL: ${case_id} returned ${status}, expected policy rejection" >&2; return 1; }
  [[ -z "${output}" ]] || { echo "FAIL: ${case_id} emitted a rejected config" >&2; return 1; }
  grep -Fx -q 'external policy case rejected' "${stderr_log}" || {
    echo "FAIL: ${case_id} did not use the generic rejection" >&2
    return 1
  }
  printf '%s\n' "${case_id}" >>"${DENIED_ROWS}"
}

SOURCE_FINGERPRINT_BEFORE="$(source_fingerprint)"
(cd "${ROOT}" && cargo build -p agenthub-core --example go_route_external_config_probe --locked) >>"${BUILD_LOG}" 2>&1
[[ -x "${CORE_BIN}" ]] || { echo "FAIL: core probe executable missing" >&2; exit 1; }
(cd "${MOD}" && go build -o "${GO_BIN}" .) >>"${BUILD_LOG}" 2>&1
[[ -x "${GO_BIN}" ]] || { echo "FAIL: Go runtime executable missing" >&2; exit 1; }

for case_id in "${ALLOWED_CASES[@]}"; do
  run_allowed_case "${case_id}"
done
for case_id in "${DENIED_CASES[@]}"; do
  run_denied_case "${case_id}"
done

SOURCE_FINGERPRINT_AFTER="$(source_fingerprint)"
[[ "${SOURCE_FINGERPRINT_BEFORE}" == "${SOURCE_FINGERPRINT_AFTER}" ]] || {
  echo "FAIL: source changed while the probe was running" >&2
  exit 1
}

python3 - \
  "${ACCEPTED_ROWS}" "${DENIED_ROWS}" "${EVIDENCE}" \
  "${SOURCE_FINGERPRINT_AFTER}" "$(git -C "${ROOT}" rev-parse HEAD)" <<'PY'
import json
import pathlib
import platform
import sys

accepted_path, denied_path, evidence_path, fingerprint, commit = sys.argv[1:]
rows = [line.rstrip("\n").split("\t") for line in pathlib.Path(accepted_path).read_text().splitlines()]
expected = {
    "anthropic_api_key": ["anthropic_api", "api_key", "anthropic_messages", "messages", "https://api.anthropic.com/v1"],
    "openai_api_key": ["openai_api", "api_key", "openai_chat_completions", "responses", "https://api.openai.com/v1"],
    "openai_chat_api_key": ["openai_api", "api_key", "openai_chat_completions", "chat_completions", "https://api.openai.com/v1"],
    "kimi_api_key": ["kimi_code_membership", "api_key", "openai_chat_completions", "responses", "https://api.kimi.com/coding/v1"],
    "kimi_chat_api_key": ["kimi_code_membership", "api_key", "openai_chat_completions", "chat_completions", "https://api.kimi.com/coding/v1"],
    "codex_official_login": ["codex_chatgpt_subscription", "official_login", "codex_responses", "responses", "https://chatgpt.com/backend-api/codex"],
    "codex_official_login_to_grok": ["codex_chatgpt_subscription", "official_login", "codex_responses", "responses", "https://chatgpt.com/backend-api/codex"],
    "grok_official_login": ["grok_xai_subscription", "official_login", "grok_responses", "responses", "https://cli-chat-proxy.grok.com/v1"],
}
actual = {row[0]: row[1:] for row in rows}
assert actual == expected, (actual, expected)
denied = pathlib.Path(denied_path).read_text().splitlines()
expected_denied = [
    "codex_official_login_missing_account_id", "codex_official_login_to_grok_flag_off",
    "grok_official_login_flag_off", "kimi_oauth",
    "custom_relay", "moonshot_api_key", "xai_api_key",
    "anthropic_evil_host", "anthropic_wrong_port", "openai_evil_host", "openai_query",
]
assert denied == expected_denied, (denied, expected_denied)
evidence = {
    "schema": "go-route-external-policy-probe.v1",
    "status": "ok",
    "commit": commit,
    "platform": platform.system(),
    "arch": platform.machine(),
    "source_fingerprint_before_and_after": fingerprint,
    "core_generated_config_piped_to_real_go": True,
    "go_runtime_validation": "ok",
    "go_start_validation_only": True,
    "route_requests_sent": 0,
    "external_requests": 0,
    "allowed_cases": sorted(actual),
    "allowed_case_count": len(actual),
    "denied_cases": denied,
    "denied_case_count": len(denied),
    "config_hash_confirmed_by_go_status": True,
    "processes_stopped": True,
    "ports_released": True,
    "real_home_untouched": True,
    "secret_scan": "ok",
    "real_external_service_validation": "not_run",
}
pathlib.Path(evidence_path).write_text(json.dumps(evidence, ensure_ascii=False, sort_keys=True) + "\n")
PY

scan_outputs
echo "PASS: core external-target policy configs were validated by the real Go runtime without external requests"
echo "Evidence: ${EVIDENCE}"
