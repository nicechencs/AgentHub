---
title: MCP inventory
description: AgentHub MCP 扫描与写入的路径、格式、片段和已知缺口。
type: reference
audience: contributor
status: current
updated: 2026-09-29
---

# MCP inventory

本页是 `/mcp` 页扫描与写入的现行契约。对象是 **MCP server 条目**，不是插件包（见 [Agent 插件表面](agent-plugin-surfaces.md)），也不等于 `Capability::Mcp` 的完整管理。

| 用途 | Tauri command | 实现（`crates/agenthub-core/src/services/`） |
|---|---|---|
| 只读扫描 | `list_mcp_inventory` | `mcp_inventory.rs` |
| 写入 / 启用 | `list_mcp_catalog` → `probe_mcp_server` → `upsert_mcp_server` / `set_mcp_server_enabled` | `mcp_manage.rs` |

## 返回结构

| 字段 | 含义 |
|---|---|
| `sources[]` | 每个已知配置文件一条：路径、是否存在、是否可读、解析错误、server 数量、角色标签、本机文件片段 |
| `servers[]` | 每个解析出的 server 一条：Agent、名称、传输、command、url、来源路径/格式、enabled、本机文件片段 |

不存在的探测路径仍会出现在 `sources` 里，`exists=false`，方便 UI 显示「未发现已知配置文件」。`servers` 按 Agent、名称、路径排序。

## 扫描位置

路径经 `home_dir()` / `agent_home()` 解析，因此 Claude 的 `CLAUDE_CONFIG_DIR`、Pi 的 `PI_CODING_AGENT_DIR`、WorkBuddy 的 `WORKBUDDY_CONFIG_DIR`、ZCode 的 `ZCODE_HOME`、Grok 的 `GROK_HOME` 会被尊重。**不**扫描项目目录下的 `.mcp.json` / `.cursor/mcp.json` / `.grok/config.toml`。

| Agent | 文件 | 格式 | 标签 |
|---|---|---|---|
| Claude | `~/.claude.json` | JSON | Claude 全局 |
| Claude | `<claude-home>/settings.json` | JSON | Claude settings.json |
| Codex | `<codex-home>/config.toml` | TOML | Codex config.toml |
| Grok | `<grok-home>/config.toml` | TOML | Grok config.toml |
| WorkBuddy | `<workbuddy-config>/.mcp.json` | JSON | WorkBuddy .mcp.json |
| Cursor | `~/.cursor/mcp.json` | JSON | Cursor ~/.cursor/mcp.json |
| Cursor | `<cursor-home>/mcp.json`（与上一行相同则合并，只保留第一份） | JSON | Cursor agent mcp.json |
| Pi | `<pi-config>/mcp.json` | JSON | Pi mcp.json |
| Pi | `<pi-config>/.mcp.json` | JSON | Pi .mcp.json |
| Grok / Kimi / DSH / ZCode / Kiro | `<agent-home>/mcp.json` | JSON | 探测 mcp.json |
| Grok / Kimi / DSH / ZCode / Kiro | `<agent-home>/.mcp.json` | JSON | 探测 .mcp.json |

默认 home：Claude `~/.claude`，Codex `~/.codex`，Cursor `~/.cursor`，Pi config `~/.pi/agent`，Grok `~/.grok`，Kimi `~/.kimi-code`（否则 `~/.kimi`），DSH `~/.dsh`，WorkBuddy `~/.workbuddy`，ZCode `~/.zcode`，Kiro `~/.kiro`。

## 解析形状

JSON 接受：

- 根上的 `mcpServers` / `mcp_servers` / `servers`
- `mcp.mcpServers` 或 `mcp.servers`，或 `mcp` 本身像 server map
- 裸的 name → `{command|url|type|args|transport}` map

根对象若含 `theme` / `model` / `permissions` / `env` / `hooks` / `enabledPlugins` / `projects` / `userID` / `oauthAccount`，不把它当裸 server map，避免把 Claude `settings.json` 整份当成 MCP。

TOML **只**读根表 `mcp_servers`（Codex / Grok 形状 `[mcp_servers.name]`）。没有该键则零条 server，不算解析错误。

传输分类：显式 `type`/`transport` 含 sse / http / streamablehttp；否则有 `command` 视为 stdio；否则有 `url` 视为 http；否则 `unknown`。`enabled` 来自 `enabled` 或取反后的 `disabled`（JSON 与 TOML 相同）；没有这些键则空。

## 片段

片段最多 16KiB，内容与本机文件一致，不按字段名打码。这是用户自己的配置；列表、日志和密钥输入框仍走原有遮罩。

## 写入 / 启用

可写 Agent（`writable_mcp_agents`）：Claude（`~/.claude.json`）、Codex（`<codex-home>/config.toml` 的 `mcp_servers`）、Grok（`<grok-home>/config.toml` 的 `mcp_servers`）、Cursor（`~/.cursor/mcp.json`）、WorkBuddy（`<workbuddy-config>/.mcp.json`）。模板目录是本地内置的，没有远程市场，也没有 OAuth。stdio 探测查 PATH；HTTP/SSE 做连通性检查。Codex 无独立 enabled：关闭 = 删除该条目。Grok 关闭写 `enabled = false`，保留条目和 `env`。Grok 可写 stdio 与 HTTP/SSE。

## 当前缺口（实现事实，不是待办承诺）

以下是扫描今天做不到、但厂商已有的形状。补齐属于提案范围，见 [插件管理提案](../proposals/plugin-management.md)。

| 缺口 | 证据 |
|---|---|
| 不读项目级 MCP（`.mcp.json`、`.cursor/mcp.json`、`.codex/config.toml`、`.grok/config.toml`） | 有意：stdio server 会拉起本机进程，项目文件默认不可信 |
| 不枚举 Claude `enabledPlugins` / `~/.claude/plugins/` | 那是 Plugin 包，不是 `mcpServers` 条目 |
| 不枚举 Codex `~/.codex/plugins/cache/` 或 `codex plugin` | Plugin 市场与 `[mcp_servers]` 分离 |
| 不枚举 Grok `~/.grok/plugins/` 或 `grok plugin` | Plugin 与 `[mcp_servers]` 分离 |
| 不调用各家 CLI（`claude mcp`、`codex mcp`、`grok mcp`） | 直接读写配置文件，不走 CLI 的启停或 doctor |
| Kimi / DSH / ZCode / Kiro 仅探测 JSON | 没有已验证的稳定 MCP 契约 |

## 相关页面

- [插件、MCP 与技能](../concepts/plugins-and-mcp.md)
- [Agent 插件表面](agent-plugin-surfaces.md)
- [能力参考](capabilities.md)
