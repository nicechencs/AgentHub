---
title: Chat 支持深度矩阵
description: 每家 Agent 的探测/启动与 Chat 宿主深度对照；与 Capability 能力矩阵分工。
type: reference
status: current
owner: maintainers
updated: 2026-09-29
---

# Chat 支持深度矩阵

Agents 目录里有这家，不等于 Chat 做得一样深。本页只标 Chat 深度；安装、配置、技能、MCP 等看 [能力参考](capabilities.md)。

代码来源（`crates/agenthub-core/src/services/chat_runtime/`）：

- 通道枚举 `RuntimeChannel`：`types.rs`（`acp` / `app-server` / `stream-json` / `legacy`）
- 持续通道名单 `is_runtime_chat_agent`、ACP 子集 `is_acp_runtime_agent`：`store.rs`
- Agent → 通道映射 `runtime_channel`：`mod.rs`

## 档位

| 档 | 含义 | 进入条件 |
|---|---|---|
| **D0 可探测/可启动** | 能探测、安装或打开对方程序；CLI `run` / `build_run_spec` 可用 | adapter 与 integrations 已注册 |
| **D1 一次性 Chat** | 原发送方式：每轮起一次命令行，过程多在内存 | 不在持续通道名单里，或是有历史的旧会话 |
| **D2 持续通道** | 同一进程跑多轮；`RuntimeChannel` 不是 `legacy` | 在 `is_runtime_chat_agent` 名单里且是**新空**会话 |
| **D3 宿主加深** | 允许/拒绝卡、对方斜杠命令、命令卡片、「用于本次」技能、模型/思考 Options 等 | 对方协议真有该能力；没声明就不画，不造假按钮 |

深度可以逐步加深：新 Agent 可以先 D0 + D1，有协议证据再升到 D2 / D3。不要为了“目录好看”把 Planned 写成 Full，也不要用本表代替 `capability()`。

## 现行矩阵

| Agent | Chat 档位 | 新空会话通道 | D3 内容（新空会话） | 备注 |
|---|---|---|---|---|
| Codex | **D2** 新空 | `app-server` | 允许/拒绝/一直允许、模型/思考、图片、「用于本次」、斜杠动态菜单；计划模式未做；Computer Use 无产品路径 | 一场对话常驻一个进程；最深 |
| Claude | **D2** 新空；有历史 **D1**（print + resume） | `stream-json` | 模型/思考、图片；TodoWrite / Task 进计划条；**无**允许/拒绝卡（`dontAsk` / 危险模式 `bypassPermissions`）；无「用于本次」 | |
| Grok | **D2** 新空 | `acp` | 允许/拒绝（按对方选项）、模型/思考、图片；**无**「用于本次」；生成中不能补充（可排队） | ACP 族 |
| Kiro | **D2** 新空；旧会话 **D1** | `acp` | 允许/拒绝、停止；模型/思考/权限生成中不能改，改完下一轮重新拉起进程；生成中不能补充（可排队） | ACP 族；旧会话不切 ACP |
| Kimi | **D1** | `legacy` | — | |
| Pi | **D1** | `legacy` | — | |
| WorkBuddy | **D1** | `legacy` | — | |
| ZCode | **D1** | `legacy` | — | |
| DSH | **D1** | `legacy` | — | |
| Cursor | **D1**（默认软隐藏） | `legacy` | — | 软隐藏不等于删除适配器；见 [身份兼容族](../concepts/agent-identity-families.md) |

十家都是 D0。行为细节（斜杠菜单、确认卡片等）以 [Chat 与 Agent 运行](../concepts/chat-and-agents.md) 和 [STATUS](../STATUS.md) 为准，本表只标档位。

## 该看哪一页

| 问题 | 看哪 |
|---|---|
| 能否安装、探测、写配置、技能、MCP、用量？ | [能力参考](capabilities.md) |
| Chat 是否持续、有没有确认卡？ | 本页 + [STATUS](../STATUS.md) |
| 本机路由能否接到这家？ | [Route 兼容性](route-compatibility.md) |
| 启动参数在哪？ | [Agent 启动与探测清单](agent-launch-inventory.md) |
| 模型等选项从哪来？ | [Chat 会话选项目录](chat-session-options.md) |

## 新增 Agent 时

1. 先如实填 `capability()` 的 14 个键。
2. 在本表加一行：至少 D0；没接持续通道就写 D1 + `legacy`。
3. 协议和真窗证据都齐了，才把它加进 `is_runtime_chat_agent` 并升到 D2 / D3。
4. 失败后不能在持续通道与一次性发送之间**静默**切换。

相关：[添加 Agent](../guides/adding-an-agent.md)、[多 Agent 扩家硬约束](../guides/multi-agent-support-rules.md)。
