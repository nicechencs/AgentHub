---
title: Agent 身份兼容族
description: 产品身份保留、协议后端可复用；禁止为省事合并 AgentId。
type: concept
status: current
owner: maintainers
updated: 2026-09-29
---

# Agent 身份兼容族

**一句话**：每个 Agent 在界面和存储里的身份（`AgentId`）各自独立；协议相似的可以共用解析和传输代码，但**不要把两家产品合成一个 id**。

## 现行族

“族”只是共用代码的分组，不是产品分组。

| 族 | 成员（身份各自独立） | 共用什么 | 代码线索（`crates/agenthub-core/src/`） |
|---|---|---|---|
| ACP 持续对话 | Grok、Kiro | ACP `session/update` 解码；同一个传输结构，按家分启动参数 | `utils/stream_parse/acp.rs`；`chat_runtime/store.rs` 的 `is_acp_runtime_agent`；`codex_transport.rs` 的 `spawn_grok*` / `spawn_kiro*` |
| app-server | Codex（新空会话） | 常驻 JSON-RPC 进程，一场对话多轮复用 | `CodexTransport::spawn` |
| stream-json | Claude（新空会话） | Claude CLI stream-json 管线 | `spawn_claude_stream_interruptible` |
| 原发送方式 | 其余 Agent 与已有历史的旧会话 | 一次性 Channel 事件流 | `RuntimeChannel::Legacy` |

`CodexTransport` 是历史命名，现在三种持续通道（app-server、ACP、stream-json）都用它的 spawn 方法。各家当前深度见 [Chat 支持深度矩阵](../reference/chat-support-depth.md)。

新加入的 Agent 若走 ACP：复用共享解码与传输骨架，但**新增自己的 `AgentId`、capability、integrations 目录和启动参数**，不要塞进 Grok 或 Kiro 的 id。

## 软隐藏不等于删除

Cursor Agent 的适配器仍在代码与注册表里；`dev` 默认的 store-stamp 软隐藏只影响界面列表，取消隐藏后仍是独立身份。不要为了“少维护”删掉适配器，或把 Cursor 并进别家 id。

## 禁止

- 为共用代码合并两个产品的 `AgentId`。
- 在用户文案里把“协议像”写成“是同一家产品”。
- 在平台层用大 `match AgentId` 复制族逻辑；差异留在 adapter / `chat_runtime` 已有的分派点。

相关：[多 Agent 扩家硬约束](../guides/multi-agent-support-rules.md)。
