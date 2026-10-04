#!/usr/bin/env bash
# Exercise AgentHub's plugin write API against real Codex and Pi CLIs without
# touching either tool's actual user directory or using a network package.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
FIXTURES="${ROOT}/scripts/plugin-probe/fixtures"
CODEX_MARKET="${FIXTURES}/codex-marketplace"
PI_PACKAGE="${FIXTURES}/pi-package"
BASE="/tmp/agenthub-plugin-write-probe"

mkdir -p "${BASE}"
SCRATCH="$(mktemp -d "${BASE}/run.XXXXXX")"
SCRATCH="$(cd "${SCRATCH}" && pwd -P)"
case "${SCRATCH}" in
  "${BASE}"/run.*) ;;
  *) echo "FAIL: mktemp returned an unexpected directory" >&2; exit 1 ;;
esac

on_exit() {
  local status=$?
  if [[ "${status}" -ne 0 ]]; then
    echo "FAIL: isolated scratch retained at ${SCRATCH}" >&2
  fi
}
trap on_exit EXIT

HOME_DIR="${SCRATCH}/home"
CODEX_HOME_DIR="${SCRATCH}/codex"
PI_HOME_DIR="${SCRATCH}/pi-agent"
WORK_DIR="${SCRATCH}/work"
XDG_CONFIG_DIR="${SCRATCH}/xdg-config"
XDG_DATA_DIR="${SCRATCH}/xdg-data"
XDG_CACHE_DIR="${SCRATCH}/xdg-cache"
TMP_DIR="${SCRATCH}/tmp"
BUILD_LOG="${SCRATCH}/build.log"
RUN_LOG="${SCRATCH}/run.json"
EVIDENCE="${SCRATCH}/evidence.json"

mkdir -p \
  "${HOME_DIR}" \
  "${CODEX_HOME_DIR}" \
  "${PI_HOME_DIR}" \
  "${WORK_DIR}" \
  "${XDG_CONFIG_DIR}" \
  "${XDG_DATA_DIR}" \
  "${XDG_CACHE_DIR}" \
  "${TMP_DIR}"

for fixture in \
  "${CODEX_MARKET}/.agents/plugins/marketplace.json" \
  "${CODEX_MARKET}/plugin/.codex-plugin/plugin.json" \
  "${PI_PACKAGE}/package.json"; do
  [[ -f "${fixture}" ]] || { echo "FAIL: fixture missing" >&2; exit 1; }
done

CODEX_BIN="$(command -v codex)"
PI_BIN="$(command -v pi)"
[[ -x "${CODEX_BIN}" ]] || { echo "FAIL: Codex command is unavailable" >&2; exit 1; }
[[ -x "${PI_BIN}" ]] || { echo "FAIL: Pi command is unavailable" >&2; exit 1; }
CODEX_VERSION="$(${CODEX_BIN} --version | tr -d '\r\n')"
PI_VERSION="$(${PI_BIN} --version | tr -d '\r\n')"
[[ "${CODEX_VERSION}" == codex-cli\ * ]] || { echo "FAIL: unexpected Codex version output" >&2; exit 1; }
[[ "${PI_VERSION}" =~ ^[0-9]+\.[0-9]+\.[0-9]+ ]] || { echo "FAIL: unexpected Pi version output" >&2; exit 1; }

# All vendor and core probe processes receive only disposable homes. Offline
# flags and dead loopback proxies make accidental remote package resolution fail.
ISOLATED_ENV=(
  env -i
  "PATH=${PATH}"
  "HOME=${HOME_DIR}"
  "CODEX_HOME=${CODEX_HOME_DIR}"
  "PI_CODING_AGENT_DIR=${PI_HOME_DIR}"
  "PI_CODING_AGENT_SESSION_DIR=${SCRATCH}/pi-sessions"
  "PI_OFFLINE=1"
  "PI_TELEMETRY=0"
  "XDG_CONFIG_HOME=${XDG_CONFIG_DIR}"
  "XDG_DATA_HOME=${XDG_DATA_DIR}"
  "XDG_CACHE_HOME=${XDG_CACHE_DIR}"
  "TMPDIR=${TMP_DIR}"
  "GIT_CONFIG_NOSYSTEM=1"
  "GIT_TERMINAL_PROMPT=0"
  "HTTP_PROXY=http://127.0.0.1:9"
  "HTTPS_PROXY=http://127.0.0.1:9"
  "ALL_PROXY=http://127.0.0.1:9"
  "NO_PROXY=127.0.0.1,localhost"
)

safe_remove() {
  local target="$1"
  case "${target}" in
    "${SCRATCH}"/*) rm -f -- "${target}" ;;
    *) echo "FAIL: refusing to delete outside validated mktemp" >&2; return 1 ;;
  esac
}

run_codex_json() {
  local stdout_file="$1"
  local stderr_file="$2"
  shift 2
  "${ISOLATED_ENV[@]}" "${CODEX_BIN}" "$@" >"${stdout_file}" 2>"${stderr_file}"
  python3 -m json.tool "${stdout_file}" >/dev/null
}

MARKET_ADD_JSON="${SCRATCH}/market-add.json"
MARKET_ADD_ERR="${SCRATCH}/market-add.err"
run_codex_json "${MARKET_ADD_JSON}" "${MARKET_ADD_ERR}" \
  plugin marketplace add "${CODEX_MARKET}" --json
python3 - "${MARKET_ADD_JSON}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    value = json.load(handle)
assert value["marketplaceName"] == "agenthub-probe-market", value
assert value["alreadyAdded"] is False, value
PY

(cd "${ROOT}" && cargo build -p agenthub-core --example plugin_write_probe --locked) \
  >"${BUILD_LOG}" 2>&1
PROBE_BIN="${ROOT}/target/debug/examples/plugin_write_probe"
[[ -x "${PROBE_BIN}" ]] || { echo "FAIL: plugin probe executable missing" >&2; exit 1; }

(cd "${WORK_DIR}" && \
  "${ISOLATED_ENV[@]}" \
  "AGENTHUB_PLUGIN_PROBE_CODEX_BIN=${CODEX_BIN}" \
  "AGENTHUB_PLUGIN_PROBE_PI_BIN=${PI_BIN}" \
  "AGENTHUB_PLUGIN_PROBE_PI_PACKAGE=${PI_PACKAGE}" \
  "${PROBE_BIN}") >"${RUN_LOG}" 2>&1

python3 - "${RUN_LOG}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    lines = [line.strip() for line in handle if line.strip()]
assert len(lines) == 1, lines
value = json.loads(lines[0])
assert value["schema"] == "plugin-write-probe.v1", value
assert value["status"] == "ok", value
for section, fields in {
    "codex": (
        "available_listed",
        "preview_matches",
        "confirmation_guarded",
        "invalid_source_rejected",
        "installed_listed",
        "disabled_listed",
        "enabled_listed",
        "failed_install_rolled_back",
        "removed",
    ),
    "pi": (
        "preview_matches",
        "relative_local_rejected",
        "confirmation_guarded",
        "invalid_source_rejected",
        "installed_listed",
        "inventory_source_exact",
        "toggle_rejected",
        "failed_install_rolled_back",
        "removed",
    ),
}.items():
    assert set(value[section]) == set(fields), value[section]
    for field in fields:
        assert value[section][field] is True, (section, field, value)
PY

MARKET_REMOVE_JSON="${SCRATCH}/market-remove.json"
MARKET_REMOVE_ERR="${SCRATCH}/market-remove.err"
run_codex_json "${MARKET_REMOVE_JSON}" "${MARKET_REMOVE_ERR}" \
  plugin marketplace remove agenthub-probe-market --json
python3 - "${MARKET_REMOVE_JSON}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    value = json.load(handle)
assert value == {"marketplaceName": "agenthub-probe-market", "installedRoot": None}, value
PY

FINAL_CODEX_JSON="${SCRATCH}/codex-final.json"
FINAL_CODEX_ERR="${SCRATCH}/codex-final.err"
run_codex_json "${FINAL_CODEX_JSON}" "${FINAL_CODEX_ERR}" plugin list --json
python3 - "${FINAL_CODEX_JSON}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    value = json.load(handle)
assert value.get("installed") == [], value
PY

FINAL_PI_OUT="${SCRATCH}/pi-final.out"
FINAL_PI_ERR="${SCRATCH}/pi-final.err"
(cd "${WORK_DIR}" && "${ISOLATED_ENV[@]}" "${PI_BIN}" list --no-approve) \
  >"${FINAL_PI_OUT}" 2>"${FINAL_PI_ERR}"
grep -F -q 'No packages installed.' "${FINAL_PI_OUT}"
python3 - "${PI_HOME_DIR}/settings.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    value = json.load(handle)
assert value.get("packages") == [], value
PY

if [[ -n "$(find "${CODEX_HOME_DIR}/plugins/cache" -type f -print -quit 2>/dev/null)" ]]; then
  echo "FAIL: Codex plugin cache was not removed" >&2
  exit 1
fi
if [[ -n "$(find "${HOME_DIR}" -mindepth 1 -print -quit)" ]]; then
  echo "FAIL: a tool polluted the isolated default user directory" >&2
  exit 1
fi

python3 - \
  "${RUN_LOG}" \
  "${EVIDENCE}" \
  "${CODEX_VERSION}" \
  "${PI_VERSION}" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    value = json.load(handle)
value["codex_cli_version"] = sys.argv[3]
value["pi_cli_version"] = sys.argv[4]
value["local_fixtures_only"] = True
value["network_package_resolution_blocked"] = True
value["isolated_user_directories"] = True
value["final_state_clean"] = True
with open(sys.argv[2], "w", encoding="utf-8") as handle:
    json.dump(value, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY

if grep -E -q '/workspace/|/home/|/tmp/|sk-[A-Za-z0-9]|api[_-]?key|bearer' "${EVIDENCE}"; then
  echo "FAIL: evidence contains a path or secret-shaped value" >&2
  exit 1
fi

for transient in \
  "${BUILD_LOG}" \
  "${RUN_LOG}" \
  "${MARKET_ADD_JSON}" \
  "${MARKET_ADD_ERR}" \
  "${MARKET_REMOVE_JSON}" \
  "${MARKET_REMOVE_ERR}" \
  "${FINAL_CODEX_JSON}" \
  "${FINAL_CODEX_ERR}" \
  "${FINAL_PI_OUT}" \
  "${FINAL_PI_ERR}"; do
  safe_remove "${transient}"
done

echo "PASS: real Codex and Pi plugin write lifecycle"
echo "evidence: ${EVIDENCE}"
