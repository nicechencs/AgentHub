#!/usr/bin/env bash
# Desktop plan/bind -> isolated Go -> unbind real-process probe.

set -euo pipefail
umask 077

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
[[ $# -eq 0 ]] || { echo "FAIL: this probe does not accept a caller-selected scratch path" >&2; exit 1; }
REAL_HOME="$(cd "${HOME}" && pwd -P)"
ORIGINAL_CARGO_HOME="${CARGO_HOME:-${REAL_HOME}/.cargo}"
ORIGINAL_RUSTUP_HOME="${RUSTUP_HOME:-${REAL_HOME}/.rustup}"
PRODUCT_PORT=43121
SOURCE_KEY="sk-agenthub-bind-go-probe-do-not-use-000000"
ORIGINAL_KEY="probe-original-local-key"

SCRATCH_INPUT="$(mktemp -d /tmp/agenthub-bind-go-e2e.XXXXXXXXXX)"
SCRATCH="$(cd "${SCRATCH_INPUT}" && pwd -P)"
case "${SCRATCH}" in
  /tmp/agenthub-bind-go-e2e.*|/private/tmp/agenthub-bind-go-e2e.*) ;;
  *) echo "FAIL: scratch must be inside the bind-Go probe temp tree" >&2; exit 1 ;;
esac
case "${SCRATCH}" in
  "${REAL_HOME}"|"${REAL_HOME}"/*) echo "FAIL: refusing scratch inside the real user home" >&2; exit 1 ;;
esac

HOME_DIR="${SCRATCH}/home"
AGENTHUB_DIR="${SCRATCH}/agenthub"
CODEX_DIR="${SCRATCH}/codex"
XDG_CONFIG_DIR="${SCRATCH}/xdg-config"
XDG_DATA_DIR="${SCRATCH}/xdg-data"
XDG_CACHE_DIR="${SCRATCH}/xdg-cache"
XDG_STATE_DIR="${SCRATCH}/xdg-state"
BUILD_LOG="${SCRATCH}/build.log"
GO_BUILD_LOG="${SCRATCH}/go-build.log"
RUN_LOG="${SCRATCH}/run.log"
MOCK_LOG="${SCRATCH}/mock.log"
MOCK_HIT="${SCRATCH}/mock-hit.txt"
EVIDENCE="${SCRATCH}/evidence.json"
ADAPTERD_BIN="${SCRATCH}/agenthub-adapterd"
TOOLING_BEFORE="${SCRATCH}/real-tooling-before.txt"
TOOLING_AFTER="${SCRATCH}/real-tooling-after.txt"
MOCK_PID=""
PROBE_PID=""

mkdir -p \
  "${HOME_DIR}" "${AGENTHUB_DIR}" "${CODEX_DIR}" \
  "${XDG_CONFIG_DIR}" "${XDG_DATA_DIR}" "${XDG_CACHE_DIR}" "${XDG_STATE_DIR}"

snapshot_real_tooling() {
  for root in "${ORIGINAL_CARGO_HOME}" "${ORIGINAL_RUSTUP_HOME}"; do
    if [[ -e "${root}" ]]; then
      find "${root}" -xdev -printf '%P\t%y\t%m\t%s\t%T@\t%C@\n'
    fi
  done | LC_ALL=C sort
}
snapshot_real_tooling >"${TOOLING_BEFORE}"
TOOLCHAIN_SYSROOT="$(rustc --print sysroot)"
TOOLCHAIN_BIN="${TOOLCHAIN_SYSROOT}/bin"

[[ -d "${ORIGINAL_CARGO_HOME}/registry" ]] || { echo "FAIL: preloaded Cargo registry is missing" >&2; exit 1; }
CARGO_HOME_DIR="${SCRATCH}/cargo-home"
mkdir -p "${CARGO_HOME_DIR}/registry"
ln -s "${ORIGINAL_CARGO_HOME}/registry/index" "${CARGO_HOME_DIR}/registry/index"
ln -s "${ORIGINAL_CARGO_HOME}/registry/cache" "${CARGO_HOME_DIR}/registry/cache"
python3 - "${CARGO_HOME_DIR}/config.toml" <<'PY'
import sys
path = sys.argv[1]
with open(path, "w", encoding="utf-8") as handle:
    handle.write("[net]\noffline = true\n")
PY

export HOME="${HOME_DIR}"
export AGENTHUB_HOME="${AGENTHUB_DIR}"
export CODEX_HOME="${CODEX_DIR}"
export XDG_CONFIG_HOME="${XDG_CONFIG_DIR}"
export XDG_DATA_HOME="${XDG_DATA_DIR}"
export XDG_CACHE_HOME="${XDG_CACHE_DIR}"
export XDG_STATE_HOME="${XDG_STATE_DIR}"
export GOCACHE="${XDG_CACHE_DIR}/go-build"
export GOPATH="${SCRATCH}/go-path"
export CARGO_HOME="${CARGO_HOME_DIR}"
unset RUSTUP_HOME
export PATH="${TOOLCHAIN_BIN}:${PATH}"
export CARGO_NET_OFFLINE=true

pick_port() {
  python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
}

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  if [[ -n "${PROBE_PID}" ]] && kill -0 "${PROBE_PID}" 2>/dev/null; then
    kill -TERM -- "-${PROBE_PID}" 2>/dev/null || true
    for _ in $(seq 1 40); do
      kill -0 "${PROBE_PID}" 2>/dev/null || break
      sleep 0.05
    done
    kill -KILL -- "-${PROBE_PID}" 2>/dev/null || true
    wait "${PROBE_PID}" 2>/dev/null || true
  fi
  if [[ -n "${MOCK_PID}" ]] && kill -0 "${MOCK_PID}" 2>/dev/null; then
    kill "${MOCK_PID}" 2>/dev/null || true
    wait "${MOCK_PID}" 2>/dev/null || true
  fi
  exit "${status}"
}
trap cleanup EXIT INT TERM

UPSTREAM_PORT="$(pick_port)"
[[ "${UPSTREAM_PORT}" != "${PRODUCT_PORT}" ]] || { echo "FAIL: mock selected product port" >&2; exit 1; }

AGENTHUB_MOCK_SOURCE_KEY="${SOURCE_KEY}" python3 -u - "${UPSTREAM_PORT}" "${MOCK_HIT}" <<'PY' >"${MOCK_LOG}" 2>&1 &
import json, os, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

port, hit_path = int(sys.argv[1]), sys.argv[2]
expected_key = os.environ.pop("AGENTHUB_MOCK_SOURCE_KEY")

class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *_):
        pass
    def send_json(self, status, value):
        raw = json.dumps(value, separators=(",", ":")).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)
    def do_GET(self):
        if self.path.rstrip("/").endswith("/models"):
            self.send_json(200, {"object":"list","data":[{"id":"gpt-4o","object":"model"}]})
        else:
            self.send_json(404, {"error":"not_found"})
    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length)
        if self.path != "/v1/chat/completions" or self.headers.get("Authorization") != "Bearer " + expected_key:
            self.send_json(401, {"error":"unauthorized"})
            return
        try:
            body = json.loads(raw)
        except Exception:
            self.send_json(400, {"error":"invalid_json"})
            return
        if body.get("model") != "gpt-4o":
            self.send_json(400, {"error":"wrong_model"})
            return
        with open(hit_path, "w", encoding="utf-8") as handle:
            handle.write("authorized_chat_completion\n")
        self.send_json(200, {
            "id":"chatcmpl-bind-go-probe",
            "object":"chat.completion",
            "created":1,
            "model":"gpt-4o",
            "choices":[{"index":0,"message":{"role":"assistant","content":"agenthub-bind-go-upstream-marker"},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}
        })

ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
PY
MOCK_PID=$!

for _ in $(seq 1 100); do
  if python3 - "${UPSTREAM_PORT}" <<'PY'
import socket, sys
try:
    with socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=.1):
        pass
except OSError:
    raise SystemExit(1)
PY
  then
    break
  fi
  sleep 0.05
done
kill -0 "${MOCK_PID}" 2>/dev/null || { echo "FAIL: mock did not start" >&2; exit 1; }
if [[ -r "/proc/${MOCK_PID}/cmdline" ]] && tr '\0' '\n' <"/proc/${MOCK_PID}/cmdline" | grep -F -q -- "${SOURCE_KEY}"; then
  echo "FAIL: synthetic source API Key appeared in mock process arguments" >&2
  exit 1
fi

(cd "${ROOT}/go/agenthub-adapterd" && go build -trimpath -buildvcs=false -o "${ADAPTERD_BIN}" .) >"${GO_BUILD_LOG}" 2>&1
(cd "${ROOT}" && node scripts/build-go-sidecar.mjs) >>"${GO_BUILD_LOG}" 2>&1
(cd "${ROOT}" && cargo build -p agenthub-gui --example go_route_bind_e2e_probe --features go-route-bind-probe --locked) >"${BUILD_LOG}" 2>&1
BIN="${ROOT}/target/debug/examples/go_route_bind_e2e_probe"
[[ -x "${BIN}" && -x "${ADAPTERD_BIN}" ]] || { echo "FAIL: probe binaries missing" >&2; exit 1; }

SOURCE_FINGERPRINT="$(
  {
    git -C "${ROOT}" rev-parse HEAD
    sha256sum \
      "${ROOT}/src-tauri/Cargo.toml" \
      "${ROOT}/src-tauri/src/lib.rs" \
      "${ROOT}/src-tauri/src/go_route_bind_probe.rs" \
      "${ROOT}/src-tauri/src/commands/adapter.rs" \
      "${ROOT}/src-tauri/src/commands/provider.rs" \
      "${ROOT}/src-tauri/src/go_route_isolated.rs" \
      "${ROOT}/src-tauri/examples/go_route_bind_e2e_probe.rs" \
      "${ROOT}/scripts/route-runtime-probe/bind-go-e2e-isolated.sh"
    find "${ROOT}/go/agenthub-adapterd" -maxdepth 1 -type f -name '*.go' -print0 \
      | sort -z \
      | xargs -0 sha256sum
  } | sha256sum | awk '{print $1}'
)"

setsid env \
  AGENTHUB_PROBE_REAL_HOME="${REAL_HOME}" \
  AGENTHUB_PROBE_SOURCE_FINGERPRINT="${SOURCE_FINGERPRINT}" \
  AGENTHUB_ADAPTERD_BIN="${ADAPTERD_BIN}" \
  HOME="${HOME_DIR}" \
  AGENTHUB_HOME="${AGENTHUB_DIR}" \
  CODEX_HOME="${CODEX_DIR}" \
  XDG_CONFIG_HOME="${XDG_CONFIG_DIR}" \
  XDG_DATA_HOME="${XDG_DATA_DIR}" \
  XDG_CACHE_HOME="${XDG_CACHE_DIR}" \
  XDG_STATE_HOME="${XDG_STATE_DIR}" \
  "${BIN}" "${SCRATCH}" "http://127.0.0.1:${UPSTREAM_PORT}" >"${RUN_LOG}" 2>&1 &
PROBE_PID=$!
probe_deadline=$((SECONDS + 120))
while kill -0 "${PROBE_PID}" 2>/dev/null; do
  if (( SECONDS >= probe_deadline )); then
    echo "FAIL: bind-Go probe timed out" >&2
    exit 1
  fi
  sleep 0.1
done
if ! wait "${PROBE_PID}"; then
  PROBE_PID=""
  tail -80 "${RUN_LOG}" >&2 || true
  exit 1
fi
PROBE_PID=""

snapshot_real_tooling >"${TOOLING_AFTER}"
cmp -s "${TOOLING_BEFORE}" "${TOOLING_AFTER}" || {
  echo "FAIL: probe modified the real Cargo/Rustup homes" >&2
  exit 1
}

python3 - "${RUN_LOG}" "${EVIDENCE}" "${PRODUCT_PORT}" <<'PY'
import json, sys
run_log, evidence_path, product_port = sys.argv[1], sys.argv[2], int(sys.argv[3])
with open(run_log, encoding="utf-8") as handle:
    rows = [line.strip() for line in handle if line.strip().startswith("{")]
assert rows, "probe emitted no JSON evidence"
evidence = json.loads(rows[-1])
assert evidence["schema"] == "go-route-bind-e2e-probe.v1", evidence
assert evidence["status"] == "ok", evidence
assert evidence["plan_route"] == "local_bridge", evidence
assert evidence["rule_id"] == "openai-api-to-codex-v1", evidence
assert not evidence["first_bind_active"] and evidence["generated_provider_switched_current"], evidence
assert evidence["persisted_pool_enrolled"] and evidence["persisted_pool_member_matches_source"], evidence
assert evidence["persisted_pool_ingress_key_matches_request"], evidence
assert evidence["http_status"] == 200 and evidence["upstream_marker_seen"], evidence
assert evidence["responses_conversion_seen"] and evidence["codex_bytes_restored"], evidence
assert evidence["original_provider_restored_current"], evidence
assert evidence["generated_profile_removed"] and evidence["generated_provider_removed"], evidence
assert evidence["rust_listener_stopped"] and evidence["go_port_released"], evidence
assert evidence["go_required_reload_ack_count_after_unbind"] == evidence["go_required_reload_ack_count_before_unbind"] + 1, evidence
assert evidence["go_state_after_unbind"] == "ready" and evidence["go_port_stable_after_reload"], evidence
assert evidence["go_member_count_before_unbind"] >= 1 and evidence["go_healthy_member_count_before_unbind"] >= 1, evidence
assert evidence["rust_listener_port"] != product_port and evidence["go_port"] != product_port, evidence
evidence["real_tooling_homes_unchanged"] = True
for key, value in evidence.items():
    if key.endswith("fingerprint") or key.startswith("go_hash"):
        assert isinstance(value, str) and len(value) == 64, (key, value)
with open(evidence_path, "w", encoding="utf-8") as handle:
    json.dump(evidence, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY

grep -F -q 'authorized_chat_completion' "${MOCK_HIT}" || { echo "FAIL: mock did not authenticate the source API Key" >&2; exit 1; }

for secret in "${SOURCE_KEY}" "${ORIGINAL_KEY}"; do
  if grep -F -q -- "${secret}" "${RUN_LOG}" "${BUILD_LOG}" "${GO_BUILD_LOG}" "${MOCK_LOG}" "${EVIDENCE}"; then
    echo "FAIL: synthetic key escaped into logs or evidence" >&2
    exit 1
  fi
done
if grep -F -q -- "${SCRATCH}" "${EVIDENCE}" || grep -F -q -- "${REAL_HOME}" "${EVIDENCE}"; then
  echo "FAIL: local path escaped into evidence" >&2
  exit 1
fi

echo "PASS: desktop bind -> Go -> unbind isolated probe"
echo "evidence: ${EVIDENCE}"
