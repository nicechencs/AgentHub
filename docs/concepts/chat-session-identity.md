---
title: Chat 会话身份
type: concept
status: current
owner: maintainers
audience: chat and core contributors
source-of-truth: Conversation / ChatService / chat_runtime thread_id
updated: 2026-09-15
---

# Chat 会话身份

约定「这一次会话是谁」，让对话页、历史列表和续聊用同一套字段解释。不照搬其它产品的字符串格式，也不做一次大迁移。一会话一 Agent；禁止跨 Agent 共用同一份对方原生会话 id。

## 字段（Hub 侧）

| 含义 | 存哪里 | 说明 |
| --- | --- | --- |
| 谁（Agent） | `conversations.agent_ids`（JSON；产品保证一条） | 创建时写入。切换 Agent 是另一场对话，不改写已有行去「借用」别人的原生 id。 |
| 哪（工作目录） | `conversations.cwd` | 可空。历史「在对话继续」缺目录不硬失败；发送前缺目录仍是阻断。 |
| Hub 会话 id | `conversations.id`（`conv-…`） | 对话页与历史列表的主键。 |
| 对方原生会话 id | `conversations.native_session_id` | 各家自己的 id，语义不统一，**不要改写成同一规则**。空表示还没接到对方会话。Kiro 打印路径 HTTP 续聊用 `kiro-http:` 前缀，与 CLI `--resume-id` 不是同一命名空间。 |
| 持续通道线程 | `chat_runtime.thread_id` | 新空持续聊天的运行时线程，和打印路径的 `native_session_id` 相关但不是同一列。 |
| 从哪进 | **没有**独立列 | 对话页走 `ChatService`；CLI `agenthub run` 是一次性多 Agent 跑，不写入上述会话行，也不带 `native_session_id`。不要为 IM/cron 造第二套表面。 |

对照代码：`Conversation`（`crates/agenthub-core/src/models/chat.rs`）、`ChatService::create_conversation` / `open_from_session`。

## 何时新建，何时续聊

| 动作 | 结果 |
| --- | --- |
| 对话页「新建对话」 | 插入新行：`native_session_id` 为空，直到对方给出会话 id。 |
| 点开历史列表里已有行 | 续同一 `conversations.id`。持续通道用运行时线程；旧会话用已存的 `native_session_id` 解释标题和续聊。 |
| 按对方会话 id 打开（`open_from_session`） | 先用 `native_session_id` 查找：命中则续该行（空记录可补导入历史）；未命中再新建并写入该 id。 |
| 默认空会话 `ensure_default_conversation` | 避免初始化重复插行；显式新建仍始终插入。 |
| CLI `agenthub run` | 不复用对话页会话表。 |

## 不做

- 不强迫各家 `native_session_id` 同一格式或同一生命周期。
- 不跨 Agent 搬原生会话。
- 本波不新增 `surface` 列、不做历史回填。

现行产品行为仍以 [STATUS](../STATUS.md) 和 [Chat 与 Agent](chat-and-agents.md) 为准。
