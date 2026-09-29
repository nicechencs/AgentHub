---
title: Chat 与 Agent 运行
type: explanation
status: current
owner: maintainers
audience: chat, adapter, and frontend contributors
source-of-truth: ChatService/RunService, ChatRuntime, ChatEvent, stream parsers, Tauri Channel adapter, and chat process reducer
updated: 2026-09-29
---

# Chat 与 Agent 运行

本页解释 Chat 怎样启动各家 Agent、怎样把对方的输出变成对话和过程。相关页：

- 会话是谁、何时新建或续聊：[Chat 会话身份](chat-session-identity.md)
- 过程面板里有哪些步骤：[Chat 过程事件](chat-process-events.md)
- 每家 Chat 做到多深：[Chat 支持深度矩阵](../reference/chat-support-depth.md)
- 模型、思考等选项从哪来：[Chat 会话选项目录](../reference/chat-session-options.md)
- 跨页的当前实现事实：[STATUS](../STATUS.md)

## 产品形态

一个会话对应一个 Agent；发送按会话隔离，多个会话可以同时生成。同一轮内的过程状态按 `(turn, agent)` 隔离。

“新空会话”指还没有对方历史的新对话。四家新空会话走**持续通道**（同一个对方进程跑多轮），其余 Agent 和已有历史的旧会话走**原发送方式**（每轮起一次命令行，拿一次性输出）。

| Agent | 新空会话怎么跑 | 要点 |
| --- | --- | --- |
| Codex | 常驻 `codex app-server` | 同一场对话多轮共用一个进程；换模型或思考强度不重启。换登录、换程序路径或文件夹、关掉「一直允许」、进程退出或空闲 10 分钟后，下一轮才重新打开并接回原会话。点停止只打断这一轮（`turn/interrupt`），不关进程。支持模型/思考强度、最小操作菜单、本地图片、「用于本次」技能（见 [B2](../archive/chat-codex-b2.md)）。计划模式未做；文本问答等上游默认稳定再跟，不打开 under-development 开关、不造假卡片；Linux 上不可用 Codex Computer Use |
| Grok | `grok agent … stdio`（ACP） | 可选模型和思考等级；支持图片；生成中不能补充，只能排队到下一轮；不支持「用于本次」技能，界面也不画假按钮 |
| Kiro | `kiro-cli acp`（ACP） | 允许/拒绝、停止；生成中不能补充，可排队。旧对话保留原发送方式，不能切到 ACP |
| Claude | `claude -p --input-format stream-json` | 多轮、本地图片、正文边写边出；停止只打断这一轮；生成中不能补充；没有允许/拒绝卡片，默认 `dontAsk`，危险模式 `bypassPermissions`。TodoWrite / Task 工具更新的任务清单进输入区上方的计划条（与 Grok / Kiro 相同），不进过程时间线。见 [Claude B3](../archive/chat-claude-b3.md) |
| 其余 Agent | 原发送方式 | 有历史的旧 Claude 会话仍走 print + resume |

Kiro 旧打印路径：本机登录或 `KIRO_API_KEY` 可用时走 HTTP 多轮（会话 id 带 `kiro-http:` 前缀）；已有 HTTP 会话失败时直接报错并保留会话，不回退成新的命令行会话。企业 IdC / `profileArn` 仍是提案剩余边界。

## 允许 / 拒绝 / 一直允许

只覆盖 Codex / Grok / Kiro 的持续通道。没有真实确认通道的 Agent 不画可点的假按钮。

1. **记住卡片上的选项。** 待处理请求的选项写入 SQLite；快照或进程重开后，未回复的卡片仍显示同一组按钮（包括这次能不能点「一直允许」）。
2. **点了「一直允许」后还问不问。** 三家都由 AgentHub 在**当前这场对话**里记住，不写数据库，新对话再问。
   - 范围按卡片分开：文件修改卡记住后，同一对话里后续文件修改（含工作目录外的路径）不再出卡；命令和其他工具只记住同一条（同标题、同内容），换一条仍出卡；文件卡的一直允许不放行命令。
   - Codex：卡片上的「一直允许」由 AgentHub 补上，发给对方的是 `acceptForSession`。进程常驻时对方也记得；关掉「一直允许」会重开 Codex，对方自己记住的批准一起清掉。
   - Grok / Kiro：只有这次 `session/request_permission` 自带 `allow_always` 类选项（Kiro 常见 `allow_always_tool` / `allow_always_tool_args`）才显示「一直允许」，点了把对方给的选项原样回传。没有该选项时仍出卡片，不补假按钮。
   - Grok 写工作目录外的文件（本机 `fs/write_text_file`）时，Chat 先出「修改文件」卡片（允许 / 一直允许 / 拒绝）再写。
3. 点过之后对话里写「本会话已一直允许」，并注明它不是会话设置里的自动批准 / 完全访问权限。会话设置里可以关掉记住，不能从设置里假装打开。

Kiro 会话设置里的「帮我批准 / 完全访问权限」是另一回事：下次启动时带 `--trust-all-tools`。生成中不能改；改完下一轮重新拉起进程。Kiro 重启后接不回原会话，AgentHub 会把之前的对话带进这次提问；Grok 换设置时会接回原会话。

## 本机对接与这次会话

持续通道按 Agent 写死（`chat_runtime/store.rs` 的 `is_runtime_chat_agent`），不是用户可改的总表。界面分两层：

- **Agents 详情「新对话」**：这个 Agent 开新对话时走哪种方式。隐藏的 Agent 不会用来开新对话。
- **Chat 会话设置「这次对话」**：这场对话实际在走哪种。旧会话仍走原发送方式时，两处会不一致。顶栏不写接法。

斜杠菜单里，AgentHub 自己的动作当场执行；对方声明的斜杠命令选中后作为一轮正常发出（无必填参数直接发，有必填参数先插入 `/名字 `），对方没声明就不画。目录和模型等选项放在会话 Options 里，不进过程时间线。

## 数据流

**持续通道**：页面 → ChatPort runtime 操作 → Tauri 阻塞命令 → `ChatRuntime`（每个会话一个串行 worker）→ 对方进程。后台把消息、事件和终态写进 SQLite；页面读取带 sequence、待处理请求、`currentMessage` 和 gap 的快照。正文取同一次读取里的完整 `currentMessage`，不靠字符串相似度猜增量。活动会话约每 80ms 读一次快照（`RUNTIME_SNAPSHOT_POLL_ACTIVE_MS`）；首字前显示「正在想」，出正文后显示「正在写」，不把整段回复拆开假装逐字打出。关掉页面不影响后台运行；重开时用持久化的原生会话 id 接回。

**原发送方式**：

```text
Chat 输入框
  → src/lib/api/chat.ts（生产 façade）
  → backend.chat.send
  → Tauri command + ipc::Channel<ChatEvent>（不是 SSE）
  → ChatService::send → RunService::run_each
  → adapter build_run_spec(ProcessMode::Auto)
  → StreamingProcessRunner → stream_parse/*（能结构化就结构化，否则按文本）
  → RunEvent → ChatEvent → 前端 reducer（正文 + 过程摘要）
```

这条路径的过程数据主要在内存里，最终正文和消息入库，刷新后不保证能回放过程。浏览器 `dev:mock` 与 Vitest 用 `src/dev/mocks/chat.ts` 提供同一 port 契约的可控事件；`src/lib/api/chat.ts` 不是 mock。

## 统一事件语义

前端处理的稳定事件：turn/agent 排队与开始；Agent 启动命令；stdout 文本（进正文，也进过程）；stderr/诊断（只进过程）；结构化过程步骤（思考、工具、状态等，取决于解析器）；Agent 完成、取消、失败与整体结束/出错。步骤种类见 [Chat 过程事件](chat-process-events.md)。

- Claude、Codex、Kimi、Grok、Pi、Kiro 的原发送方式可走 `ProcessMode::Auto` 结构化解析；WorkBuddy、ZCode 没有结构化解析器，按文本展示；DSH 的 `StructuredStream` 仍是 Planned。
- Kiro 结构化输出要 `--agent-engine v2`（v1 拒绝 stream-json）。
- ZCode 对话需要 PATH 上有 `zcode` 命令；只装桌面端不算。
- Cursor Agent 默认软隐藏，结构化输出与登录写入修好前不在 Chat 开放。
- 解析失败降级为原始/文本事件，不因一行 JSON 无法识别而丢掉整次对话；CLI 不支持某个 flag 时不得静默换成另一种语义重试。

持续通道另有：有限的持久化事件（截断用 gap 表示，不承诺无限历史）、真实确认回复、同机恢复。用量只在本轮结束后用小字写输入 / 输出，只画协议给的数字，不估算费用。图片附件只在持续通道露出；普通文件与 `@` 未接。各项真窗验收情况见 [STATUS](../STATUS.md)。

## Codex 外部安装

Chat 不调用 VS Code 扩展或 Codex 桌面 App 的界面，而是启动检测到的 Codex 可执行文件：新空会话用 `codex app-server`，原发送方式用 `codex exec --skip-git-repo-check [resume <id>] [--json] …`。检测到安装不等于协议和账号可用；失败明确返回，不静默换发送方式。

| 安装来源 | 怎样识别（`adapters/codex_copies.rs`） |
| --- | --- |
| npm 全局 | PATH、`npm prefix -g` 或常见目录 |
| VS Code / Cursor 插件 | 扫描 `openai.chatgpt-*` 扩展目录 |
| Windows 桌面 App | `%LOCALAPPDATA%\Programs\OpenAI\Codex`、`%LOCALAPPDATA%\OpenAI\Codex\bin`、`WindowsApps\OpenAI.Codex_*` |
| macOS 桌面 App | `/Applications/Codex.app` |

任一来源的前置条件相同：`~/.codex/auth.json` 有效、已选工作目录、Agent 未隐藏。IDE / 桌面副本在 Agents 页标为「在 IDE/桌面 App 内更新」，不影响 Chat 使用。

## Agent 能力边界

能力等级是调用门禁（见 [能力参考](../reference/capabilities.md)）。`StructuredStream` 只决定能否启用结构化过程，不代表该 Agent 的所有 Chat 特性都已实现。未知或不支持必须显示明确状态，不能借用别家的解析器或 mock。

## 相关页面

- [Chat 体验标杆](../ui/chat-experience-bar.md)
- [Chat 统一体验](../proposals/chat-unified-experience.md)（提案）
- [Chat 宿主加深与多 Agent 兼容](../archive/chat-host-depth.md)（提案）
- [Core and runtime](../architecture/core-runtime.md)
- [Frontend and backend boundary](../architecture/frontend-backend.md)
