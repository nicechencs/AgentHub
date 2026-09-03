---
title: MCP inventory
description: AgentHub 只读 MCP 扫描的路径、格式、片段和已知缺口。
type: reference
audience: contributor
status: current
updated: 2026-08-30
---

# MCP inventory

本页是 `list_mcp_inventory` 的现行契约。实现在 `crates/agenthub-core/src/services/mcp_inventory.rs`，Tauri command 为 `list_mcp_inventory`。这是 **MCP server 条目** 的检查，不是插件（extension / plugin）包，也不是 `Capability::Mcp` 管理。插件包见 [Agent 插件表面](agent-plugin-surfaces.md) 与 [插件管理提案](../proposals/plugin-management.md)。

## 返回结构

| 字段 | 含义 |
|---|---|
| `sources[]` | 每个已知配置文件一条：路径、是否存在、是否可读、解析错误、server 数量、角色标签、本机文件片段 |
| `servers[]` | 每个解析出的 server 一条：Agent、名称、传输、command、url、来源路径/格式、enabled、本机文件片段 |

不存在的探测路径仍会出现在 `sources` 里，`exists=false`，方便 UI 显示「未发现已知配置文件」。`servers` 按 Agent、名称、路径排序。

## 扫描位置

路径经 `home_dir()` / `agent_home()` 解析，因此 Claude 的 `CLAUDE_CONFIG_DIR`、Pi 的 `PI_CODING_AGENT_DIR`、WorkBuddy 的 `WORKBUDDY_CONFIG_DIR`、ZCode 的 `ZCODE_HOME` 会被尊重。**不**扫描项目目录下的 `.mcp.json` / `.cursor/mcp.json` / `.grok/config.toml`。

| Agent | 文件 | 格式 | 标签 |
|---|---|---|---|
| Claude | `~/.claude.json` | JSON | Claude 全局 |
| Claude | `<claude-home>/settings.json` | JSON | Claude settings.json |
| Codex | `<codex-home>/config.toml` | TOML | Codex config.toml |
| WorkBuddy | `<workbuddy-config>/.mcp.json` | JSON | WorkBuddy .mcp.json |
| Cursor | `~/.cursor/mcp.json` | JSON | Cursor ~/.cursor/mcp.json |
| Cursor | `<cursor-home>/mcp.json`（与上一行相同则合并，只保留第一份） | JSON | Cursor agent mcp.json |
| Pi | `<pi-config>/mcp.json` | JSON | Pi mcp.json |
| Pi | `<pi-config>/.mcp.json` | JSON | Pi .mcp.json |
| Grok / Kimi / DSH / ZCode | `<agent-home>/mcp.json` | JSON | 探测 mcp.json |
| Grok / Kimi / DSH / ZCode | `<agent-home>/.mcp.json` | JSON | 探测 .mcp.json |

默认 home：Claude `~/.claude`，Codex `~/.codex`，Cursor `~/.cursor`，Pi config `~/.pi/agent`，Grok `~/.grok`，Kimi `~/.kimi-code`（否则 `~/.kimi`），DSH `~/.dsh`，WorkBuddy `~/.workbuddy`，ZCode `~/.zcode`。

## 解析形状

JSON 接受：

- 根上的 `mcpServers` / `mcp_servers` / `servers`
- `mcp.mcpServers` 或 `mcp.servers`，或 `mcp` 本身像 server map
- 裸的 name → `{command|url|type|args|transport}` map

根对象若含 `theme` / `model` / `permissions` / `env` / `hooks` / `enabledPlugins` / `projects` / `userID` / `oauthAccount`，不把它当裸 server map，避免把 Claude `settings.json` 整份当成 MCP。

TOML **只**读根表 `mcp_servers`（Codex 形状 `[mcp_servers.name]`）。没有该键则零条 server，不算解析错误。

传输分类：显式 `type`/`transport` 含 sse / http / streamablehttp；否则有 `command` 视为 stdio；否则有 `url` 视为 http；否则 `unknown`。`enabled` 来自 `enabled` 或取反后的 `disabled`；Codex TOML 当前不填 enabled。

## 片段

片段最多 16KiB，内容与本机文件一致，不按字段名打码。这是用户自己的配置；列表、日志和密钥输入框仍走原有遮罩。

## 当前缺口（实现事实，不是待办承诺）

这些是 scanner 今天做不到、但厂商文档已经存在的形状。补齐属于提案切片，见 [插件管理](../proposals/plugin-management.md)。

| 缺口 | 证据 |
|---|---|
| 不读 Grok `~/.grok/config.toml` 的 `[mcp_servers]` | Grok 官方配置是 TOML；scanner 只探 JSON `mcp.json` |
| 不读项目级 MCP（`.mcp.json`、`.cursor/mcp.json`、`.codex/config.toml`、`.grok/config.toml`） | 有意：stdio server 会拉起本机进程，项目文件默认不可信 |
| 不枚举 Claude `enabledPlugins` / `~/.claude/plugins/` | 那是 Plugin 包，不是 `mcpServers` 条目 |
| 不枚举 Codex `~/.codex/plugins/cache/` 或 `codex plugin` | Plugin 市场与 `[mcp_servers]` 分离 |
| 不枚举 Grok `~/.grok/plugins/` 或 `grok plugin` | Plugin 与 `[mcp_servers]` 分离 |
| 不调用各家 CLI（`claude mcp`、`codex mcp`、`grok mcp`） | 只读文件，不启停、不 doctor |
| Kimi / DSH / ZCode 仅探测 JSON | 没有已验证的稳定 MCP 契约 |

## 相关页面

- [插件、MCP 与技能](../concepts/plugins-and-mcp.md)
- [Agent 插件表面](../reference/agent-plugin-surfaces.md)
- [能力参考](../reference/capabilities.md)
---