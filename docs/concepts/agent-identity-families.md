---
title: Agent 身份兼容族
description: 产品身份保留、协议后端可复用；禁止为省事合并 AgentId。
type: concept
status: current
owner: maintainers
updated: 2026-09-15
---

# Agent 身份兼容族

**一句话**：界面与存储上的 Agent 身份（`AgentId`）保持独立；相似协议可以共用解析器与传输辅助，但**不要把两家产品合成一个 id**。

对照 Orca：OpenClaude 保留独立身份，转录格式可走 Claude 布局；Pi 家族可共享 hook 归一——身份不合并，行为可复用。

## 现行族（实现事实）

| 族 | 成员（身份各自独立） | 共享什么 | 代码线索 |
|---|---|---|---|
| **ACP 持续 Chat** | Grok、Kiro | ACP `session/update` 解码；`CodexTransport` 上分方言 spawn | `utils/stream_parse/acp.rs`；`is_acp_runtime_agent`；`spawn_grok` / `spawn_kiro` |
| **app-server** | Codex（新空） | JSON 控制口持续会话 | `spawn` + app-server 请求 |
| **stream-json** | Claude（新空） | Claude CLI stream-json 管线 | `spawn_claude_stream_*` |
| **legacy / print** | 其余 + 部分旧会话 | Channel 一次性流 | `RuntimeChannel::Legacy` |

新加入家若走 ACP：复用共享解码与传输骨架，**新增自己的 `AgentId`、capability、integrations 路径与 spawn 方言**，不要塞进 Grok 或 Kiro 的 id。

## 软隐藏 ≠ 删除

Cursor Agent 适配器仍在代码与 registry 中；`dev` 默认 store-stamp **软隐藏**只影响界面列表。取消隐藏后仍是独立身份。不要为了「少维护」删掉适配器或把 Cursor 并进别家 id。

## 禁止

- 为共享代码路径合并两个产品的 `AgentId`  
- 把「协议像」写成「产品就是同一家」的用户文案  
- 在平台层用大 `match AgentId` 复制族逻辑（差异留在 adapter / chat_runtime 既有分派点）

相关：[Chat 支持深度矩阵](../reference/chat-support-depth.md)、[多 Agent 扩家硬约束](../guides/multi-agent-support-rules.md)。
