---
title: 插件、MCP 与技能
type: concept
status: current
owner: maintainers
audience: product, frontend, and core contributors
source-of-truth: SkillService, mcp_inventory.rs, mcp_manage.rs, plugin_inventory.rs, plugin_apply.rs, vendor plugin CLIs, and linked reference pages
updated: 2026-10-03
---

# 插件、MCP 与技能

AgentHub 当前界面里有三类容易混淆的扩展内容。用户说的**插件**是各家的 plugin / extension 包，**不是** MCP server，也不是提案中的 AgentHub 官方扩展。各家厂商侧的细节见 [Agent 插件表面](../reference/agent-plugin-surfaces.md)；官方扩展候选见[模块化提案](../proposals/modularity.md#8-功能模块与官方扩展)，当前没有对应的插件平台或商店。

## 三类扩展

| 界面叫法 | 是什么 | AgentHub 现在能做什么 |
|---|---|---|
| **插件**（`/plugins`） | 可安装的包，常带 skills、commands、agents、hooks，有时附带 MCP | 列出 Claude / Grok / Pi 已装的包；Claude / Grok 可安装、卸载、启用、停用。Codex 为 Planned；其余不支持。没有 `Capability::Plugins` |
| **MCP**（`/mcp`） | Agent 作为客户端去连的 MCP server 条目 | 盘点各家配置文件；Claude / Codex / Grok / Cursor / WorkBuddy 可探测并写入、启用、关闭（无 OAuth），`Capability::Mcp` 对这五家是 Partial |
| **技能**（`/skills`） | 带 `SKILL.md` 的技能目录 | 用户技能共享库 `~/.agents/skills/`，再同步到各 Agent 自己的目录；项目技能在所选工作区的 `.agents/skills/` |

插件包里可以**含有** MCP，但安装/卸载的是整个包。不要把 `/mcp` 改名为插件页，也不要用 MCP 盘点冒充已装插件列表。Goose 把 MCP 叫 “extension”，那是 Goose 的用词；AgentHub 的「插件」对齐 Claude / Codex / Grok / Pi 的包。厂商 plugin 市场也不是技能市场（`skills.sh` / `skillhub.cn`）。

## 各页面的要点

- **技能**：`Capability::Skills` 由 adapter 声明。Kimi 是 Partial：它直接读共享库，AgentHub 不再同步一份。项目技能按「历史」页已识别的工作区选择。
- **MCP**：列出 server 名、传输方式、命令/地址和来源文件。Codex 关闭即删除条目；Grok 关闭写 `enabled = false`。不含 OAuth，也不等于插件已安装。扫描范围见 [MCP inventory](../reference/mcp-inventory.md)。
- **插件**：
  - Claude / Grok 优先读官方 CLI 的 JSON，否则读本机目录（`~/.claude/plugins/` + `enabledPlugins`，`~/.grok/plugins/`）。
  - 安装要先确认：Grok 从官方市场、git 或本地路径装，确认后才带 `--trust` 调 `grok plugin install`；Claude 安装 `name@marketplace`，确认后带 `-y`。卸载默认保留插件数据目录。
  - Pi 没有列表 JSON，读 `~/.pi/agent/settings.json` 的 `packages`（及 npm/git 安装目录），对比本机版本与配置里的指定版本；不查线上最新版。Pi 装上即加载，没有包级启用，也不能从本页安装。
  - 设置「显示插件页面」只控制侧栏入口。附带的 MCP 只作为包内组件显示。
  - 更新和 Codex / Pi 写入仍是[提案](../proposals/plugin-management.md)。

## 谁拥有真实状态

| 对象 | 真实状态在哪 |
|---|---|
| 技能 | AgentHub：用户技能 `~/.agents/skills/`，项目技能 `<工作区>/.agents/skills/` |
| MCP 配置 | 各 Agent 自己的 json / toml |
| 插件包 | 各 Agent 的插件目录和启用列表（`enabledPlugins`、`[plugins]`、Pi settings） |

## 与连接、路由的边界

- Connections 和 Routes 都不装插件。
- 从插件页卸掉一个包，可能顺带去掉它附带的 MCP；那是厂商行为，不是 MCP 页的删除。

## 相关页面

- [Agent 插件表面](../reference/agent-plugin-surfaces.md)
- [MCP inventory](../reference/mcp-inventory.md)
- [能力参考](../reference/capabilities.md)
- [插件管理提案](../proposals/plugin-management.md)
- [模块化提案：功能模块与官方扩展](../proposals/modularity.md#8-功能模块与官方扩展)
- [UI 页面模式](../ui/page-patterns.md)
