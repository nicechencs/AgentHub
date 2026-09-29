---
title: Chat 宿主加深与多 Agent 兼容
type: archive
status: archived
owner: maintainers
audience: product owners and implementation agents
updated: 2026-09-29
---

# Chat 宿主加深与多 Agent 兼容

> 状态：proposed。切片 A–E、G–I 已在 `dev`，F 取消。只剩真窗验收和白名单收口。现行事实只写在 [STATUS](../STATUS.md)。

在共用对话页和各家机器通道上加深思考、命令和跨 Agent 兼容，**不用伪终端当对话底座**。对照 [AionUi](https://github.com/iOfficeAI/AionUi)：吸收「握手定能力、统一事件、协议斜杠目录」，不搬内置引擎、常驻后台进程或多人协作。本页不替换 [Chat 统一体验](../proposals/chat-unified-experience.md)。

## 当前基线

| 点 | 现状 | 证据 |
| --- | --- | --- |
| 通道 | 新空 Codex 走 app-server 常驻进程；Grok / Kiro 走 ACP；Claude 走 stream-json；其余一次性 | [STATUS](../STATUS.md) Chat 表 |
| Options | 带 `transport`、`nativeCommands`、`sessionReady`；握手可改图片；ACP config 可刷模型/思考 | `src/lib/backend/contracts/chat-runtime.ts` |
| 快照 | 只放回合态；另有廉价 `catalogEpoch`，变化时重拉 Options；命令列表不进快照 | 同上 |
| ACP 事件 | 命令目录进 Options；思考可标完成；工具分类；`context_usage` 进用量小字；`plan` 进计划条 | `crates/agenthub-core/src/utils/stream_parse/acp.rs`、`grok.rs` |
| 宿主终端 | 握手声明 `terminal`，每条 `terminal/*` 命令一张卡片，可停这一条；管道拉起，不是对话 TTY | `services/chat_runtime/ops.rs` |
| `/` 菜单 | 对方声明的斜杠命令当作一轮正常发出；「启动命令行」（DeepSeek 为「打开网页会话」）在外部打开 | `src/pages/chat/chat-actions.ts` |
| enable 白名单 | 仍按 Agent 名：Codex / Grok / Kiro / Claude，未拆 | `chat_runtime/store.rs` 的 `is_runtime_chat_agent` |

## 切片状态

| 刀 | 状态 |
| --- | --- |
| A Options 能力位 | 已合入 |
| B ACP 事件与命令目录 | 已合入 |
| C `/` 接协议目录 | 已合入 |
| D 目录变更刷新 `/` | 已合入 |
| E `config_options` 刷模型/思考 | 已合入 |
| F ACP 结构化提问 | 取消：Grok/Kiro 未见提问请求，仓库无夹具；不声明 `elicitation` |
| G 用量小字 / 计划条 | 已合入 |
| H 宿主终端卡片 | 已合入 |
| I 打开对方命令行 | 已合入 |

## 剩余边界

- 真窗验收：Grok 声明斜杠命令后选中发送。Kiro 列出已在 Linux 真窗验过，选中发送和其他 Agent 未验。没有真窗不写「已验收对方命令」。
- 拆 enable 白名单：等统一体验不再需要按名字判断时收口，不单开一刀。
- F：若 Grok/Kiro 给出真实提问帧，再单开一刀。

## 原则

1. 能力写在 `RuntimeOptions`，不塞进 80ms 快照。
2. 外接命令行优先 ACP；Codex 继续 app-server，Claude 继续 stream-json，直到有经验证的对等控制口。
3. 思考以实时事件为主，会话记录为辅，不从屏幕猜正文。
4. 不支持就隐藏，不画假控件。失败后不在持续通道和一次性发送之间自动切换。
5. 新 Agent 接入顺序：ACP → 对方 JSON 控制口 → 持续 stream-json → 一次性 JSON 行 → 会话记录回填 → 只能发一轮或打开对方命令行。

## 非目标

- 默认内嵌终端，或把各家 TUI 当对话页。
- 未出现在协议目录里的斜杠项（含 Kiro 终端 `/model`、`/agent`）。
- 跨 Agent 搬运原生会话；自带大模型引擎；为对话拆 sidecar。
- 把 Codex 改成 `codex --acp` 只为统一协议。
- 凭据落盘加密、国产官方登录转 API。

## 本页不负责

| 主题 | 去处 |
| --- | --- |
| Claude 允许/拒绝 | [统一体验](../proposals/chat-unified-experience.md)、[Claude B3](../archive/chat-claude-b3.md) |
| Pi / Kimi / ZCode 等持续通道 | [统一体验](../proposals/chat-unified-experience.md) |
| 本机路由进程拆出 | [Local Route Sidecar](../proposals/adapter-sidecar.md) |

## 相关页面

- [Chat 与 Agent](../concepts/chat-and-agents.md)、[Chat 体验标杆](../ui/chat-experience-bar.md)、[能力参考](../reference/capabilities.md)
- 历史批次：[Codex B1](../archive/chat-codex-b1.md)、[Codex B2](../archive/chat-codex-b2.md)、[Claude B3](../archive/chat-claude-b3.md)
