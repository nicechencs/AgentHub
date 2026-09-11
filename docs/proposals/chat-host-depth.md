---
title: Chat 宿主加深与多 Agent 兼容
type: proposal
status: proposed
owner: maintainers
audience: product owners and implementation agents
updated: 2026-09-11
---

# Chat 宿主加深与多 Agent 兼容

在现有对话页和各家机器通道上加深思考、命令和跨 Agent 兼容；**不以伪终端当对话底座**。对照 [AionUi](https://github.com/iOfficeAI/AionUi) 的 GUI 宿主，吸收握手定能力、统一事件、协议斜杠目录；不搬内置引擎、独立后台进程或多人协作。

本页不替换 [Chat 统一体验](chat-unified-experience.md)，只补「底座已定之后如何加深」。未合入、未验收前不得把能力标成 Full，也不得改 [STATUS](../STATUS.md) 的现行事实。切片进度见下文。

## 问题

对话页要对标 Claude Code 桌面、Cursor Chat、Codex app：同一套气泡、过程、确认和输入区，接多家 Agent。加深时要的是：

1. 思考过程有正文，而不只是「正在想」。
2. 能触发与对话相关的命令：对方跑的工具，以及对方声明的 `/compact` 一类动作。

用伪终端嵌各家交互界面，表面覆盖会变多，语义兼容会变差。AionUi 用统一对话页 + ACP 接很多家命令行，伪终端只给「对方要跑的那条命令」。那是对照宿主，不是第二套产品。

## 当前基线

2026-09-11 对照源码。细节以 [STATUS](../STATUS.md) 为准。

| 点 | 现行 |
| --- | --- |
| 产品形态 | 共用 GUI 对话页。[统一体验](chat-unified-experience.md) 已把默认内嵌终端、全量原生命令菜单列为范围外 |
| 持续通道 | 新空 Codex：`app-server --stdio`。新空 Grok / Kiro：ACP。新空 Claude：stream-json。其余与旧会话：一次性发送 |
| 回合快照 `RuntimeSnapshot` | `enabled`、phase、runId、事件、待确认、`currentMessage`。约 80ms 一轮。无能力表、无斜杠目录、无 transport |
| 会话目录 `RuntimeOptions` | 模型、技能、`imageInput`、`steer`。图片对白名单 **写死 true**；steer 仅 Codex 为 true |
| 谁能开持续聊天 | 白名单 Codex / Grok / Kiro / Claude 写入 `enabled`。发送只看 `enabled`。页面绑快照、拉 options、画确认卡片仍看名字 |
| Grok / Kiro ACP | 思考 / 正文 / 工具 / 部分用量已进过程。确认走 `session/request_permission`，已有卡片。`available_commands`、`config_option_update`、`context_usage` **故意丢弃**（测试断言为空，避免进过程时间线）。握手 **不声明** `terminal` |
| `/` 菜单 | Hub：新建对话、复制最近回复。按能力加：换模型、思考强度、「用于本次」技能。空会话芯片才是样例提问。**无**对方声明的斜杠命令 |
| Kiro `/` | 持续聊天已与 Grok 同一套门闩。不要再写成「Kiro 关掉 `/`」。终端补全仍不得宣称 |
| 进程 | 一次性发送 stdin 关闭。sidecar 是 [另一份提案](adapter-sidecar.md)，不从本页派生 |

`StructuredStream` 只表示能否解析过程，不等于思考正文、确认、斜杠命令都已接上。Grok 技能库可用，对话里「用于本次」仍不支持，不画假按钮。

## 候选结论

1. 对话底座保持机器通道 + 统一事件，不换成伪终端宿主。
2. 能力写在 `RuntimeOptions`，**不塞进 80ms 快照**。页面 chrome 读 Options；enable 白名单可暂时保留。
3. 外接命令行默认加深 ACP（Grok / Kiro 已在用）。Codex 继续 app-server，Claude 继续 stream-json，直到该家有经验证的对等控制口。
4. 思考以实时事件为主，会话记录为辅；不从屏幕猜正文。
5. `/` 分来源：Hub 动作立刻执行；对方声明的斜杠命令插入 `/名字 ` 再当普通一轮发出（不是 Hub RPC，也不是往终端打字）。
6. 宿主终端卡片（ACP `terminal/*`）后置；第一批切片继续不声明 `terminal`。
7. 先深已接线的四家，再按梯子扩家。

未落地。未授权不得当现行。

## 非目标

与统一体验范围外一致，并补充：

- 默认内嵌终端，或把各家 TUI 当对话页
- 未出现在协议目录里的斜杠项（含 Kiro 终端 `/model`、`/agent`）
- 跨 Agent 搬运原生会话
- 自带一份可聊的大模型引擎
- 为对话单独拆常驻 sidecar
- 把 Codex 改成 `codex --acp` 只为「大家都走 ACP」
- 凭据落盘加密、国产官方登录转 API
- 把本页或 AionUi 对照写成现行契约

## 对照 AionUi（只吸收同向部分）

AionUi 桌面只画界面；`aioncore` 用 ACP JSON-RPC（stdio）拉起本机 CLI。对话是 JSON 行，不是屏幕。GUI 吃 HTTP 快照 + 统一流，不直接当 JSON-RPC 对端。

| 吸收 | 不搬 |
| --- | --- |
| 页面只认统一事件 | 自带大模型引擎 |
| 握手缺省 false；缺项隐藏 | 常驻 `aioncore` 式进程 |
| 斜杠目录不进气泡；对方命令插入 `/名字 ` | 多人协作 / 领袖分派 |
| 确认和提问两条回执 | 嵌 TUI |
| 伪终端只给对方要跑的命令 | 为接 20 家而换掉已验收的 Codex app-server |

## 兼容原则

同一套用户操作（发送、看思考、看对方跑了什么、允许/拒绝、停止、换模型）。每一项：完整 / 降级 / 不支持 / 还没接。不支持就隐藏。

不要求每家表现一模一样，也不为接更多家画假控件。失败后不得在持续通道和一次性发送之间自动切换。

新 Agent 梯子：ACP → 对方 JSON 控制口（app-server）→ 持续 stream-json → 一次性 JSON 行 → 会话记录回填 → 只能发一轮或「打开对方命令行」。连通性检查是 spawn + 协议握手，不是打开交互界面。

## 数据放哪

```text
页面 chrome ──读──► RuntimeOptions（目录：模型、能力、对方斜杠命令）
页面回合 ──读──► RuntimeSnapshot（约 80ms：正文、过程、待确认、phase）
对方 ACP ──► 对接层 ──► 过程事件 | Options 目录 | pendingRequests
```

| 仓 | 现在 | 加深后 |
| --- | --- | --- |
| `RuntimeSnapshot` | 回合态 | 仍只放回合态。可选加 `transport`（`acp` / `app-server` / `stream-json` / `legacy`），**不加**命令列表 |
| `RuntimeOptions` | 模型、技能、写死的 image/steer | 能力位 + `nativeCommands`。握手和 `available_commands` 写这里 |
| 过程事件 | 思考 / 工具 / 用量 | 斜杠目录、config 更新 **不要**当过程行 |
| `pendingRequests` | 命令 / 文件 / 提问 | 保持；提问回执不走确认口 |

白名单暂时只当「准不准 enable 持续聊天」。图片、补充、`/` 里对方命令、确认卡片改看 Options。

### Options 字段草案（实施时可改名，语义不要混）

在现有 `RuntimeOptions` 上增量，camelCase 与现行契约一致：

| 字段 | 类型（草案） | 含义 |
| --- | --- | --- |
| `transport` | `acp` / `app-server` / `stream-json` / `legacy` | 这份会话实际在走的通道 |
| `imageInput` | boolean | 改为握手 `promptCapabilities.image`；省略 = false。切片 A 可先保持写死值，B 再接握手 |
| `steer` | boolean | 仅对方真有生成中补充口时为 true（现状：只有 Codex） |
| `nativeCommands` | `{ name, description, hint? }[]` | 对方声明的斜杠命令，无前导 `/`。未就绪或未声明 = `[]` |
| `sessionReady` | boolean | 可以查对方目录 / 发斜杠插入。连接中为 false |

不在 Options 里放 MCP 传输、fork、loadSession：那些是 Agent 级握手，不是对话回合目录。需要时另开切片，挂检测/Agents，不挂 80ms 快照。

## 现行缺口（Grok / Kiro ACP）

| 对方给的 | 现在 | 本方案 |
| --- | --- | --- |
| `agent_thought_chunk` | 思考正文，`done` 一直 false | 正文开始或本轮结束时标完成 |
| `tool_call` / update | 过程 Tool，kind 用原文字 | 收成读取 / 修改 / 执行 |
| `agent_message_chunk` | 拼进 `currentMessage` | 保持 |
| `session/request_permission` | 待确认卡片 | 保持；选项 id 原样回传 |
| 结构化提问 | ACP 未接线（提问口偏 Codex） | 有协议再接；回执不走确认 |
| `available_commands_update` | **丢弃** | 写入 `nativeCommands`，刷新 `/`，不进气泡 |
| `config_option_update` | 丢弃 | 后置：刷新模型/模式 |
| 本轮 usage / `context_usage` | 部分 usage | 回复下小字用已有 usage；窗口用量后置 |
| `plan` | 一行 Status | 后置：当前轮计划条，不进气泡 |
| `terminal/*` | 握手不声明 | 后置；A–C **继续不声明** |

页面上仍按名字分叉、且本方案要收口的：`isRuntimeChatAgent`（绑快照 / 拉 options / 确认卡片）、`isQueueFollowUpAgent`（排队 vs 补充，应改看 `steer`）、空输入区「不能中途补充」提示、Codex 工具栏不画技能。

## 伪终端

| 含义 | 态度 |
| --- | --- |
| 对话就是各家 TUI | 不采用 |
| JSON 通道挂在伪终端上骗过 isatty | 仅当没有 TTY 就不开机器通道 |
| 对方跑的命令需要终端 | 后置宿主卡片，不是对话模型 |

A–C 不引入伪终端，不声明 `clientCapabilities.terminal`。

## 建议切片

文档先行。未合入前不得把本页标成 current。切片 A / B / C 在分支 `feat/chat-host-depth-options` 实施。

依赖：`C ← B`（没有目录就没有对方命令可列）。`A` 可单独先做。不要先扩 80ms 快照。不要顺手拆 enable 白名单。

```mermaid
flowchart LR
  A[A Options 能力位]
  B[B ACP 事件与命令目录]
  C["C / 接上协议目录"]
  A --> C
  B --> C
```

### 切片 A — Options 带能力

**做：** 契约加上 `transport`、`nativeCommands: []`、`sessionReady`。`imageInput` / `steer` 语义写进注释：A 可保持现行写死值。页面：确认卡片、图片按钮、是否查询对方命令改读 Options；发送仍看快照 `enabled`。

**不做：** 改 ACP 解析；拆 `is_runtime_chat_agent`；把命令列表放进 Snapshot；改 mock 以外的产品文案。

**文件：** `src/lib/backend/contracts/chat-runtime.ts`、`crates/agenthub-core/src/services/chat_runtime/types.rs`、`mod.rs` 的 `options_with`、`src/dev/mocks/chat.ts`、`src/pages/chat/chat-runtime-model.ts`、`use-chat-runtime-ops.ts`、`use-chat-page.ts`（读字段，不画对方命令）。

**测：** `chat_runtime_contract.rs`、`chat_runtime/tests.rs`、`chat-runtime-ops-model.test.ts`、`chat-runtime-model.test.ts`。

**验收：** 非持续聊天 Options 仍是 inactive（命令空、image/steer false）。白名单会话 Options 带 `transport` 与空 `nativeCommands`。页面不因名字把 Pi 画成持续聊天。mock 与契约字段一致。

### 切片 B — Grok / Kiro 事件对齐

**做：** `available_commands` 产出可写入 Options 的目录，不再 `vec![]`。**将改掉**「必须为空」的现有断言。思考在正文开始或 turn 结束时 `done: true`。工具 kind 映射为读取 / 修改 / 执行。确认路径回归，不改协议。`imageInput` 若握手已有 `promptCapabilities.image` 则改读握手。

**不做：** `/` 菜单接线（那是 C）；声明 `terminal`；接 ACP 提问口；把目录当 ProcessStep 推进右侧栏。

**文件：** `crates/agenthub-core/src/utils/stream_parse/acp.rs`、`grok.rs`、`chat_runtime/mod.rs`（`grok_session_update` / `options_with`）、必要的 store 缓存（会话级，不进 snapshot events）。

**测：** `stream_parse/grok/tests.rs`、`stream_parse/tests.rs`、`actor_tests.rs`。固定 JSON 帧，不跑真 TUI。

**验收：** 一条带 `available_commands_update` 的夹具帧之后，Options.`nativeCommands` 非空，过程时间线仍无命令列表行。思考在回复开始后可标完成。允许/拒绝卡片与现在一致。

### 切片 C — `/` 接上协议目录

**做：** 对方命令作为独立来源进入现有 `extraActions`。选中插入 `/名字 `（可带 hint 空格），**不**立刻 `runtimeStart`。`sessionReady === false` 或列表空则不画对方项。Hub 的新建 / 复制 / 换模型 / 技能保持立刻执行。

**不做：** 把插入当成「已交给 Agent」；在连接中查询对方；把 Kiro 终端选择器写进菜单；改空会话芯片。

**文件：** `src/pages/chat/chat-actions.ts`、`use-chat-page.ts` 的 `runtimeCommandActions`、`ChatActionMenu.tsx` / `ChatComposer.tsx`（插入草稿）。

**测：** `chat-actions.test.ts`；mock Options 带一条命令时 `/` 能搜到；选中后草稿为 `/name `。

**验收：** 无声明则 `/` 与现在相同。有声明则出现该项，发送仍是用户按发送。中文输入法组字规则不变。

### 之后（单独授权）

- Claude 允许/拒绝接到真实确认通道；确认和提问分口
- `config_options` 刷新模型/模式菜单
- 宿主终端：先改握手声明，再画卡片；须验收 Grok 会不会误调
- Pi / Kimi 能否升持续通道；升不了保持一轮 + 过程
- 「打开对方命令行」仅作无机器通道的逃生口

WorkBuddy / ZCode / DeepSeek 等无可靠过程流的保持受限。

## 门槛

- 每项有对方协议或会话记录证据；无证据不画。
- 契约测试 + 对应 Vitest / Cargo filter；mock 不能当真实 Agent 验收。
- 跨层按 [AGENTS.md](../../AGENTS.md) 走实现与独立审查。
- 改 `available_commands` 语义时，必须同时改断言为空的测试，并在切片说明里写明。
- 不顺手拆白名单、不上 PTY、不把提案标成 current。

## 相关页面

- 现行：[Chat 与 Agent](../concepts/chat-and-agents.md)、[当前实现状态](../STATUS.md)、[Chat 体验标杆](../ui/chat-experience-bar.md)、[能力参考](../reference/capabilities.md)
- 共用界面与各家对接：[Chat 统一体验](chat-unified-experience.md)
- 进程边界（不从本页派生）：[adapter sidecar](adapter-sidecar.md)
- 终端能力不得宣称：[接入 Kiro](agent-kiro.md)
- 对照宿主：[AionUi](https://github.com/iOfficeAI/AionUi)
- 历史批次：[Codex B1](../archive/chat-codex-b1.md)、[Codex B2](../archive/chat-codex-b2.md)、[Claude B3](../archive/chat-claude-b3.md)
