---
title: Chat 会话身份
type: concept
status: current
owner: maintainers
audience: chat and core contributors
source-of-truth: Conversation / ChatService / chat_runtime thread_id
updated: 2026-09-29
---

# Chat 会话身份

本页约定“这一场会话是谁”，让对话页、历史列表和续聊用同一套字段。一场会话只对应一个 Agent；不同 Agent 之间不能共用同一个对方会话 id。

## 字段

代码：`Conversation`（`crates/agenthub-core/src/models/chat.rs`）、`ChatService::create_conversation` / `open_from_session`。

| 含义 | 存在哪 | 说明 |
| --- | --- | --- |
| 哪个 Agent | `conversations.agent_ids`（JSON，产品保证只有一个） | 创建时写入。换 Agent 就是另一场对话，不改写已有行去借别人的会话 id |
| 工作目录 | `conversations.cwd` | 可空。从历史「在对话继续」缺目录不直接失败；发送前缺目录仍会拦住 |
| AgentHub 会话 id | `conversations.id`（`conv-…`） | 对话页与历史列表的主键 |
| 对方会话 id | `conversations.native_session_id` | 各家自己的格式，**不统一改写**。空表示还没接到对方会话。Kiro 打印路径的 HTTP 续聊用 `kiro-http:` 前缀，和 CLI `--resume-id` 不是同一套 id |
| 持续通道线程 | `chat_runtime.thread_id` | 持续通道用的对方线程 id；与 `native_session_id` 相关，但不是同一列 |
| 从哪里进来 | **没有**单独的列 | 对话页走 `ChatService`。CLI `agenthub run` 是一次性多 Agent 运行，不写会话表 |

## 何时新建，何时续聊

| 动作 | 结果 |
| --- | --- |
| 对话页「新建对话」 | 插入新行，`native_session_id` 为空，直到对方给出会话 id |
| 点开历史列表里的已有行 | 续同一 `conversations.id`；持续通道用 `thread_id`，旧会话用 `native_session_id` |
| 按对方会话 id 打开（`open_from_session`） | 先按 `native_session_id` 查找：找到就续该行（空记录可补导入历史）；找不到再新建并写入该 id。同一 Agent、同一目录但对方会话不同，仍新开一条 |
| 默认空会话（`ensure_default_conversation`） | 避免初始化时重复插行；显式新建始终插入 |
| CLI `agenthub run` | 不使用对话页的会话表 |

## 不做

- 不强求各家 `native_session_id` 同一格式或同一生命周期。
- 不跨 Agent 搬运对方会话。
- 不新增 `surface` 列，不做历史回填。

当前产品行为见 [STATUS](../STATUS.md) 与 [Chat 与 Agent 运行](chat-and-agents.md)。
