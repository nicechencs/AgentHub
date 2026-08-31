# AgentHub

[简体中文](README.md) · **English**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-0078D6.svg)](#quick-start)
[![Release](https://img.shields.io/github/v/release/nicechencs/AgentHub?label=version)](https://github.com/nicechencs/AgentHub/releases)

AgentHub is a local desktop tool for multiple coding agents. One GUI and CLI manage install runtimes, logins and connections, Skills, usage, and local sessions. It is built with Tauri v2, React, and Rust, and runs on Windows, macOS, and Linux.

The desktop UI is available in English and 简体中文. Switch under **Settings → Preferences → Language**.

## Features

- Manage local agents such as Claude Code, Codex, Kimi, Grok, Pi, WorkBuddy, ZCode, Cursor Agent, and DeepSeek Harness. Cursor Agent is hidden from the sidebar and Connections by default; you can unhide it on Agents.
- See logins and connections across tools. Prefer writing directly into the target tool’s config. **Local forwarding** is used only when there are published rules and a tested protocol conversion; otherwise the UI shows **Not supported now**, and nothing is forwarded silently.
- Manage shared Skills, inspect discovered MCP, and browse projects and sessions.
- Start a local session in the desktop app and watch the streaming process. Usage and cost estimates are parsed from local session logs.
- Use the CLI for `doctor`, `env`, `agent`, `provider`, `account`, `skill`, `usage`, `backup`, `run`, and `config`.

Current implementation limits and known gaps: [implementation status](docs/STATUS.md) (Chinese). Product decisions and the connection model: [docs index](docs/README.md) (Chinese).

## UI preview

These are the current desktop pages. Screenshots are from a real desktop window, with the system taskbar cropped. The same pages are available in English from **Settings → Preferences → Language**.

### Dashboard

See whether each agent is ready, plus usage and cost estimates parsed from local logs. When you need to sign in or switch, go to Connections from here.

![Dashboard](docs/assets/screenshots/dashboard.png)

### Connections

A cross-tool **login list**. You can import an authorization already on this computer, use **official login** in the browser, or add an API Key. When connecting to another tool, prefer **Direct** or **Use this login**. **Local route** is used only when the two sides don’t speak the same protocol and a tested conversion exists; otherwise the UI shows **Not supported now**.

![Connections / login list](docs/assets/screenshots/connections.png)

### Chat

Chat with an installed agent in the desktop app. The session list is on the left, the conversation in the middle; process steps can be expanded. Pick an agent and a working directory before you send.

![Chat](docs/assets/screenshots/chat.png)

### Agents

Detect and fix the local runtime, then install or update each agent by **channel**. Open a card to see **endpoint type**, channel, and **config folder**, and to open that folder. Cursor Agent is hidden from the sidebar and Connections by default; you can unhide it here.

![Agents](docs/assets/screenshots/agents.png)

### Skills

User skills live in the shared library and can be enabled on each tool. Project skills are managed per workspace. You can also install from the skill market.

![Skills](docs/assets/screenshots/skills.png)

### Projects

Browse local projects and sessions by agent. You can open a folder, preview an excerpt, or continue in Chat.

![Projects](docs/assets/screenshots/projects.png)

### Settings

Four tabs:

- **Preferences**: language, theme, brand color, launch at login, close to tray, whether the sidebar shows Routes / Plugins, skill market, and related options
- **This computer**: data directory and logs
- **Backups**: config snapshots kept before a switch or import; restore or delete
- **About**: version, check for updates, and the repository link

![Settings: Preferences / This computer / Backups / About](docs/assets/screenshots/settings.png)

### MCP

This page only scans and lists MCP that was already found. It does not install anything, and it does not change each tool’s settings. Write and manage are not implemented yet.

### Plugins

Read-only view of plugin packs already installed in Claude and Grok. There is no install button, and this page does not change those packs.

### Routes

Manage local forwarding: fill clients with a local address. Login details stay on Connections. Most connections can use Direct or Use this login. Mixed vendors and some protocol conversions are still in development; this is not a finished universal router.

## Roadmap

Local forwarding, plugins, and similar management will keep growing. These are currently **in development**.

- **Plugins**: read-only list of Claude / Grok plugin packs; no install button.
- **MCP**: read-only scan only; write and manage are not implemented yet.
- **Routes**: some connections can already be forwarded; mixed vendors and some protocol conversions are still in progress. Do not treat this as a finished universal router.

Finer implementation limits: [implementation status](docs/STATUS.md) (Chinese).

## Quick start

### Use a release build

Download the installer for your platform from [GitHub Releases](https://github.com/nicechencs/AgentHub/releases):

| Platform | Package | Release notes |
| --- | --- | --- |
| Windows | NSIS `.exe` or MSI | Installer and updater are signed |
| macOS | `.dmg` | Updater is signed; Apple notarization is not promised |
| Linux | `.deb` or AppImage | The package may be unsigned; auto-update is enabled only when the release manifest includes a signed Linux entry |

### Run from source

You need Node.js LTS, Rust, Git, and pnpm.

```powershell
pnpm install
pnpm tauri:dev
```

On Windows you can also run `.\run.ps1`; on macOS/Linux you can run `./run.sh`. These scripts check desktop dependencies and start the real Tauri backend. If Linux system libraries are missing, run `./scripts/check-linux-prereqs.sh --print-packages` for the matching distro install hints.

To browse demo data in the browser only:

```bash
pnpm install
pnpm dev:mock
```

`pnpm dev` is the ordinary Vite frontend dev server. It is not the Tauri launch command, and it does not automatically provide a mock backend. Use `pnpm tauri:dev` for the real desktop backend, and `pnpm dev:mock` for demo data.

## Common commands

| Command | Purpose |
| --- | --- |
| `pnpm dev` | Start the ordinary Vite frontend dev server |
| `pnpm dev:mock` | Start the browser mock demo |
| `pnpm tauri:dev` | Start the real Tauri desktop app |
| `pnpm typecheck` | Check frontend types |
| `pnpm typecheck:test` | Check test types |
| `pnpm test` | Run frontend tests |
| `pnpm test:e2e:browser` | Run Playwright browser smoke (`dev:mock` only) |
| `pnpm build` | Production frontend build; forces the Tauri adapter |
| `pnpm tauri:build` | Build desktop installers |
| `cargo test -p agenthub-core --locked` | Run Rust core tests |
| `cargo run -p agenthub-cli -- --help` | Show CLI help |
| `pnpm check:docs` | Check doc links, metadata, heading anchors, and stale terms |

Frontend backend layering, mock boundaries, and `invoke` constraints: [architecture](docs/architecture/overview.md) (Chinese). Full verification matrix: [testing](docs/reference/testing.md) (Chinese). Routing limits: [route compatibility](docs/reference/route-compatibility.md) (Chinese). Real desktop verification: [Adapter dogfood](docs/guides/adapter-dogfood.md) (Chinese).

## Data and privacy

AgentHub works with data on this computer by default. Common locations are `~/.agenthub/` (state, settings, logs, and backups), `~/.agents/skills/` (user skills), and `.agents/skills/` inside a project (project skills). Usage only reads local sessions or logs; it does not intercept requests through a proxy and does not upload to the cloud. Local forwarding does not record request or response bodies.

Credentials stay in the project’s existing local storage. The UI, CLI, and log output redact secrets. Encrypting credentials on disk is out of current product scope. Screenshots, test data, releases, and files that must not be committed: [privacy and release](docs/reference/privacy-and-release.md) (Chinese). Vulnerability reporting: [security policy](SECURITY.md) (Chinese).

## Development and docs

- [Contributing](CONTRIBUTING.md) (Chinese): branches, development environment, verification, PRs, and release flow.
- [Project conventions](AGENTS.md) (Chinese): architecture limits, product scope, and collaboration.
- [Docs index](docs/README.md) (Chinese): design, implementation, operations, and history by purpose.
- [Implementation status](docs/STATUS.md) (Chinese): current facts and unimplemented limits.
- [Doc style](docs/STYLE.md) (Chinese): metadata, categories, links, and maintenance rules.

Day-to-day development and PRs use the `dev` branch. For a formal release, bump the version on `dev`, update `CHANGELOG.md`, merge into `release`, then tag `vX.Y.Z` on `dev` to trigger CI. Details: [Contributing](CONTRIBUTING.md).

## License

This project is under the [MIT License](LICENSE). Usage parsing and config switching borrow from [ccusage](https://github.com/ccusage/ccusage) and [cc-switch](https://github.com/farion1231/cc-switch); local routing borrows from [CLIProxyAPI](https://github.com/router-for-me/CLIProxyAPI).
