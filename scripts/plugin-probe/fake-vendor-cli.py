#!/usr/bin/env python3
"""Offline Claude/Codex/Grok/Pi CLI fixture used by the desktop plugin probe.

The executable name selects the vendor.  The enclosing shell harness creates
`claude`, `codex`, `grok`, and `pi` symlinks to this file and supplies disposable
homes.  This intentionally models only the plugin commands AgentHub calls.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import sys
from pathlib import Path
from typing import Any


AGENT = Path(sys.argv[0]).name.lower()
ARGS = sys.argv[1:]
SUPPORTED_AGENTS = {"claude", "codex", "grok", "pi"}


def die(message: str, code: int = 2) -> "NoReturn":
    print(message, file=sys.stderr)
    raise SystemExit(code)


if AGENT not in SUPPORTED_AGENTS:
    die(f"unsupported fixture executable: {AGENT}")

fixture_root_raw = os.environ.get("AGENTHUB_PLUGIN_FIXTURE_ROOT", "")
fixture_dir_raw = os.environ.get("AGENTHUB_PLUGIN_FIXTURE_DIR", "")
log_path_raw = os.environ.get("AGENTHUB_PLUGIN_FIXTURE_LOG", "")
if not fixture_root_raw or not fixture_dir_raw or not log_path_raw:
    die("fixture root, fixture directory, and invocation log are required")

FIXTURE_ROOT = Path(fixture_root_raw).resolve()
FIXTURE_DIR = Path(fixture_dir_raw).resolve()
LOG_PATH = Path(log_path_raw).resolve()
FIXTURE_ROOT.mkdir(parents=True, exist_ok=True)
LOG_PATH.parent.mkdir(parents=True, exist_ok=True)

SECRET_SHAPES = (
    re.compile(r"(?i)^bearer\s+"),
    re.compile(r"(?i)(api[_-]?key|access[_-]?token|client[_-]?secret)="),
    re.compile(r"^(sk|xai)-[A-Za-z0-9_-]{12,}$"),
)
for argument in ARGS:
    if any(pattern.search(argument) for pattern in SECRET_SHAPES):
        die("secret-shaped values must not be passed in plugin command arguments")


def atomic_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    temporary.write_text(text, encoding="utf-8")
    temporary.replace(path)


def atomic_json(path: Path, value: Any) -> None:
    atomic_text(path, json.dumps(value, indent=2, sort_keys=True) + "\n")


def read_json(path: Path, default: Any) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return default


def append_log(operation: str, exit_code: int) -> None:
    record = {
        "agent": AGENT,
        "args": ARGS,
        "cwd": os.getcwd(),
        "operation": operation,
        "exitCode": exit_code,
    }
    with LOG_PATH.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")


def operation_name() -> str:
    if AGENT == "pi":
        if ARGS[:2] == ["update", "--extensions"]:
            return "update"
        if ARGS and ARGS[0] == "remove":
            return "uninstall"
        return ARGS[0] if ARGS else "missing"
    if ARGS[:3] == ["plugin", "marketplace", "update"]:
        return "marketplace-update"
    if ARGS[:3] == ["plugin", "marketplace", "upgrade"]:
        return "marketplace-upgrade"
    if len(ARGS) >= 2 and ARGS[0] == "plugin":
        if ARGS[1] == "list" and "--available" in ARGS:
            return "list-available"
        return {"add": "install", "remove": "uninstall"}.get(ARGS[1], ARGS[1])
    return "missing"


OPERATION = operation_name()


def home_for(agent: str) -> Path:
    home = Path(os.environ["HOME"])
    return {
        "claude": Path(os.environ.get("CLAUDE_CONFIG_DIR", home / ".claude")),
        "codex": Path(os.environ.get("CODEX_HOME", home / ".codex")),
        "grok": Path(os.environ.get("GROK_HOME", home / ".grok")),
        "pi": Path(os.environ.get("PI_CODING_AGENT_DIR", home / ".pi" / "agent")),
    }[agent]


HOME = home_for(AGENT)
HOME.mkdir(parents=True, exist_ok=True)
STATE_PATH = FIXTURE_ROOT / f"{AGENT}-state.json"


def corrupt_live_file() -> None:
    live = {
        "claude": HOME / "settings.json",
        "codex": HOME / "config.toml",
        "grok": HOME / "config.toml",
        "pi": HOME / "settings.json",
    }[AGENT]
    if live.suffix == ".json":
        atomic_json(live, {"fixtureFailure": f"{AGENT}:{OPERATION}"})
    else:
        atomic_text(live, f'fixture_failure = "{AGENT}:{OPERATION}"\n')


def should_fail_once() -> bool:
    requested = os.environ.get("AGENTHUB_PLUGIN_FIXTURE_FAIL_ONCE", "").strip()
    aliases = {f"{AGENT}:{OPERATION}"}
    if OPERATION in {"marketplace-update", "marketplace-upgrade"}:
        aliases.add(f"{AGENT}:refresh")
    if requested not in aliases:
        return False
    marker = FIXTURE_ROOT / "fail-once" / requested.replace(":", "-")
    if marker.exists():
        return False
    marker.parent.mkdir(parents=True, exist_ok=True)
    marker.write_text("consumed\n", encoding="utf-8")
    return True


if should_fail_once():
    corrupt_live_file()
    append_log(OPERATION, 73)
    print(f"injected fixture failure for {AGENT}:{OPERATION}", file=sys.stderr)
    raise SystemExit(73)


def require_exact(expected: list[str]) -> None:
    if ARGS != expected:
        die(f"unexpected {AGENT} fixture arguments: {ARGS!r}")


def copy_fixture(source: Path, destination: Path) -> None:
    if not source.is_dir():
        die(f"offline fixture directory is missing: {source.name}")
    if destination.exists():
        shutil.rmtree(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, destination)


def claude() -> None:
    source = "agenthub-claude-probe@agenthub-probe-market"
    name, market = source.split("@", 1)
    state = read_json(STATE_PATH, {"installed": False, "enabled": False, "version": "0.0.1"})
    installed_path = HOME / "plugins" / "cache" / name / state["version"]
    if OPERATION == "list-available":
        require_exact(["plugin", "list", "--json", "--available"])
        print(json.dumps({"available": [{
            "name": name,
            "marketplace": market,
            "version": "0.0.2",
            "scope": "user",
            "description": "Offline Claude plugin fixture",
            "status": "installed" if state["installed"] else "available",
            "installed": bool(state["installed"]),
            "components": [{"type": "skills", "name": "agenthub-claude-probe"}],
        }]}))
        return
    if OPERATION == "list":
        require_exact(["plugin", "list", "--json"])
        rows = []
        if state["installed"]:
            rows.append({
                "name": name,
                "marketplace": market,
                "version": state["version"],
                "scope": "user",
                "enabled": bool(state["enabled"]),
                "installPath": str(installed_path),
                "description": "Offline Claude plugin fixture",
                "status": "enabled" if state["enabled"] else "disabled",
                "components": [{"type": "skills", "name": "agenthub-claude-probe"}],
            })
        print(json.dumps({"installed": rows}))
        return
    if OPERATION == "install":
        require_exact(["plugin", "install", source, "-y", "-s", "user"])
        state.update(installed=True, enabled=True, version="0.0.1")
        copy_fixture(FIXTURE_DIR / "claude-plugin", installed_path)
    elif OPERATION in {"enable", "disable"}:
        require_exact(["plugin", OPERATION, source])
        if not state["installed"]:
            die("Claude fixture plugin is not installed", 4)
        state["enabled"] = OPERATION == "enable"
    elif OPERATION == "update":
        require_exact(["plugin", "update", source, "-y", "-s", "user"])
        if not state["installed"]:
            die("Claude fixture plugin is not installed", 4)
        old_path = installed_path
        state["version"] = "0.0.2"
        installed_path = HOME / "plugins" / "cache" / name / state["version"]
        copy_fixture(FIXTURE_DIR / "claude-plugin", installed_path)
        if old_path != installed_path and old_path.exists():
            shutil.rmtree(old_path)
    elif OPERATION == "marketplace-update":
        require_exact(["plugin", "marketplace", "update"])
        state["marketplaceRefreshes"] = int(state.get("marketplaceRefreshes", 0)) + 1
    elif OPERATION == "uninstall":
        require_exact(["plugin", "uninstall", source, "-s", "user", "-y", "--keep-data"])
        state.update(installed=False, enabled=False)
        cache = HOME / "plugins" / "cache" / name
        if cache.exists():
            shutil.rmtree(cache)
    else:
        die(f"unsupported Claude fixture operation: {OPERATION}")
    atomic_json(STATE_PATH, state)
    write_claude_live(state, source, name, market)
    print(json.dumps({"ok": True, "operation": OPERATION}))


def write_claude_live(state: dict[str, Any], source: str, name: str, market: str) -> None:
    settings = {"enabledPlugins": {source: bool(state["enabled"])} if state["installed"] else {}}
    atomic_json(HOME / "settings.json", settings)
    plugins: dict[str, Any] = {}
    if state["installed"]:
        plugins[source] = {
            "name": name,
            "marketplace": market,
            "version": state["version"],
            "scope": "user",
            "enabled": bool(state["enabled"]),
            "installPath": str(HOME / "plugins" / "cache" / name / state["version"]),
        }
    atomic_json(HOME / "plugins" / "installed_plugins.json", {"plugins": plugins})


def codex() -> None:
    source = "agenthub-probe-plugin@agenthub-probe-market"
    name, market = source.split("@", 1)
    state = read_json(STATE_PATH, {"installed": False, "version": "0.0.1"})
    installed_path = HOME / "plugins" / "cache" / market / name
    if OPERATION == "list-available":
        require_exact(["plugin", "list", "--available", "--json"])
        print(json.dumps({"available": [{
            "pluginId": source,
            "name": name,
            "marketplaceName": market,
            "version": state.get("version", "0.0.1"),
            "description": "Offline Codex plugin fixture",
            "status": "installed" if state["installed"] else "available",
            "installed": bool(state["installed"]),
            "path": str(installed_path),
        }]}))
        return
    if OPERATION == "list":
        require_exact(["plugin", "list", "--json"])
        rows = []
        if state["installed"]:
            enabled = codex_enabled(source)
            rows.append({
                "pluginId": source,
                "name": name,
                "marketplaceName": market,
                "version": state["version"],
                "status": "installed",
                "installed": True,
                "enabled": enabled,
                "path": str(installed_path),
            })
        print(json.dumps({"installed": rows}))
        return
    if OPERATION == "install":
        require_exact(["plugin", "add", source, "--json"])
        state.update(installed=True, version="0.0.1")
        copy_fixture(FIXTURE_DIR / "codex-marketplace" / "plugin", installed_path)
        write_codex_config(source, True)
    elif OPERATION == "uninstall":
        require_exact(["plugin", "remove", source, "--json"])
        state["installed"] = False
        if installed_path.exists():
            shutil.rmtree(installed_path)
        write_codex_config(source, None)
    elif OPERATION == "marketplace-upgrade":
        require_exact(["plugin", "marketplace", "upgrade"])
        state["marketplaceRefreshes"] = int(state.get("marketplaceRefreshes", 0)) + 1
        if state["installed"]:
            state["version"] = "0.0.2"
    else:
        die(f"unsupported Codex fixture operation: {OPERATION}")
    atomic_json(STATE_PATH, state)
    print(json.dumps({"ok": True, "operation": OPERATION}))


def write_codex_config(source: str, enabled: bool | None) -> None:
    if enabled is None:
        atomic_text(HOME / "config.toml", "")
        return
    atomic_text(HOME / "config.toml", f'[plugins."{source}"]\nenabled = {str(enabled).lower()}\n')


def codex_enabled(source: str) -> bool:
    try:
        text = (HOME / "config.toml").read_text(encoding="utf-8")
    except FileNotFoundError:
        return True
    section = re.search(
        rf'^\[plugins\."{re.escape(source)}"\]\s*(.*?)(?=^\[|\Z)',
        text,
        flags=re.MULTILINE | re.DOTALL,
    )
    if section is None:
        return True
    enabled = re.search(r"^enabled\s*=\s*(true|false)\s*$", section.group(1), re.MULTILINE)
    return enabled is None or enabled.group(1) == "true"


def grok() -> None:
    name = "agenthub-grok-probe"
    state = read_json(STATE_PATH, {
        "installed": False,
        "enabled": False,
        "version": "0.0.1",
        "installSource": name,
    })
    installed_path = HOME / "plugins" / name
    if OPERATION == "list-available":
        require_exact(["plugin", "list", "--json", "--available"])
        print(json.dumps({"available": [{
            "name": name,
            "marketplace": "agenthub-probe-market",
            "version": "0.0.2",
            "description": "Offline Grok plugin fixture",
            "status": "installed" if state["installed"] else "available",
            "installed": bool(state["installed"]),
            "components": [{"type": "skills", "name": "agenthub-grok-probe"}],
        }]}))
        return
    if OPERATION == "list":
        require_exact(["plugin", "list", "--json"])
        rows = []
        if state["installed"]:
            rows.append({
                "name": name,
                "version": state["version"],
                "scope": "user",
                "status": "installed",
                "trusted": True,
                "enabled": bool(state["enabled"]),
                "installSource": str(FIXTURE_DIR / "grok-plugin"),
                "path": str(installed_path),
                "components": [{"type": "skills", "name": "agenthub-grok-probe"}],
            })
            # Grok mutates by package name only.  The desktop probe enables
            # this fixture row around one mutation to prove a successful CLI
            # exit cannot be presented as success when the follow-up list has
            # two same-name, different-source candidates.
            if os.environ.get("AGENTHUB_PLUGIN_FIXTURE_GROK_AMBIGUOUS") == "1":
                rows.append({
                    "name": name,
                    "version": state["version"],
                    "scope": "user",
                    "status": "installed",
                    "trusted": True,
                    "enabled": bool(state["enabled"]),
                    "installSource": str(FIXTURE_DIR / "grok-plugin" / "skills" / "agenthub-probe"),
                    "path": str(HOME / "plugins" / f"{name}-local"),
                    "components": [{"type": "skills", "name": "agenthub-grok-probe"}],
                })
        print(json.dumps({"plugins": rows}))
        return
    if OPERATION == "install":
        if ARGS != ["plugin", "install", name, "--trust"]:
            die(f"unexpected Grok fixture arguments: {ARGS!r}")
        state.update(installed=True, enabled=True, version="0.0.1", installSource=name)
        copy_fixture(FIXTURE_DIR / "grok-plugin", installed_path)
    elif OPERATION in {"enable", "disable"}:
        require_exact(["plugin", OPERATION, name])
        if not state["installed"]:
            die("Grok fixture plugin is not installed", 4)
        state["enabled"] = OPERATION == "enable"
    elif OPERATION == "update":
        require_exact(["plugin", "update", name])
        if not state["installed"]:
            die("Grok fixture plugin is not installed", 4)
        state["version"] = "0.0.2"
    elif OPERATION == "marketplace-update":
        require_exact(["plugin", "marketplace", "update"])
        state["marketplaceRefreshes"] = int(state.get("marketplaceRefreshes", 0)) + 1
    elif OPERATION == "uninstall":
        require_exact(["plugin", "uninstall", name, "--confirm", "--keep-data"])
        state.update(installed=False, enabled=False)
        if installed_path.exists():
            shutil.rmtree(installed_path)
    else:
        die(f"unsupported Grok fixture operation: {OPERATION}")
    atomic_json(STATE_PATH, state)
    write_grok_config(state, name)
    print(json.dumps({"ok": True, "operation": OPERATION}))


def write_grok_config(state: dict[str, Any], name: str) -> None:
    enabled = f'["{name}"]' if state["installed"] and state["enabled"] else "[]"
    disabled = f'["{name}"]' if state["installed"] and not state["enabled"] else "[]"
    atomic_text(HOME / "config.toml", f"[plugins]\nenabled = {enabled}\ndisabled = {disabled}\n")


def pi() -> None:
    settings_path = HOME / "settings.json"
    settings = read_json(settings_path, {"packages": []})
    packages = settings.setdefault("packages", [])
    if OPERATION == "list":
        require_exact(["list", "--no-approve"])
        if packages:
            for source in packages:
                print(source if isinstance(source, str) else source.get("source", ""))
        else:
            print("No packages installed.")
        return
    if OPERATION == "install":
        if len(ARGS) != 3 or ARGS[0] != "install" or ARGS[2] != "--no-approve":
            die(f"unexpected Pi fixture arguments: {ARGS!r}")
        source = ARGS[1]
        if source not in packages:
            packages.append(source)
    elif OPERATION == "uninstall":
        if len(ARGS) != 3 or ARGS[0] != "remove" or ARGS[2] != "--no-approve":
            die(f"unexpected Pi fixture arguments: {ARGS!r}")
        source = ARGS[1]
        packages[:] = [item for item in packages if item != source and not (
            isinstance(item, dict) and item.get("source") == source
        )]
    elif OPERATION == "update":
        require_exact(["update", "--extensions", "--no-approve"])
        eligible = []
        for item in packages:
            source = item if isinstance(item, str) else item.get("source", "")
            if not re.fullmatch(r"npm:(?:@[^/@]+/)?[^/@]+@\d+\.\d+\.\d+", source):
                eligible.append(source)
        atomic_json(FIXTURE_ROOT / "pi-update-result.json", {"eligible": eligible})
    else:
        die(f"unsupported Pi fixture operation: {OPERATION}")
    atomic_json(settings_path, settings)
    atomic_json(STATE_PATH, {"packages": packages})
    print(json.dumps({"ok": True, "operation": OPERATION}))


try:
    {"claude": claude, "codex": codex, "grok": grok, "pi": pi}[AGENT]()
except SystemExit:
    raise
except Exception as error:  # keep fixture failures legible in captured stderr
    append_log(OPERATION, 70)
    die(f"{AGENT} fixture internal error: {error}", 70)
else:
    append_log(OPERATION, 0)
