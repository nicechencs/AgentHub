---
title: Chat 与 Agent 运行
type: explanation
status: current
owner: maintainers
audience: chat, adapter, and frontend contributors
source-of-truth: ChatService/RunService, ChatEvent, stream parsers, Tauri Channel adapter, and chat process reducer
updated: 2026-09-05
---

# Chat 与 Agent 运行

## 产品形态

Chat 是 AgentHub 里的运行工作台。当前一个会话对应一个 Agent；同一 turn 内的过程状态仍以 `(turn, agent)` 隔离。新空 Codex 会话使用 app-server 持续聊天，旧会话和其他 Agent 保留原有运行方式。Codex 会话级模型/思考强度选择和统一菜单尚未完成，不以其他 Agent 的现有设置入口代替。

## 当前数据流

新空 Codex 会话：页面 → ChatPort runtime 操作 → Tauri blocking command → ChatRuntime 串行会话 → Codex app-server。后台将消息、事件与终态保存到 SQLite；页面读取带 sequence、待处理请求、currentMessage 与 gap 的快照。正文采用同一次读取中的完整 currentMessage，不能用字符串相似性猜测增量是否重复。页面关闭不拥有后台生命周期；重开使用持久化的原生 thread。详见 [B1 实施记录](../status/chat-codex-b1.md)。

旧会话和其他 Agent：

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

旧发送方式的过程数据主要是内存视图，最终正文和会话消息入库；刷新后不保证过程回放。Codex runtime 另有有限持久化事件、真实确认/问答回复及同机恢复；截断通过 gap 表达，不承诺无限过程历史。问答和文件审批的完整桌面真实验收仍待补，过程内 usage 也未完成。

## Codex 外部安装

Chat 不调用 VS Code 扩展或 Codex 桌面 App 的界面；它启动检测到的 Codex 可执行文件。新会话调用 `codex app-server --stdio`，旧发送方式调用 `codex exec --skip-git-repo-check [--json] …`。检测到安装不等于协议与账号可用；失败明确返回，不静默切换发送方式。

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
