---
title: 插件、MCP 与技能
type: concept
status: current
owner: maintainers
audience: product, frontend, and core contributors
source-of-truth: Capability::Mcp/Skills, mcp_inventory.rs, SkillService, and linked reference pages
updated: 2026-08-26
---

# 插件、MCP 与技能

本页说明 AgentHub 里三类容易混用的「扩展」各自是什么、当前做到哪一步。安装目录、配置格式和各家 CLI 命令的逐项对照见 [Agent 插件表面](../reference/agent-plugin-surfaces.md)；AgentHub 自己的只读扫描契约见 [MCP inventory](../reference/mcp-inventory.md)。尚未承诺的管理模块见 [插件管理提案](../proposals/plugin-management.md)。

## 三类扩展

| 用户说法 | 实际对象 | AgentHub 当前状态 |
|---|---|---|
| 技能 / Skills | 带 `SKILL.md` 的技能包 | **已管理**：共享真源 `~/.agents/skills/`，再投影到各 Agent 技能目录 |
| MCP / 额外能力 | 各 Agent 作为 MCP **客户端**去连接的 server | **只读盘点**：扫描已知配置文件，不安装、不写入、不启停进程 |
| 插件 / Plugin | 各家自己的发行单元（常打包 skills、commands、hooks 和 MCP） | **未接入**：Claude / Codex / Grok 各有 marketplace；AgentHub 不当作统一商店 |

页面文案可以说「插件」，但产品对象必须落到上表某一行。不要把 Skills 页、MCP 页和某家 `/plugin` 命令画成同一个模块。

## 当前产品表面

- **Skills**（`/skills`）是工作区里的技能库与市场。安装、卸载、更新、同步、投影属于 `SkillService` 和 `platform/skills`。`Capability::Skills` 由各 adapter 诚实声明；Kimi 为 Unsupported。
- **MCP**（`/mcp`）是工作区里的只读清单。它列出已发现的 server 名、传输、命令/地址和来源文件。空、加载失败、文件不可读各自有状态。清单存在不等于 AgentHub 能改这份配置。
- `Capability::Mcp` 对全部内置 Agent 都是 **Planned（待验证接入）**。只读 inventory **不得** `registry.require(Mcp)`，也不得把能力矩阵改成 Full。
- 厂商 Plugin 市场（Claude `/plugin`、Codex `/plugins`、Grok `plugin` / `marketplace`）不是当前页面，也不是 Skills 市场（`skills.sh` / `skillhub.cn`）。

## 所有权

MCP server 的 live 配置属于目标 Agent 自己的文件（例如 Claude 的 `mcpServers`、Codex/Grok 的 `[mcp_servers]`、Cursor 的 `mcp.json`）。AgentHub 扫描这些文件时脱敏 `env` / `headers` / 密钥键，不把密钥带进 UI。

技能的共享真源属于 AgentHub：`~/.agents/skills/`。各 Agent 目录里的投影可以是链接或受管副本；卸载共享技能会先拆投影再删源。厂商 Plugin 目录（例如 `~/.claude/plugins/`、`~/.codex/plugins/cache/`、`~/.grok/plugins/`）目前不是 AgentHub 的写入面。

## 和连接 / 路由的边界

MCP 不是登录，也不是本机路由。Connections 管登录；Routes 管 loopback 转发。把一份登录接到某个 Agent，不会自动给那个 Agent 安装 MCP server。反过来，MCP inventory 也不创建 Adapter profile。

## 相关页面

- [MCP inventory](../reference/mcp-inventory.md)
- [Agent 插件表面](../reference/agent-plugin-surfaces.md)
- [能力参考](../reference/capabilities.md)
- [插件管理提案](../proposals/plugin-management.md)
- [UI 页面模式 · MCP](../ui/page-patterns.md#9-agents-and-mcp)
- [添加 Agent](../guides/adding-an-agent.md)
---