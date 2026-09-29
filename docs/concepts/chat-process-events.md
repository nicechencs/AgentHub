---
title: Chat 过程事件
type: concept
status: current
owner: maintainers
audience: chat and core contributors
source-of-truth: ProcessStep, ChatEvent, RuntimeSnapshot
updated: 2026-09-29
---

# Chat 过程事件

对话页的过程面板显示的是已经整理好的步骤，不是对方原始协议全文。本页固定**最小步骤种类**和**不能画成过程的内容**。

## 怎样送到页面

- **持续通道**：活动会话约每 80ms 读一次 `RuntimeSnapshot`，里面有正文 `currentMessage`、过程步骤、待确认请求，以及可选的 `catalogEpoch`、`plan`、`hostTerminals`（宿主命令卡片）。带序号的事件落在 SQLite 表 `chat_runtime_events`。
- **原发送方式**：过程只在内存里，刷新后不保证回放。

## 步骤种类（`ProcessStep`）

类型定义在 `crates/agenthub-core/src/models/chat.rs`（`ProcessStep` / `ChatEvent`）。

| 种类 | 用户看到什么 | 打码要求 |
| --- | --- | --- |
| 状态 `Status` | 运行详情里的阶段，不冒充工具 | 不写 Key、完整 token |
| 思考 `Thinking` | 右侧栏思考正文 | 只用对方给出的文本，不从屏幕猜 |
| 工具 `Tool` | 主列「正在读取 / 正在修改 / 正在执行」；细节里是名称和折叠的 JSON | 输入/结果里的 Key 只留末四位或整段去掉；路径可保留 |
| 文本 / 原始行 `Text` / `Raw` | 过程或降级展示 | 坏行写成「有一行输出没法展示」，不把 Key 打进气泡 |
| 错误 `Error` | 失败信息 | 同上 |
| 用量 `Usage` | 本轮结束后小字写输入 / 输出；`scope=context` 才表示上下文窗口用量 | 只画协议里的数字，不估算费用 |

允许 / 拒绝按钮在待确认卡片上，过程面板只写「等待允许或拒绝」。面板首行“你说了什么”来自这一轮已有的用户消息，不另开步骤种类。

## 不能当过程行

以下内容进会话 Options 或专用条，**不能**推进过程时间线冒充“对方做了一步”：

- 斜杠命令目录（ACP `available_commands_update`、Kiro `_kiro.dev/commands/available`、`nativeCommands`）；
- ACP `config_option_update`（模型 / 思考目录）；
- `catalogEpoch`（只用来提示页面重拉 Options）；
- `plan`：进当前轮的计划条，换轮丢掉。来源是 Grok / Kiro 的 ACP `sessionUpdate: plan`，以及新空 Claude 会话的 TodoWrite / TaskCreate / TaskUpdate / TaskList；
- 宿主 `terminal/*`：进命令卡片，不是对话终端。

## 边界

- 新步骤挂在现有过程面板上，不新开第二套轨迹 DTO，也不换掉约 80ms 的快照读取，不做公网推送。
- 调试导出时读已有快照或 `chat_runtime_events`。

当前面板行为见 [STATUS](../STATUS.md) 与 [Chat 与 Agent 运行](chat-and-agents.md)。
