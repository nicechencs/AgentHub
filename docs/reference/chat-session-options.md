---
title: Chat 会话选项目录
description: Runtime Options 的 seed / 探测来源与 fail-closed 边界。
type: reference
status: current
owner: maintainers
updated: 2026-09-20
---

# Chat 会话选项目录

Chat「模型 / 思考 / 扩展 / 斜杠原生命令」等来自会话 **Options**（`RuntimeOptions`），与约 80ms 过程快照分开。  
对照 Orca session-option-catalog 的纪律：**探测决定成员资格；菜单与默认项不得靠猜测补齐**。不整表抄对方旗标。

## 代码落点

| 层 | 路径 |
|---|---|
| 类型 | `crates/agenthub-core/src/services/chat_runtime/types.rs`（`RuntimeOptions`、`native_commands`、`session_ready`） |
| 拉取 | `ChatRuntime::options` / `fetch_catalog`：`…/chat_runtime/mod.rs` |
| 前端 | `src/lib/backend/contracts/chat-runtime.ts`；Chat 页 Options / `/` 菜单 |

## 分家来源（`fetch_catalog`）

| Agent（会话绑定） | 模型等目录来源 | 失败时 |
|---|---|---|
| Grok | 活进程 `_x.ai/models/list`；否则 `grok_fallback_catalog` | 回退 seed；空则不当作「已探测权威全表」 |
| Kiro | `fetch_kiro_catalog` | 按实现回退；启动后设置常冻结（换模型需新会话，见 STATUS） |
| Claude | `claude_fallback_catalog`（seed） | 无活探测权威时仅用 seed |
| Codex（及其他走 app-server 探测的路径） | `model/list`、`skills/list`、`plugin/installed` | 探测失败 → 空/缓存策略；不伪装厂商未声明的项 |

原生命令目录（斜杠）由持续通道在会话就绪后注入 `native_commands`；未就绪或目录空 → UI fail-closed（不假装有对方命令）。Grok 走标准 ACP `available_commands_update`；Kiro 在 `session/new` 之后用厂商通知 `_kiro.dev/commands/available` 的 `commands[]`（不要把 `prompts` / 技能、`tools`、`mcpServers` 摊进 `/`）。立刻执行的动作（如新建对话）与对方命令分来源，见 STATUS / Chat 概念页。`transport` 会画在会话设置「这次对话」，不进过程时间线。

## 纪律

1. **未知模型**：不因用户手输就写成「已支持」；继承默认选项的规则须在代码注释与产品文案中可解释。  
2. **launch vs 会话中**：Kiro 等「开始后固定」是产品事实；不要做成中途假切换。  
3. **与深度矩阵**：只有 D2+ 会话才期望丰富 Options；D1 legacy 不假装有 app-server 目录。  

相关：[Chat 支持深度矩阵](chat-support-depth.md)、[身份兼容族](../concepts/agent-identity-families.md)。
