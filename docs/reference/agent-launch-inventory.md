---
title: Agent 启动与探测清单
description: 各家 detect / build_run_spec / Chat spawn 的源码落点；禁止双源 argv。
type: reference
status: current
owner: maintainers
updated: 2026-09-15
---

# Agent 启动与探测清单

本页是 **现行启动差异的地图**，不是第二套可执行配置表。  
改 argv / env / 探测时，只改下列真源；不要平行新增 Orca 式 `TUI_AGENT_CONFIG` 运行时 builder（学习其穷尽表达可以，双源不可以）。

共享探测辅助：`crates/agenthub-core/src/adapters/detect_binary.rs`。  
Chat 持续进程：`crates/agenthub-core/src/services/chat_runtime/codex_transport.rs`。

## 按 Agent

| Agent | detect / run_spec | Chat 持续 spawn（若有） | 提示 / 启动形态要点 |
|---|---|---|---|
| Claude | `adapters/claude.rs` | `spawn_claude_stream_*`（新空） | 持续：`-p` + stream-json；旧会话 print+resume |
| Codex | `adapters/codex.rs` | `CodexTransport::spawn`（app-server） | headless `build_run_spec` 与 app-server 路径分离 |
| Grok | `adapters/grok.rs` | `spawn_grok*`（`grok agent … stdio`） | ACP；旗标顺序敏感（见 STATUS） |
| Kiro | `adapters/kiro.rs` | `spawn_kiro*`（`kiro-cli acp`） | 新空 ACP；旧 headless/HTTP 另路径 |
| Kimi | `adapters/kimi.rs` | —（legacy Chat） | |
| Pi | `adapters/pi.rs` | — | |
| WorkBuddy | `adapters/workbuddy.rs` | — | |
| ZCode | `adapters/zcode.rs` | — | |
| DSH | `adapters/dsh.rs` | — | GUI 打开另见 `src-tauri` launch（如 web） |
| Cursor | `adapters/cursor.rs` | — | 软隐藏；半表面 |

Integrations 侧探测注册：`integrations/agents/<key>/mod.rs`（`register_fn_detector` 等），与 adapter `detect()` 对齐，不各写各的「是否已装」故事。

## 纪律

1. **单一真源**：CLI/`RunService` 走 `build_run_spec`；持续 Chat 走 `codex_transport` spawn。清单仅索引。  
2. **平台特例写进该 adapter**（或已有共享 helper），不要在页面 `if agent ===` 拼命令行。  
3. **深度**见 [Chat 支持深度矩阵](chat-support-depth.md)；本页不宣称 Chat 能力。  
4. 将来若引入纯数据 `LaunchProfile`，必须由上述真源消费，禁止第三处拼 argv。

相关：[添加 Agent](../guides/adding-an-agent.md)、[多 Agent 扩家硬约束](../guides/multi-agent-support-rules.md)。
