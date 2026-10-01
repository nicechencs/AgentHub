---
title: Chat 会话选项目录
description: Runtime Options 的 seed / 探测来源与 fail-closed 边界。
type: reference
status: current
owner: maintainers
updated: 2026-09-29
---

# Chat 会话选项目录

Chat 的模型、思考、扩展和对方斜杠命令来自会话 **Options**（`RuntimeOptions`），和约 80ms 一次的过程快照分开。规则：**探测到什么才列什么；菜单和默认项不靠猜测补齐**。

## 代码落点

| 层 | 路径 |
|---|---|
| 类型 | `crates/agenthub-core/src/services/chat_runtime/types.rs`（`RuntimeOptions`、`native_commands`、`session_ready`） |
| 拉取 | `ChatRuntime::options` / `fetch_catalog`（`…/chat_runtime/mod.rs`） |
| 前端 | `src/lib/backend/contracts/chat-runtime.ts`；Chat 页 Options 与 `/` 菜单 |

## 各家来源（`fetch_catalog`）

| Agent | 目录来源 | 失败时 |
|---|---|---|
| Codex | 另起一个临时 `codex app-server`，请求 `model/list`、`skills/list`、`plugin/installed` 后关闭 | 返回空目录；不补对方没声明的项 |
| Grok | 活进程 `_x.ai/models/list`；否则 `grok_fallback_catalog` | 回退内置 seed；seed 不算“已探测的完整目录” |
| Kiro | `fetch_kiro_catalog` | 按实现回退 |
| Claude | `claude_fallback_catalog`（内置 seed） | 没有活探测，只用 seed |

对方斜杠命令由持续通道在会话就绪后写进 `native_commands`；未就绪或为空时菜单不画对方命令。Grok 用标准 ACP `available_commands_update`；Kiro 在 `session/new` 之后用厂商通知 `_kiro.dev/commands/available` 的 `commands[]`（`prompts`、技能、`tools`、`mcpServers` 不放进 `/`）。AgentHub 自己的动作（如新建对话）另有来源。当前走哪种接法显示在会话设置「这次对话」，不进过程时间线。

## 规则

1. **未知模型**：用户手输不等于“已支持”；继承默认选项的规则要能在代码注释和产品文案里讲清楚。
2. **生成中不改正在跑的一轮。**
   - Codex：模型和思考强度按轮传给 `turn/start`，换了不重启进程。
   - Grok / Kiro：换模型、思考或权限会在下一轮重新拉起进程，不假装当前进程已切换。Grok 重启后接回原会话（`session/load`，不支持时新开并带上之前的对话）；Kiro 新开会话并带上之前的对话，不悄悄丢上下文。
3. **看深度档**：只有 D2 及以上的会话才期望丰富的 Options；D1 原发送方式不假装有目录。档位见 [Chat 支持深度矩阵](chat-support-depth.md)。

相关：[Agent 身份兼容族](../concepts/agent-identity-families.md)。
