---
title: 多 Agent 扩家硬约束
description: 扩家时必读的短约束：能力分级、身份族、表驱动与 fail-closed。
type: guide
audience: contributor
status: current
owner: maintainers
updated: 2026-09-29
---

# 多 Agent 扩家硬约束

本页只列「多家 Agent 一起支持」的工程约束。产品关闭边以 [产品边界](../decisions/product-boundaries.md) 和根 [AGENTS.md](../../AGENTS.md) 红线为准；具体接入步骤见 [添加 Agent](adding-an-agent.md)。

## 必须

1. **能力声明穷尽**：`capability()` 对 `Capability::ALL` 的 14 个键逐一给出 Full / Partial / Unsupported / Planned 和原因，禁止 `_ =>` 兜底。
2. **深度可见**：进了 Agents 目录不等于 Chat 一样深。改动时同步 [Chat 支持深度矩阵](../reference/chat-support-depth.md)。
3. **身份不等于后端**：产品 `AgentId` 不合并，协议可以复用（见 [身份兼容族](../concepts/agent-identity-families.md)）。软隐藏不等于删掉 adapter。
4. **差异只进 adapter / integrations**：平台 service、页面和通用 utils 不新增 `match AgentId` 功能分支。
5. **Skills 路径要有证据**：只登记验证过的 skills 目录；外部生态名不确定就不登记（见 [Skills 外部 Agent 命名](../reference/skills-external-agent-keys.md)）。
6. **启动参数只有一处来源**：argv / env 以各家 `build_run_spec` 和 Chat 运行时 `codex_transport`（`services/chat_runtime/`）的启动代码为准，先读 [启动清单](../reference/agent-launch-inventory.md)，不另建一套启动构造。
7. **选项 fail-closed**：模型或命令目录没声明、探测失败时，不假装可用（见 [Chat 会话选项](../reference/chat-session-options.md)）。
8. **一场会话一个 Agent**：不做领袖分派，不跨 Agent 搬运原生会话。

## 不要

- 用 PTY 标题或 OSC 序列当 Chat 过程数据来源。
- 为了「支持任意 CLI」放弃能力矩阵。
- 整包移植 Orca 的 Coordinator、worktree 扇出或 Mobile companion。

## 下一步读哪

| 任务 | 文档 |
|---|---|
| 接入一家新 Agent | [添加 Agent](adding-an-agent.md) |
| 看 Chat 有多深 | [Chat 支持深度矩阵](../reference/chat-support-depth.md) |
| 改启动参数 | [启动清单](../reference/agent-launch-inventory.md) |
| Skills 目录与外部名 | [Skills 外部 Agent 命名](../reference/skills-external-agent-keys.md) |
