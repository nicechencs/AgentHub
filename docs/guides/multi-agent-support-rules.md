---
title: 多 Agent 扩家硬约束
description: 扩家时必读的短约束：深度阶梯、身份族、表纪律与 fail-closed。
type: guide
audience: contributor
status: current
owner: maintainers
updated: 2026-09-15
---

# 多 Agent 扩家硬约束

半屏可读。产品关闭边仍以 [产品边界](../decisions/product-boundaries.md) 与根 `AGENTS.md` 红线为准。本页只压 **多家 Agent 支持工程** 约束（对照 Orca 的阶梯/表驱动文化，不照搬 IDE/编排）。

## 必须

1. **Capability 穷尽**：`capability()` 对 14 键给明确 Full/Partial/Unsupported/Planned + 原因；禁止 `_ =>` 兜底。  
2. **深度可见**：能进 Agents 目录 ≠ Chat 一样深。更新或查阅 [Chat 支持深度矩阵](../reference/chat-support-depth.md)。  
3. **身份 ≠ 后端**：产品 `AgentId` 不合并；协议可复用（见 [身份兼容族](../concepts/agent-identity-families.md)）。软隐藏 ≠ 删适配器。  
4. **差异进 adapter / integrations**：平台 service、页面、通用 utils **不**散落新的 `match AgentId` 功能分支。  
5. **Skills 路径有证据**：只注册已验证的 skills 根；外部生态名不确定 → 丢弃（见 [Skills 外部 Agent 命名](../reference/skills-external-agent-keys.md)）。  
6. **启动真源单一**：argv/env 以各家 `build_run_spec` 与 Chat `codex_transport` spawn 为准；先读 [启动清单](../reference/agent-launch-inventory.md)，本波不平行再建一套 launch builder。  
7. **Options fail-closed**：模型/命令目录未声明或探测失败，不伪装可用（见 [Chat 会话选项](../reference/chat-session-options.md)）。  
8. **一会话一 Agent**：不做领袖分派、跨 Agent 搬运原生会话。

## 不要

- 以 PTY 标题/OSC 当 Chat 过程真源  
- 为「支持任意 CLI」放弃能力矩阵  
- 凭据落盘加密、国产 OAuth→API、动态插件 ABI  
- 整包移植 Orca Coordinator / worktree 扇出 / Mobile companion  

## 下一步读哪

| 任务 | 文档 |
|---|---|
| 接入一家新 Agent | [添加 Agent](adding-an-agent.md) |
| 看 Chat 有多深 | [Chat 支持深度矩阵](../reference/chat-support-depth.md) |
| 改启动参数 | [启动清单](../reference/agent-launch-inventory.md) |
| Skills 投影 / 外部名 | [Skills 外部 Agent 命名](../reference/skills-external-agent-keys.md) |
