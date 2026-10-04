#!/usr/bin/env bash
# Linux real-process probe for packaged-sidecar staging and stale-root GC.
# It replaces the bundled source at the exact close-after-copy boundary and
# proves the GUI executes the independently verified private scratch copy.

set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "SKIP: packaged sidecar replacement probe currently uses Linux LD_PRELOAD"
  exit 0
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN_ROOT="$(mktemp -d /tmp/agenthub-sidecar-security-probe.XXXXXX)"
PROBE_USER_HOME="${RUN_ROOT}/user-home"
BUILT_SOURCE="${ROOT}/target/debug/agenthub-adapterd"
BUILT_GUI="${ROOT}/target/debug/agenthub-gui"
PACKAGED_DIR="${RUN_ROOT}/packaged"
SOURCE="${PACKAGED_DIR}/agenthub-adapterd"
GUI="${PACKAGED_DIR}/agenthub-gui"
REPLACEMENT="${RUN_ROOT}/replacement"
BACKUP="${RUN_ROOT}/bundled-backup"
HOOK_SOURCE="${RUN_ROOT}/replace-on-close.c"
HOOK_LIBRARY="${RUN_ROOT}/replace-on-close.so"
HOOK_FIRED="${RUN_ROOT}/hook-fired"
START_MARKER="${RUN_ROOT}/started"
GUI_LOG="${RUN_ROOT}/gui.log"
LAUNCHER_PID=""
GUI_PID=""
ADAPTERD_PID=""
SESSION_ROOT=""
GC_DEAD_PID=2147483647
GC_NANOS="$(date +%s)000000000"
GC_ROOT="/tmp/agenthub-go-route-isolated-${GC_DEAD_PID}-${GC_NANOS}-0"
GC_NEIGHBOR="/tmp/agenthub-go-route-isolated-${GC_DEAD_PID}-${GC_NANOS}-1"

cleanup() {
  local code=$?
  if [[ -n "${LAUNCHER_PID}" ]]; then
    kill -- "-${LAUNCHER_PID}" 2>/dev/null || true
    for _ in $(seq 1 40); do
      kill -0 -- "-${LAUNCHER_PID}" 2>/dev/null || break
      sleep 0.05
    done
    kill -KILL -- "-${LAUNCHER_PID}" 2>/dev/null || true
    wait "${LAUNCHER_PID}" 2>/dev/null || true
  fi
  if [[ -e "${BACKUP}" ]]; then
    if [[ -e "${SOURCE}" ]]; then
      mv "${SOURCE}" "${REPLACEMENT}.used"
    fi
    mv "${BACKUP}" "${SOURCE}"
  fi
  if [[ -n "${SESSION_ROOT}" && "${SESSION_ROOT}" == /tmp/agenthub-go-route-isolated-* ]]; then
    rm -rf -- "${SESSION_ROOT}"
  fi
  rm -rf -- "${GC_ROOT}" "${GC_NEIGHBOR}"
  if [[ ${code} -eq 0 && "${RUN_ROOT}" == /tmp/agenthub-sidecar-security-probe.* ]]; then
    if mountpoint -q "${RUN_ROOT}/xdg-cache/doc"; then
      fusermount3 -u "${RUN_ROOT}/xdg-cache/doc" 2>/dev/null || true
    fi
    chmod -R u+rwX "${RUN_ROOT}" 2>/dev/null || true
    rm -rf -- "${RUN_ROOT}"
  fi
  if [[ ${code} -ne 0 ]]; then
    echo "FAIL: packaged sidecar security probe exited ${code}; evidence at ${RUN_ROOT}" >&2
  fi
}
trap cleanup EXIT

mkdir -p "${PROBE_USER_HOME}"

if [[ "${AGENTHUB_SIDECAR_PROBE_SKIP_BUILD:-0}" != "1" ]]; then
  (cd "${ROOT}" && pnpm tauri:build --debug --no-bundle -- --locked)
fi
[[ -x "${BUILT_GUI}" && -x "${BUILT_SOURCE}" ]] || {
  echo "FAIL: debug GUI and packaged sidecar must be staged first" >&2
  exit 1
}
mkdir -p "${PACKAGED_DIR}"
cp "${BUILT_GUI}" "${GUI}"
cp "${BUILT_SOURCE}" "${SOURCE}"
chmod 755 "${GUI}" "${SOURCE}"
if [[ "$(id -u)" -eq 0 ]]; then
  echo "FAIL: run this probe as a non-root user so root-owned source acceptance is real" >&2
  exit 1
fi
if ! sudo -n chown 0:0 "${SOURCE}"; then
  echo "FAIL: passwordless sudo is required to create the controlled root-owned source" >&2
  exit 1
fi
[[ "$(stat -c '%u' "${SOURCE}")" == "0" && "$(stat -c '%a' "${SOURCE}")" == "755" ]] || {
  echo "FAIL: controlled packaged source is not root-owned mode 755" >&2
  exit 1
}
EXPECTED_HASH="$(sha256sum "${SOURCE}" | awk '{print $1}')"
cp /bin/false "${REPLACEMENT}"
chmod 500 "${REPLACEMENT}"

# Initialize the isolated database once, then persist one synthetic saved route
# so the debug-only auto-start exercises the real supervisor.
set +e
timeout --kill-after=2s 5s env \
  HOME="${PROBE_USER_HOME}" \
  XDG_CONFIG_HOME="${RUN_ROOT}/xdg-config" \
  XDG_DATA_HOME="${RUN_ROOT}/xdg-data" \
  XDG_CACHE_HOME="${RUN_ROOT}/xdg-cache" \
  AGENTHUB_HOME="${RUN_ROOT}/agenthub-home" \
  xvfb-run -a "${GUI}" >"${RUN_ROOT}/initialize.log" 2>&1
init_code=$?
set -e
if [[ ${init_code} -ne 0 && ${init_code} -ne 124 && ${init_code} -ne 137 ]]; then
  cat "${RUN_ROOT}/initialize.log" >&2
  echo "FAIL: isolated database initialization failed (${init_code})" >&2
  exit 1
fi
python3 - "${RUN_ROOT}/agenthub-home/agenthub.db" <<'PY'
import json, sqlite3, sys
db = sqlite3.connect(sys.argv[1])
now = "probe"
db.execute(
    "INSERT INTO providers(id, agent_id, name, settings_config, meta, is_current, created_at, updated_at) VALUES(?,?,?,?,?,?,?,?)",
    ("probe-anthropic", "claude", "Probe Anthropic", json.dumps({"apiKey":"sk-probe-synthetic"}), json.dumps({"preset":"anthropic"}), 0, now, now),
)
db.execute(
    "INSERT INTO route_pools(id, target_agent_id, downstream_surface, downstream_dialect, hub_token, schedule_policy, is_default, unified_gateway_enrolled, policy_revision, auto_start, created_at, updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
    ("probe-pool", "claude", "messages", "claude", "ahb_probe_synthetic", "priority_failover", 1, 0, 1, 0, now, now),
)
db.execute(
    "INSERT INTO route_members(id, route_pool_id, source_kind, source_id, enabled, priority, position, created_at, updated_at) VALUES(?,?,?,?,?,?,?,?,?)",
    ("probe-member", "probe-pool", "provider", "probe-anthropic", 1, 0, 0, now, now),
)
db.commit()
PY

mkdir -p "${GC_ROOT}" "${GC_NEIGHBOR}"
chmod 700 "${GC_ROOT}" "${GC_NEIGHBOR}"
printf 'agenthub-go-route-owner-v1\npid=%s\ncreated_unix_nanos=%s\nattempt=0\n' \
  "${GC_DEAD_PID}" "${GC_NANOS}" >"${GC_ROOT}/.agenthub-go-route-owner-v1"
printf 'not-an-agenthub-owner-marker\n' >"${GC_NEIGHBOR}/.agenthub-go-route-owner-v1"
chmod 600 "${GC_ROOT}/.agenthub-go-route-owner-v1" "${GC_NEIGHBOR}/.agenthub-go-route-owner-v1"
touch -d '2 days ago' "${GC_ROOT}/.agenthub-go-route-owner-v1" "${GC_NEIGHBOR}/.agenthub-go-route-owner-v1"

cat >"${HOOK_SOURCE}" <<'C'
#define _GNU_SOURCE
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>

static int fired;

int close(int fd) {
  const char *source = getenv("AH_SIDECAR_PROBE_SOURCE");
  const char *replacement = getenv("AH_SIDECAR_PROBE_REPLACEMENT");
  const char *backup = getenv("AH_SIDECAR_PROBE_BACKUP");
  const char *signal_path = getenv("AH_SIDECAR_PROBE_FIRED");
  char link_path[64];
  char target[PATH_MAX + 1];
  int length = snprintf(link_path, sizeof(link_path), "/proc/self/fd/%d", fd);
  ssize_t count = length > 0 ? readlink(link_path, target, PATH_MAX) : -1;
  if (count >= 0) target[count] = '\0';
  if (source && replacement && backup && signal_path && count >= 0 &&
      strcmp(target, source) == 0 && __sync_lock_test_and_set(&fired, 1) == 0) {
    if (syscall(SYS_renameat, AT_FDCWD, source, AT_FDCWD, backup) == 0) {
      if (syscall(SYS_renameat, AT_FDCWD, replacement, AT_FDCWD, source) != 0) {
        syscall(SYS_renameat, AT_FDCWD, backup, AT_FDCWD, source);
      } else {
        int signal_fd = syscall(SYS_openat, AT_FDCWD, signal_path,
                                O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0600);
        if (signal_fd >= 0) syscall(SYS_close, signal_fd);
      }
    }
  }
  return syscall(SYS_close, fd);
}
C
cc -shared -fPIC -O2 -Wall -Wextra -o "${HOOK_LIBRARY}" "${HOOK_SOURCE}"

touch "${START_MARKER}"
HOME="${PROBE_USER_HOME}" \
XDG_CONFIG_HOME="${RUN_ROOT}/xdg-config" \
XDG_DATA_HOME="${RUN_ROOT}/xdg-data" \
XDG_CACHE_HOME="${RUN_ROOT}/xdg-cache" \
AGENTHUB_HOME="${RUN_ROOT}/agenthub-home" \
AGENTHUB_GO_ROUTE_ISOLATED_AUTO_START=1 \
AH_SIDECAR_PROBE_SOURCE="${SOURCE}" \
AH_SIDECAR_PROBE_REPLACEMENT="${REPLACEMENT}" \
AH_SIDECAR_PROBE_BACKUP="${BACKUP}" \
AH_SIDECAR_PROBE_FIRED="${HOOK_FIRED}" \
LD_PRELOAD="${HOOK_LIBRARY}" \
setsid xvfb-run -a "${GUI}" >"${GUI_LOG}" 2>&1 &
LAUNCHER_PID=$!

for _ in $(seq 1 300); do
  [[ -e "${HOOK_FIRED}" ]] && break
  kill -0 "${LAUNCHER_PID}" 2>/dev/null || { cat "${GUI_LOG}" >&2; exit 1; }
  sleep 0.05
done
[[ -e "${HOOK_FIRED}" ]] || { echo "FAIL: source replacement hook did not fire" >&2; exit 1; }
[[ ! -e "${GC_ROOT}" ]] || { echo "FAIL: conforming stale scratch root was not collected" >&2; exit 1; }
[[ -d "${GC_NEIGHBOR}" ]] || { echo "FAIL: nonconforming scratch neighbor was removed" >&2; exit 1; }
[[ "$(sha256sum "${SOURCE}" | awk '{print $1}')" != "${EXPECTED_HASH}" ]] || {
  echo "FAIL: bundled source was not replaced" >&2
  exit 1
}

for _ in $(seq 1 300); do
  SESSION_ROOT="$(find /tmp -maxdepth 1 -type d -name 'agenthub-go-route-isolated-*' -newer "${START_MARKER}" -print -quit)"
  [[ -n "${SESSION_ROOT}" && -x "${SESSION_ROOT}/bin/agenthub-adapterd" ]] && break
  kill -0 "${LAUNCHER_PID}" 2>/dev/null || { cat "${GUI_LOG}" >&2; exit 1; }
  sleep 0.05
done
[[ -n "${SESSION_ROOT}" && -x "${SESSION_ROOT}/bin/agenthub-adapterd" ]] || {
  cat "${GUI_LOG}" >&2
  echo "FAIL: verified scratch sidecar was not staged" >&2
  exit 1
}
[[ "$(stat -c '%a' "${SESSION_ROOT}")" == "700" ]]
[[ "$(stat -c '%a' "${SESSION_ROOT}/bin")" == "700" ]]
[[ "$(stat -c '%a' "${SESSION_ROOT}/bin/agenthub-adapterd")" == "500" ]]
[[ "$(sha256sum "${SESSION_ROOT}/bin/agenthub-adapterd" | awk '{print $1}')" == "${EXPECTED_HASH}" ]]

GUI_PID="$(sed -n 's/^pid=//p' "${SESSION_ROOT}/.agenthub-go-route-owner-v1")"
kill -0 "${GUI_PID}"
for _ in $(seq 1 200); do
  for process_exe in /proc/[0-9]*/exe; do
    resolved="$(readlink "${process_exe}" 2>/dev/null || true)"
    if [[ "${resolved}" == "${SESSION_ROOT}/bin/agenthub-adapterd" ]]; then
      ADAPTERD_PID="${process_exe#/proc/}"
      ADAPTERD_PID="${ADAPTERD_PID%/exe}"
      break 2
    fi
  done
  sleep 0.05
done
[[ -n "${ADAPTERD_PID}" ]] || { cat "${GUI_LOG}" >&2; echo "FAIL: staged sidecar did not execute" >&2; exit 1; }
[[ -S "${SESSION_ROOT}/home/run/adapterd.sock" ]] || { echo "FAIL: staged sidecar did not become ready" >&2; exit 1; }

echo "PASS: bundled source replacement could not change the verified scratch executable"
echo "PASS: non-root GUI accepted a root-owned, group/world-nonwritable bundled source"
echo "PASS: stale GC removed only the strict old owned root"
