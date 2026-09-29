---
title: Agent 启动与探测清单
description: 各家 detect / build_run_spec / Chat spawn 的源码落点；禁止双源 argv。
type: reference
status: current
owner: maintainers
updated: 2026-09-29
---

# Agent 启动与探测清单

本页是启动参数和探测代码的**索引**，不是第二份可执行配置。改 argv、环境变量或探测逻辑时只改下列源码，不要另建一套运行时参数表。

路径相对 `crates/agenthub-core/src/`：

- 共享探测辅助：`adapters/detect_binary.rs`
- 持续通道进程：`services/chat_runtime/codex_transport.rs`（名字沿用 Codex，Grok / Kiro / Claude 也在这里启动）
- 探测注册：`integrations/agents/<key>/mod.rs`（`register_fn_detector` 等），与 adapter `detect()` 对齐

## 按 Agent

| Agent | detect / `build_run_spec` | Chat 持续进程 | 启动要点 |
|---|---|---|---|
| Claude | `adapters/claude.rs` | `spawn_claude_stream_interruptible` | 新空会话：`-p --input-format stream-json --output-format stream-json --verbose --include-partial-messages --permission-mode …`；旧会话 print + resume |
| Codex | `adapters/codex.rs`（安装副本 `adapters/codex_copies.rs`） | `CodexTransport::spawn`：`codex app-server`，一场对话常驻复用 | 原发送方式：`codex exec --skip-git-repo-check [resume <id>] [--json]` |
| Grok | `adapters/grok.rs` | `spawn_grok*`：`grok agent --no-leader [-m …] [--reasoning-effort …] [--always-approve] stdio` | 参数由 `chat_runtime/ops.rs` 的 `grok_acp_stdio_args` 生成；`--permission-mode` 会让进程退出，不能加 |
| Kiro | `adapters/kiro.rs` | `spawn_kiro*`：`kiro-cli acp [--model …] [--effort …] [--trust-all-tools]` | 原发送方式：`--agent-engine v2` 结构化输出，续聊 `--resume-id`；另有 HTTP 路径（`adapters/kiro/http/`） |
| Kimi | `adapters/kimi.rs` | — | 原发送方式 |
| Pi | `adapters/pi.rs` | — | 原发送方式 |
| WorkBuddy | `adapters/workbuddy.rs` | — | 原发送方式 |
| ZCode | `adapters/zcode.rs` | — | 需要 PATH 上的 `zcode` |
| DSH | `adapters/dsh.rs` | — | GUI 打开走 `src-tauri/src/commands/install.rs`（`dsh web`） |
| Cursor | `adapters/cursor.rs` | — | 默认软隐藏 |

## 规则

1. **单一来源**：CLI / `RunService` 走 `build_run_spec`；持续通道走 `codex_transport.rs` 的 spawn。本页只做索引。
2. **平台特例写进对应 adapter**（或已有共享 helper），不要在页面里 `if agent === …` 拼命令行。
3. Chat 能力深度看 [Chat 支持深度矩阵](chat-support-depth.md)，本页不宣称能力。
4. 将来若引入纯数据的 `LaunchProfile`，也必须由上述源码消费，不能出现第三处拼 argv。

相关：[添加 Agent](../guides/adding-an-agent.md)、[多 Agent 扩家硬约束](../guides/multi-agent-support-rules.md)。
