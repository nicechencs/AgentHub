#!/usr/bin/env bash
# Run the desktop plugin write probe against deterministic offline vendor CLIs.
# No real vendor command, user home, package manager, git remote, or network is used.
# This is command-level isolation, not an OS network namespace.

set -euo pipefail
umask 077

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
SCRIPT_DIR="${ROOT}/scripts/plugin-probe"
SOURCE_FIXTURES="${SCRIPT_DIR}/fixtures"
FAKE_CLI="${SCRIPT_DIR}/fake-vendor-cli.py"
BASE="/tmp/agenthub-plugin-four-vendor"

[[ -f "${FAKE_CLI}" ]] || { echo "FAIL: fake vendor CLI is missing" >&2; exit 1; }
for required in \
  "${SOURCE_FIXTURES}/claude-plugin/plugin.json" \
  "${SOURCE_FIXTURES}/codex-marketplace/.agents/plugins/marketplace.json" \
  "${SOURCE_FIXTURES}/codex-marketplace/plugin/.codex-plugin/plugin.json" \
  "${SOURCE_FIXTURES}/grok-plugin/plugin.json" \
  "${SOURCE_FIXTURES}/pi-package/package.json"; do
  [[ -f "${required}" ]] || { echo "FAIL: fixture missing: ${required##*/}" >&2; exit 1; }
done

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

BIN_DIR="${SCRATCH}/bin"
HOME_DIR="${SCRATCH}/home"
AGENTHUB_HOME_DIR="${SCRATCH}/agenthub"
CLAUDE_HOME="${SCRATCH}/claude"
CODEX_HOME_DIR="${SCRATCH}/codex"
GROK_HOME_DIR="${SCRATCH}/grok"
PI_HOME_DIR="${SCRATCH}/pi-agent"
FIXTURE_ROOT="${SCRATCH}/fixture-state"
FIXTURE_DIR="${SCRATCH}/fixtures"
INVOCATION_LOG="${SCRATCH}/fixture-invocations.ndjson"
BUILD_LOG="${SCRATCH}/build.log"
RUN_LOG="${SCRATCH}/run.log"
EVIDENCE="${SCRATCH}/evidence.json"

mkdir -p \
  "${BIN_DIR}" "${HOME_DIR}" "${AGENTHUB_HOME_DIR}" "${CLAUDE_HOME}" "${CODEX_HOME_DIR}" \
  "${GROK_HOME_DIR}" "${PI_HOME_DIR}" "${FIXTURE_ROOT}" "${FIXTURE_DIR}" \
  "${SCRATCH}/xdg-config" "${SCRATCH}/xdg-data" "${SCRATCH}/xdg-cache" \
  "${SCRATCH}/tmp" "${SCRATCH}/work"
cp -R "${SOURCE_FIXTURES}/." "${FIXTURE_DIR}/"
for vendor in claude codex grok pi; do
  ln -s "${FAKE_CLI}" "${BIN_DIR}/${vendor}"
done
PYTHON_BIN="$(command -v python3)"
[[ "${PYTHON_BIN}" = /* && -x "${PYTHON_BIN}" ]] || {
  echo "FAIL: python3 executable not found" >&2
  exit 1
}
ln -s "${PYTHON_BIN}" "${BIN_DIR}/python3"

# Build before clearing the environment. --offline makes accidental dependency
# resolution fail instead of reaching the network.
(cd "${ROOT}" && CARGO_NET_OFFLINE=true cargo build \
  -p agenthub-gui --example plugin_write_e2e_probe \
  --features plugin-write-probe --locked --offline) \
  >"${BUILD_LOG}" 2>&1
PROBE_BIN="${ROOT}/target/debug/examples/plugin_write_e2e_probe"
[[ -x "${PROBE_BIN}" ]] || { echo "FAIL: desktop plugin probe executable missing" >&2; exit 1; }

ISOLATED_ENV=(
  env -i
  "PATH=${BIN_DIR}"
  "HOME=${HOME_DIR}"
  "AGENTHUB_HOME=${AGENTHUB_HOME_DIR}"
  "CLAUDE_CONFIG_DIR=${CLAUDE_HOME}"
  "CODEX_HOME=${CODEX_HOME_DIR}"
  "GROK_HOME=${GROK_HOME_DIR}"
  "PI_CODING_AGENT_DIR=${PI_HOME_DIR}"
  "PI_CODING_AGENT_SESSION_DIR=${SCRATCH}/pi-sessions"
  "XDG_CONFIG_HOME=${SCRATCH}/xdg-config"
  "XDG_DATA_HOME=${SCRATCH}/xdg-data"
  "XDG_CACHE_HOME=${SCRATCH}/xdg-cache"
  "TMPDIR=${SCRATCH}/tmp"
  "GIT_CONFIG_NOSYSTEM=1"
  "GIT_TERMINAL_PROMPT=0"
  "CARGO_NET_OFFLINE=true"
  "PI_OFFLINE=1"
  "PI_TELEMETRY=0"
  "HTTP_PROXY=http://127.0.0.1:9"
  "HTTPS_PROXY=http://127.0.0.1:9"
  "ALL_PROXY=http://127.0.0.1:9"
  "NO_PROXY=127.0.0.1,localhost"
  "AGENTHUB_PLUGIN_FIXTURE_ROOT=${FIXTURE_ROOT}"
  "AGENTHUB_PLUGIN_FIXTURE_DIR=${FIXTURE_DIR}"
  "AGENTHUB_PLUGIN_FIXTURE_LOG=${INVOCATION_LOG}"
  "AGENTHUB_PLUGIN_PROBE_CLAUDE_SOURCE=agenthub-claude-probe@agenthub-probe-market"
  "AGENTHUB_PLUGIN_PROBE_CODEX_SOURCE=agenthub-probe-plugin@agenthub-probe-market"
  "AGENTHUB_PLUGIN_PROBE_GROK_SOURCE=agenthub-grok-probe"
  "AGENTHUB_PLUGIN_PROBE_PI_SOURCE=${FIXTURE_DIR}/pi-package"
)

(cd "${SCRATCH}/work" && \
  "${ISOLATED_ENV[@]}" "${PROBE_BIN}" "${SCRATCH}" "${BIN_DIR}") \
  >"${RUN_LOG}" 2>&1

[[ -s "${INVOCATION_LOG}" ]] || { echo "FAIL: vendor CLI invocation log is empty" >&2; exit 1; }

python3 - "${INVOCATION_LOG}" "${RUN_LOG}" "${EVIDENCE}" "${ROOT}" <<'PY'
import hashlib
import json
import re
import sys
from pathlib import Path

invocations_path, run_path, evidence_path, root_path = sys.argv[1:]
with open(invocations_path, encoding="utf-8") as handle:
    invocations = [json.loads(line) for line in handle if line.strip()]
with open(run_path, encoding="utf-8") as handle:
    output = handle.read()

agents = sorted({row["agent"] for row in invocations})
assert agents == ["claude", "codex", "grok", "pi"], agents
assert all(row["exitCode"] in (0, 73) for row in invocations), invocations
assert all(not any(re.search(r"(?i)(bearer|api[_-]?key|access[_-]?token|client[_-]?secret|sk-[a-z0-9])", arg)
                   for arg in row["args"])
           for row in invocations), invocations
operations = {}
for row in invocations:
    operations.setdefault(row["agent"], set()).add(row["operation"])
required = {
    "claude": {"list", "list-available", "install", "enable", "disable", "update", "marketplace-update", "uninstall"},
    "codex": {"list", "list-available", "install", "marketplace-upgrade", "uninstall"},
    "grok": {"list", "list-available", "install", "enable", "disable", "update", "marketplace-update", "uninstall"},
    "pi": {"install", "update", "uninstall"},
}
assert all(expected <= operations.get(agent, set()) for agent, expected in required.items()), operations
assert any(row["exitCode"] == 73 for row in invocations), "failure injection was not exercised"
probe_reported_ok = re.search(r'"status"\s*:\s*"ok"', output) is not None or "PASS" in output
assert probe_reported_ok, output

root = Path(root_path)
source_paths = [
    root / "src-tauri/Cargo.toml",
    root / "src-tauri/src/commands/plugins.rs",
    root / "src-tauri/src/lib.rs",
    root / "src-tauri/src/plugin_write_probe.rs",
    root / "src-tauri/examples/plugin_write_e2e_probe.rs",
    root / "src/pages/plugins/index.test.tsx",
    root / "scripts/plugin-probe/fake-vendor-cli.py",
    root / "scripts/plugin-probe/four-vendor-desktop-isolated.sh",
]
source_paths.extend(sorted((root / "scripts/plugin-probe/fixtures").rglob("*")))
fingerprint = hashlib.sha256()
for path in source_paths:
    if not path.is_file():
        continue
    relative = path.relative_to(root).as_posix().encode()
    fingerprint.update(len(relative).to_bytes(4, "big"))
    fingerprint.update(relative)
    contents = path.read_bytes()
    fingerprint.update(len(contents).to_bytes(8, "big"))
    fingerprint.update(contents)

summary = {
    "schema": "plugin-four-vendor-desktop-probe.v1",
    "status": "ok",
    "agents": agents,
    "offlineFixturesOnly": True,
    "isolatedUserDirectories": True,
    "commandPathAllowlisted": True,
    "networkIsolation": "not-provided",
    "proxySinkholeConfigured": True,
    "secretsInArgv": False,
    "sourceFingerprint": fingerprint.hexdigest(),
    "invocationCount": len(invocations),
    "probeReportedOk": probe_reported_ok,
}
with open(evidence_path, "w", encoding="utf-8") as handle:
    json.dump(summary, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY

if grep -E -q '/workspace/|/home/|sk-[A-Za-z0-9]|api[_-]?key|bearer' "${EVIDENCE}"; then
  echo "FAIL: evidence contains a path or secret-shaped value" >&2
  exit 1
fi

echo "PASS: isolated four-vendor desktop plugin lifecycle"
echo "evidence: ${EVIDENCE}"
