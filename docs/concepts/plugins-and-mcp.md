---
title: 插件、MCP 与技能
type: concept
status: current
owner: maintainers
audience: product, frontend, and core contributors
source-of-truth: SkillService, mcp_inventory.rs, vendor plugin CLIs, and linked reference pages
updated: 2026-08-26
---

# 插件、MCP 与技能

本页区分 AgentHub 里三类扩展。用户说的**插件**是各家 `plugin` / `extension` 包，**不是** MCP server。MCP 仍是独立的 `/mcp` 只读页。尚未落地的插件工作台见 [插件管理提案](../proposals/plugin-management.md)。

## 三类扩展（不要混名）

| 用户界面 | 对象 | 典型厂商入口 | AgentHub 当前 |
|---|---|---|---|
| **插件** | 可安装的 **extension / plugin 包**（常打包 skills、commands、agents、hooks，有时附带 MCP） | Claude `/plugin`、Codex `/plugins`、Grok `plugin`、Pi `pi install` | **未接线**。无页面、无扫描、无能力键 |
| **MCP** | Agent 作为客户端去连接的 **MCP server 条目** | `claude mcp`、`codex mcp`、`grok mcp`、`~/.cursor/mcp.json` | **只读盘点** `/mcp`。`Capability::Mcp` 全部 Planned |
| **技能** | 带 `SKILL.md` 的技能目录 | 各家 `skills/` 目录 | **已管理** `/skills`，共享源 `~/.agents/skills/` |

插件包里可以**含有** MCP，但安装/卸载的对象是整个包。不要把 `/mcp` 改名为插件页，也不要用 MCP inventory 冒充已安装插件列表。

Goose 把 MCP 叫做 “extension”。那是 Goose 的用词。AgentHub 的「插件」对齐 Claude / Codex / Grok / Pi 的 plugin 包，不对齐 Goose 的 MCP 别名。

厂商侧谁有插件系统（AgentHub 均未接线）：**有** = Claude、Grok、Codex、Pi；**另一套、不硬转** = DSH Cordis；**无 / 未验证** = Cursor Agent、Kimi、WorkBuddy。逐项依据见 [Agent 插件表面 · 厂商插件系统](../reference/agent-plugin-surfaces.md#厂商插件系统plugin--extension-包)。同类产品怎么管见 [插件管理提案 · 同类怎么管](../proposals/plugin-management.md#3-同类怎么管2026-08-对照)。

## 当前产品表面

- **Skills**（`/skills`）管理共享技能与投影。`Capability::Skills` 由 adapter 声明；Kimi 为 Unsupported。
- **MCP**（`/mcp`）列出已发现的 server 名、传输、命令/地址、来源文件。清单存在不等于能改配置，更不等于插件已安装。
- **插件**：无 `/plugins` 路由、无侧栏项、无设置开关。厂商目录（`~/.claude/plugins/`、`~/.codex/plugins/`、`~/.grok/plugins/`、Pi packages）尚未扫描。
- 厂商 Plugin 市场也不是 Skills 市场（`skills.sh` / `skillhub.cn`）。

## 所有权

| 对象 | 真源属于谁 |
|---|---|
| 技能共享源 | AgentHub：`~/.agents/skills/` |
| MCP live 配置 | 各 Agent 自己的 json/toml |
| 插件包 | 各 Agent 的 plugin 缓存/目录 + `enabledPlugins` / `[plugins]` / Pi settings |

插件包的 live 状态在各 Agent 自己的目录和 CLI 里。接线方案（委托官方 CLI、不自建商店、不做 `~/.agents/plugins` 共享真源）见 [插件管理提案](../proposals/plugin-management.md)，不是当前能力。

## 和连接 / 路由 / MCP 的边界

- Connections 管登录；Routes 管本机转发。都不安装插件。
- MCP 页管 server 条目；插件页管包。从插件卸掉一个包，可能顺带去掉它附带的 MCP，但那是厂商副作用，不是 MCP 页的删除按钮。
- 凭据落盘加密和国产 OAuth 转 API 仍在范围外。

## 相关页面

- [Agent 插件表面](../reference/agent-plugin-surfaces.md)
- [MCP inventory](../reference/mcp-inventory.md)
- [能力参考](../reference/capabilities.md)
- [插件管理提案](../proposals/plugin-management.md)
- [UI 页面模式](../ui/page-patterns.md)
---