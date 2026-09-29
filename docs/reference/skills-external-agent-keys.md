---
title: Skills 外部 Agent 命名
description: Hub 技能同步目标 key 与外部生态名的 fail-closed 映射规则。
type: reference
status: current
owner: maintainers
updated: 2026-09-29
---

# Skills 外部 Agent 命名

## AgentHub 内部

- 技能同步的目标键是 `AgentKey`；内置 Agent 的 `AgentKey` 与 `AgentId` 字符串相同（如 `claude`、`codex`）。
- 注册在 `crates/agenthub-core/src/integrations/agents/<key>/mod.rs`，通过 `register_skills_from_home` / `register_skills_from_config_dir`。
- **Kimi**：不注册同步目标，它直接读 `~/.agents/skills`。
- **Kiro**：`Capability::Skills` 为 Planned，没有同步目标。
- **Cursor**：子目录名是 `skills-cursor`（仍是独立的 `AgentId::Cursor`）。

启用到各工具走 `platform/skills` 的链接/复制与归属检查，冲突时 fail closed。见 [插件、MCP 与技能](../concepts/plugins-and-mcp.md)。

## 外部生态名

社区 skills CLI 等使用**另一套** `--agent` 名称（例如 `claude-code`）。AgentHub 桥接这类名称时：

1. 查表；**没把握就返回 `None` 并丢弃，不猜。**
2. 键必须像 Agent 名（字母、数字、`.`、`-`），**不能**以 `-` 开头（避免被当成命令行参数、悄悄清空目标列表）。
3. 实现在 `crates/agenthub-core/src/platform/skills/external_agent_keys.rs`（`skills_cli_agent_key` / `agent_id_for_skills_cli_key` / `is_usable_external_agent_key`）。
4. 外部名（如 `claude-code`）**不是** AgentHub 的 `AgentKey`（仍是 `claude`）；这个模块不改变内置 Agent 的同步主路径。

禁止：自动同步未审核的社区路径表；把外部名直接当 `AgentId` 写进产品身份。

相关：[多 Agent 扩家硬约束](../guides/multi-agent-support-rules.md)、[能力参考](capabilities.md) 的 Skills 行。
