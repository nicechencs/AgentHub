---
title: Chat 过程事件
type: concept
status: current
owner: maintainers
audience: chat and core contributors
source-of-truth: ProcessStep, ChatEvent, RuntimeSnapshot
updated: 2026-09-17
---

# Chat 过程事件

对话页过程面板读的是已经规范化的步骤，不是对方原始协议全文。本页固定**最小枚举**和**不要画进过程**的项。不引入公网订阅总线，也不一次换掉约 80ms 快照。

## 现行投递

持续聊天：活动会话约 80ms 读 `RuntimeSnapshot`（正文 `currentMessage`、过程步骤、待确认、可选 `catalogEpoch` / `plan` / 宿主命令卡片）。旧发送路径是内存过程视图，刷新后不保证回放。有序号的运行时事件可落在 `chat_runtime_events`。

## 最小种类（`ProcessStep`）

| 种类 | 用户能看见 | 脱敏 |
| --- | --- | --- |
| 状态 | 运行详情里的阶段，不冒充工具 | 不要写密钥、完整 token |
| 思考 | 右侧栏思考正文 | 按对方已给出的文本；不要从屏幕猜 |
| 工具 | 主列「正在读取 / 正在修改 / 正在执行」；细节里名称与折叠 JSON | 输入/结果里的密钥只留末四位或整段去掉；路径可保留 |
| 文本 / 原始行 | 过程或降级展示 | 坏行写成「有一行输出没法展示」，不要把密钥打进气泡 |
| 错误 | 失败信息 | 同上 |
| 用量 | 本轮结束后小字输入 / 输出；过程面板末行同样只画协议数字；`scope=context` 才表示窗口用量 | 只画协议里的数字，不估算费用 |

对应 wire：`ProcessStep` / `ChatEvent`（`crates/agenthub-core/src/models/chat.rs`）。前端摘要见过程面板。允许 / 拒绝按钮在待确认卡片上；过程面板只写「等待允许或拒绝」，不造假按钮。你说了什么挂在同一面板首行，来自这一轮已有的用户消息，不另开事件种类。

## 不要当过程行

下列进 Options 或专用条，**禁止**推进过程时间线冒充「对方做了一步」：

- 斜杠命令目录（`available_commands` / Kiro `_kiro.dev/commands/available` / `nativeCommands`）
- ACP `config_option_update`（模型 / 思考目录）
- 世代号 `catalogEpoch`（只用来重拉 Options）

`plan` 走当前轮计划条，换轮丢掉。宿主 `terminal/*` 走命令卡片，不是对话 TTY。

## 本波边界

- 最小事件挂现有过程面板：你说了什么、工具、等待确认、用量、错误。不新开第二套 trajectory DTO，也不换掉约 80ms 快照。
- 导出调试：需要时读已有快照或 `chat_runtime_events`，不要先做公网推送。

面板现行事实见 [STATUS](../STATUS.md) 与 [Chat 与 Agent](chat-and-agents.md)。
