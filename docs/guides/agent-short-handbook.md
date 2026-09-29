---
title: 自动化 Agent 短手册
type: guide
status: current
owner: maintainers
audience: automation agents and new contributors
updated: 2026-09-29
---

# 自动化 Agent 短手册

本页只补充根 [AGENTS.md](../../AGENTS.md) 没写的禁止项和 Chat 相关阅读入口。红线、分支、前端分层、对用户措辞和「按任务读取」表以 AGENTS.md 为准，这里不重复。产品关闭边原文见 [产品边界](../decisions/product-boundaries.md)。

## 额外禁止

- 不做公网多人网关、动态插件 ABI、第二套领域库。
- 不做 Agent 互调、领袖分派，也不跨 Agent 搬运原生会话。
- 不把伪终端当对话底座。
- 不把协议目录里没有的斜杠项画进 `/` 菜单。
- 不把提案内容抄进 [STATUS](../STATUS.md)。

## Chat 相关该读哪份

| 你在做 | 打开 |
| --- | --- |
| 会话属于谁、何时新建或续聊 | [会话身份](../concepts/chat-session-identity.md) |
| 过程面板、用量小字、事件种类 | [过程事件](../concepts/chat-process-events.md) |
| `/` 菜单、ACP 目录、允许 / 拒绝 | [Chat 与 Agent](../concepts/chat-and-agents.md)、[STATUS](../STATUS.md) |
| 继续加深 Chat | [Chat 统一体验](../proposals/chat-unified-experience.md)（提案，不是现行能力）；已完成的宿主加深记录见 [归档](../archive/chat-host-depth.md) |
| 扩一家新 Agent | [多 Agent 扩家硬约束](multi-agent-support-rules.md) |
