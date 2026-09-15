---
title: Skills 外部 Agent 命名
description: Hub 投影 key 与外部生态名的 fail-closed 映射纪律。
type: reference
status: current
owner: maintainers
updated: 2026-09-15
---

# Skills 外部 Agent 命名

## Hub 内部（现行）

- 投影目标键 = `AgentKey`，对内建 Agent 与 `AgentId` 字符串一致（如 `claude`、`codex`）。  
- 注册：`integrations/agents/<key>/` 经 `register_skills_from_home` / `register_skills_from_config_dir`。  
- **Kimi**：不注册 Hub 投影目标（对方读 `~/.agents/skills`）。  
- **Kiro**：`Capability::Skills` 为 Planned，无 skills 目标。  
- **Cursor**：子目录名 `skills-cursor`（仍是独立 `AgentId::Cursor`）。

启用到各工具走 `platform/skills` 的 link/copy 投影与 ownership；冲突 fail-closed。详见 [插件、MCP 与技能](../concepts/plugins-and-mcp.md)。

## 外部生态名（fail-closed）

社区 skills CLI 等使用**另一套** `--agent` 命名（例如 Orca 对照里 `claude` → `claude-code`，不确定则 `null`）。  
AgentHub 若桥接这类外部名：

1. 查表；**没有把握 → `None`，丢弃，不猜测。**  
2. 键形状必须像 agent 名（字母数字与 `.-`）；**禁止**以 `-` 开头的值（避免被当成旗标、静默清空目标列表）。  
3. 实现：`crates/agenthub-core/src/platform/skills/external_agent_keys.rs`（`skills_cli_agent_key` / `agent_id_for_skills_cli_key` / `is_usable_external_agent_key`）。  
4. **本波不改变**对内建 `AgentId` 的投影主路径；模块供桥接与纪律钉住。外部名（如 `claude-code`）**不是** Hub 投影用的 `AgentKey`（Hub 仍是 `claude`）。

禁止：自动同步未审核的社区 path 表；把外部名直接当 `AgentId` 写入产品身份。

相关：[多 Agent 扩家硬约束](../guides/multi-agent-support-rules.md)、能力矩阵 Skills 行。
