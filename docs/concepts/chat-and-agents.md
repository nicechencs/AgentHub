---
title: Chat 与 Agent 运行
type: explanation
status: current
owner: maintainers
audience: chat, adapter, and frontend contributors
source-of-truth: ChatService/RunService, ChatEvent, stream parsers, Tauri Channel adapter, and chat process reducer
updated: 2026-08-29
---

# Chat 与 Agent 运行

## 产品形态

Chat 是 AgentHub 里的运行工作台，不是统一替换各家 CLI 原生会话的协议层。当前一个会话对应一个 Agent；同一 turn 内的过程状态仍以 `(turn, agent)` 隔离，避免命令、stderr、tool/thinking 步骤混成一条时间线。Chat 没有独立模型选择器；模型与参数由目标 Agent 的配置/运行契约决定。

## 当前数据流

```text
页面 Chat composer
  → lib/api/chat（生产 façade）
  → backend.chat.send
  → Tauri command + Channel<ChatEvent>
  → ChatService::send
  → RunService::run_each
  → adapter build_run_spec(ProcessMode::Auto)
  → StreamingProcessRunner
  → stream_parse/*（能结构化则结构化，否则 text）
  → RunEvent → ChatEvent
  → 前端 reducer：正文 + 过程面板
```

Tauri transport 使用 `ipc::Channel<ChatEvent>`，不是 SSE。阻塞进程执行在 command/core 的 blocking 边界隔离。浏览器 `dev:mock` 与 Vitest 通过 `src/dev/mocks/chat.ts` 提供相同 port 契约的可控事件；`src/lib/api/chat.ts` 不是 mock。

## 统一事件语义

前端处理的稳定事件包括：

- turn/agent queued 与 started；
- Agent 启动命令；
- stdout 文本片段（进入正文，也进入过程视图）；
- stderr/诊断（进入过程视图，不进入正文气泡）；
- 结构化 process step（thinking、tool、状态等，取决于 Agent parser）；
- Agent finished、cancelled、failed 与整体 finished/error。

Claude、Codex、Kimi、Grok、Pi 当前可走 `ProcessMode::Auto` 的结构化解析；WorkBuddy 与 ZCode 没有结构化 parser 时按 text 展示。ZCode 对话需要 PATH 上的 `zcode`；只装了桌面端时不能凭空当成命令行。DeepSeek Harness 的 StructuredStream 仍是 Planned。**Cursor Agent 默认软隐藏**，结构化输出与登录写入等兼容项修复完成前不在 Chat 等页面开放。解析失败降级为 raw/text 事件，不因某一行 JSON 不可识别而丢弃整次对话；CLI 不支持 flag 时不得静默重试成另一种语义。

过程数据目前主要是内存视图：最终 assistant 文本和会话消息入库，命令、stderr、步骤在刷新后不保证可回放。过程步骤落库、过程内 usage、交互式 tool approval 和完整原生多轮 session 不属于当前契约。

## Codex 外部安装

Chat **不**调用 VS Code 扩展或 Codex 桌面 App 的 UI/API；它 spawn 检测到的 `codex` CLI（`codex exec --skip-git-repo-check [--json] …`）。

| 安装来源 | AgentHub 如何识别 | Chat 前置条件 |
| --- | --- | --- |
| npm 全局 | PATH 或 `npm prefix -g` / 常见目录 | `~/.codex/auth.json` 有效；已选工作目录；Agent 未隐藏 |
| VS Code / Cursor 插件 | 扫描 `openai.chatgpt-*` 扩展目录 | 同上 |
| Windows / macOS 桌面 App | 扫描 `%LOCALAPPDATA%\\Programs\\OpenAI\\Codex` 或 `/Applications/Codex.app` | 同上 |

IDE/桌面副本在 Agents 页标记为「在 IDE/桌面 App 内更新」；不影响 Chat，只要 detect 为已安装且登录态可用。

审查与修复状态见 [Codex 安装与模块化审查](../status/codex-install-modularity-review.md)。

## Agent 能力边界

Agent catalog/registry 描述安装、配置、账号、skills、usage、runtime、projects、stream 等能力；能力等级是调用门禁。`StructuredStream` 只决定是否可启用结构化过程，不代表该 Agent 的所有 Chat 特性都已实现。未知或 unsupported 必须呈现明确状态，不静默用另一个 Agent 的 parser 或 mock。

## 相关页面

- [Architecture overview](../architecture/overview.md)
- [Core and runtime](../architecture/core-runtime.md)
- [Frontend and backend boundary](../architecture/frontend-backend.md)
- [Legacy document index](../archive/legacy-document-index.md)
