---
title: Chat 与 Agent 运行
type: explanation
status: current
owner: maintainers
audience: chat, adapter, and frontend contributors
source-of-truth: ChatService/RunService, ChatEvent, stream parsers, Tauri Channel adapter, and chat process reducer
updated: 2026-09-10
---

# Chat 与 Agent 运行

## 产品形态

Chat 是 AgentHub 里的运行工作台。当前一个会话对应一个 Agent；同一 turn 内的过程状态仍以 `(turn, agent)` 隔离。发送按会话隔离，多个会话可以同时生成。

- **新空 Codex 会话**：app-server 持续聊天；会话级模型/思考强度、最小操作菜单、本地图片附件、「用于本次」技能已落地（见 [B2](../archive/chat-codex-b2.md)）。计划模式未做；文本问答等上游默认稳定后再跟，不打开 under-development 开关、也不造假卡片；Linux AgentHub Chat 不可用 Codex Computer Use。Claude B3 首片见下。
- **新空 Grok 会话**：持续聊天，可选模型和思考等级，支持图片与后续轮排队。不能为本轮指定「用于本次」技能（与 Codex 不同；界面也不画出可点的假按钮）。生成时不能中途补充，只能排队到下一轮。
- **新空 Kiro 会话**：`kiro-cli acp` 持续通道（允许/拒绝、停止；生成时不能中途补充，可排队到下一轮）。旧对话保留原发送方式。本机登录或 `KIRO_API_KEY` 可用时，打印路径可走 HTTP 多轮（`kiro-http:` 前缀）；已有 HTTP 会话失败时直接报错并保留会话，不回退成新的命令行会话。企业 IdC / `profileArn` 仍是提案剩余边界（带上参数 ≠ 已验收）。跨页事实见 [STATUS](../STATUS.md)。
- **新空 Claude 会话**：`claude -p --input-format stream-json` 持续通道（多轮、本地图片；不能在生成中补充；本片无允许/拒绝卡片，默认 `dontAsk` / 危险模式 `bypassPermissions`）。有历史的 Claude 会话仍走 print+resume。见 [Claude B3](../archive/chat-claude-b3.md)。
- **其余 Agent 与旧会话**：保留原有发送方式。

## 允许 / 拒绝 / 一直允许

只覆盖 Codex / Grok / Kiro 的持续聊天。没有真实确认通道的 Agent 不会画出可点的假按钮。界面文案是「允许」「拒绝」「一直允许」。

两件不同的事：

1. **记住这次卡片上的选项。** 待处理请求的选项会写入 SQLite。快照或进程重开后，尚未回复的卡片仍显示同一组按钮（包括这次能不能点「一直允许」）。Codex / Grok / Kiro 都这样。
2. **点了「一直允许」之后，后面的确认还问不问。** Codex / Grok / Kiro 都由 AgentHub 在**当前这次对话**里记住，不写进数据库。发给 Codex 的「一直允许」是 `acceptForSession`；一轮结束会新起 `codex app-server`，但本机标记还在，后续同类命令/文件（含另一条工作目录外路径）不再出卡。Grok / Kiro 走 ACP：`session/request_permission` 这次请求自己带了 `allow_always`（Kiro 常见是 `allow_always_tool` / `allow_always_tool_args`）才显示「一直允许」，点了会把对方给的选项回传，并且本机对后续确认自动点允许（同一条 ACP 进程，通常跨多轮）。Grok 工作目录外的本机 `fs/write_text_file` 由 Chat 先出「修改文件」卡片（允许 / 一直允许 / 拒绝）再写文件，点了一直允许后同一对话里后续同类写出不再出卡。没有允许选项时仍出卡片，不会补一个假的「一直允许」。新对话再问。

Kiro 会话设置里的「帮我批准 / 完全访问权限」是启动时的 `--trust-all-tools`，对话开始后不能改；它不是确认卡片上的「一直允许」。跨页事实表见 [STATUS](../STATUS.md)。

## 当前数据流

新空 Codex 会话：页面 → ChatPort runtime 操作 → Tauri blocking command → ChatRuntime 串行会话 → Codex app-server。后台将消息、事件与终态保存到 SQLite；页面读取带 sequence、待处理请求、currentMessage 与 gap 的快照。正文采用同一次读取中的完整 currentMessage，不能用字符串相似性猜测增量是否重复。活动会话约 80ms 读一次快照，让已写入的增量尽快出现；首字前显示「正在想」，正文出现后显示「正在写」。不把整段回复拆开假装逐字打出。页面关闭不拥有后台生命周期；重开使用持久化的原生 thread。详见 [B1 实施记录](../archive/chat-codex-b1.md)。

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
  → 前端 reducer：正文 + 主列过程摘要（点开右侧栏看思考/工具；协议字段折叠）
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

Claude、Codex、Kimi、Grok、Pi、Kiro 当前可走 `ProcessMode::Auto` 的结构化解析；WorkBuddy 与 ZCode 没有结构化 parser 时按 text 展示。Kiro 须 `--agent-engine v2`，v1 会拒绝 stream-json。新交互对话走 `kiro-cli acp`；旧打印路径在本机登录或 `KIRO_API_KEY` 可用时可走 HTTP，已有 HTTP 会话失败不改走命令行。ZCode 对话需要 PATH 上的 `zcode`；只装了桌面端时不能凭空当成命令行。DeepSeek Harness 的 StructuredStream 仍是 Planned。**Cursor Agent 默认软隐藏**，结构化输出与登录写入等兼容项修复完成前不在 Chat 等页面开放。解析失败降级为 raw/text 事件，不因某一行 JSON 不可识别而丢弃整次对话；CLI 不支持 flag 时不得静默重试成另一种语义。

旧发送方式的过程数据主要是内存视图，最终正文和会话消息入库；刷新后不保证过程回放。Codex runtime 另有有限持久化事件、真实确认/问答回复及同机恢复；截断通过 gap 表达，不承诺无限过程历史。各项验收分开写：命令批准 Linux 真窗已验允许/拒绝；文件审批 Linux 真窗已验允许/拒绝；关窗续聊 Linux / macOS 已验。记住允许/拒绝与「一直允许」已接线，范围见上文：三家都在当前这次对话里记住后续确认（Codex 跨轮；Grok / Kiro 同一条 ACP 进程可跨轮）。文本问答的协议和界面已映射，但 Codex 0.148 Default 默认不发出 `item/tool/requestUserInput`；产品决定等上游默认稳定后再跟，不把 under-development 开关做成产品默认，也不造假问答卡片。Linux AgentHub Chat 不可用 Codex Computer Use。文件审批已接线（允许/拒绝）；工作目录外（含 `/tmp`）的 `apply_patch` 出「修改文件」卡片，可点一直允许。过程内用量：Codex 解析当前轮 `last`；Grok 解析当前轮 `turn_completed.usage`。对话里只在本轮结束后用小字写输入 / 输出，生成中不画；不把累计或窗口写进对话。ACP 某轮不带 usage 时不画假数字。不估算费用。Kiro 无 token 累计数据源。图片附件在 ChatRuntime 持续聊天（Codex / Grok / Kiro / 新空 Claude）露出；有历史的旧 Claude 仍走 print+resume，**无**图片按钮。普通文件与 `@` 未接。见 [STATUS](../STATUS.md)。

## Codex 外部安装

Chat 不调用 VS Code 扩展或 Codex 桌面 App 的界面；它启动检测到的 Codex 可执行文件。新会话调用 `codex app-server --stdio`，旧发送方式调用 `codex exec --skip-git-repo-check [--json] …`。检测到安装不等于协议与账号可用；失败明确返回，不静默切换发送方式。

| 安装来源 | AgentHub 如何识别 | Chat 前置条件 |
| --- | --- | --- |
| npm 全局 | PATH 或 `npm prefix -g` / 常见目录 | `~/.codex/auth.json` 有效；已选工作目录；Agent 未隐藏 |
| VS Code / Cursor 插件 | 扫描 `openai.chatgpt-*` 扩展目录 | 同上 |
| Windows / macOS 桌面 App | 扫描 `%LOCALAPPDATA%\\Programs\\OpenAI\\Codex` 或 `/Applications/Codex.app` | 同上 |

IDE/桌面副本在 Agents 页标记为「在 IDE/桌面 App 内更新」；不影响 Chat，只要 detect 为已安装且登录态可用。

审查与修复状态见 [Codex 安装与模块化审查](../archive/codex-install-modularity-review.md)。

## Agent 能力边界

Agent catalog/registry 描述安装、配置、账号、skills、usage、runtime、projects、stream 等能力；能力等级是调用门禁。`StructuredStream` 只决定是否可启用结构化过程，不代表该 Agent 的所有 Chat 特性都已实现。未知或 unsupported 必须呈现明确状态，不静默用另一个 Agent 的 parser 或 mock。

## 相关页面

- [当前实现状态](../STATUS.md)
- [Chat 体验标杆](../ui/chat-experience-bar.md)
- [Codex Chat B2](../archive/chat-codex-b2.md)
- [Architecture overview](../architecture/overview.md)
- [Core and runtime](../architecture/core-runtime.md)
- [Frontend and backend boundary](../architecture/frontend-backend.md)
- [Legacy document index](../archive/legacy-document-index.md)
