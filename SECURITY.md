# Security Policy

AgentHub is a **local desktop** multi-agent hub (Tauri + Rust core). This document describes how to report vulnerabilities and what is in / out of scope.

中文说明见文末 [中文摘要](#中文摘要)。

## Supported versions

| Version | Supported |
|---|---|
| Latest release on [GitHub Releases](https://github.com/nicechencs/AgentHub/releases) | Yes |
| `main` / active development branches | Best-effort |
| Older releases | No fixed support window; upgrade when possible |

## Reporting a vulnerability

**Do not** open a public GitHub Issue, Discussion, or PR that includes exploit details, real credentials, or private paths.

### Preferred channel

1. **GitHub Private Vulnerability Reporting** (Security Advisories) on this repository, if enabled:  
   [https://github.com/nicechencs/AgentHub/security/advisories/new](https://github.com/nicechencs/AgentHub/security/advisories/new)
2. If Advisories are unavailable, open a **minimal** public Issue titled only  
   `Security: request private contact`  
   **without** technical details, and wait for maintainers to move the conversation to a private channel.

### What to include

- Affected version or commit, OS (e.g. Windows 10/11)
- Impact summary (e.g. local arbitrary file write, credential exposure in logs/UI, command injection)
- Steps to reproduce with **synthetic** data only
- Whether a fix idea exists (optional)

### Please do not include

- Real API keys, OAuth tokens, session cookies, or production account emails
- Full dumps of `~/.agenthub/`, third-party agent auth files, or personal screenshots with real paths
- 0-day chains against **third-party** Agent CLIs unless AgentHub’s code is required to trigger them (report those to the upstream vendor as well)

### Expected handling

| Step | Target |
|---|---|
| Initial acknowledgement | Within **7 days** (best-effort; maintainers may be part-time) |
| Severity triage | After reproduce |
| Fix / mitigation / public disclosure | Coordinated when a fix is ready; no fixed SLA |

We appreciate responsible disclosure and will credit reporters who wish to be named (unless they prefer anonymity).

## Scope

### In scope

Issues in **AgentHub’s own code** that can cause:

- Credential or token leakage via GUI, CLI, logs, backups export, or IPC
- Path traversal / arbitrary file read-write outside intended live/backup allowlists
- Command or argument injection when invoking install scripts, agents, or shell helpers
- Tampering or spoofing of the **app update** chain controlled by this project
- Privilege escalation **from the app’s security model** (e.g. bypassing confirmations for destructive writes) when caused by AgentHub bugs
- XSS or unsafe `invoke` bridging that reaches native capabilities without intended checks

### Out of scope

| Topic | Reason |
|---|---|
| Missing **at-rest credential encryption** (keyring / AES / master password) | Explicitly **out of product scope**; see [AGENTS.md](AGENTS.md) and plan docs |
| Bugs **only** in third-party Agents (Claude / Codex / …) with no AgentHub involvement | Report upstream |
| Social engineering, physical access, unlocked user session | User environment |
| Dependency CVEs with no realistic local impact path | Track via normal dependency updates unless exploitable through AgentHub |
| DoS against the local machine by exhausting disk/CPU via intentional heavy use | Not a security boundary for a single-user desktop tool |
| Features marked Planned / Unsupported in the [capability matrix](docs/capability-matrix.md) | Not vulnerabilities |
| Documentation typos, UI polish, feature requests | Use normal Issues |

## Security-related product stance (not vulnerabilities)

These are intentional design choices, not reportable “missing features”:

- Credentials use the **current on-disk storage model** with **output redaction** in API / CLI / logs; encryption-at-rest is not planned.
- Usage statistics are **read-only** parsers of local agent logs (no local MITM proxy).
- Skills / config writes follow backup + path allowlist patterns; details live in source and internal design docs—**do not** expand third-party credential path inventories in public issues ([docs/privacy.md](docs/privacy.md)).

## Safe harbor

If you make a good-faith effort to follow this policy, avoid privacy harm, and do not exploit issues beyond what is needed to demonstrate impact, we will not pursue legal action related to the research.

## Non-security contact

General bugs, docs, and features: [GitHub Issues](https://github.com/nicechencs/AgentHub/issues).

---

## 中文摘要

| 项 | 说明 |
|---|---|
| **怎么报** | 优先用 GitHub **私有漏洞报告**（Security Advisories）；不可用时只开标题为「请求私信」的空 Issue，**不要**公开复现步骤 |
| **支持版本** | 最新 Release + 活跃分支尽力；旧版本请升级 |
| **范围内** | 本仓库代码导致的凭据泄露、任意文件读写、命令注入、更新链问题、不安全的前端→原生桥等 |
| **范围外** | 凭据落盘加密未实现（产品明确不做）、纯第三方 Agent 漏洞、物理/社工、无实际路径的依赖 CVE、功能规划项 |
| **响应** | 尽力 **7 日内**确认收到；修复与公开披露协调进行，无硬性 SLA |
| **关联** | 仓库禁止提交项与截图规范见 [docs/privacy.md](docs/privacy.md) |
