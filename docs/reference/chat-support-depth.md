---
title: Chat 支持深度矩阵
description: 每家 Agent 的探测/启动与 Chat 宿主深度对照；与 Capability 能力矩阵分工。
type: reference
status: current
owner: maintainers
updated: 2026-09-20
---

# Chat 支持深度矩阵

本页回答：**Agents 目录里有这家，不等于 Chat 一样深。**  
装/配/技能/MCP 等仍看 [能力矩阵](capabilities.md)（`Capability::*` 穷尽 14 键）。本页只标 **Chat / 持续宿主** 深度，供贡献者与产品扫读。

代码真源：

- 通道枚举 `RuntimeChannel`：`crates/agenthub-core/src/services/chat_runtime/types.rs`
- 持续 Chat 白名单 `is_runtime_chat_agent`、ACP 子集 `is_acp_runtime_agent`：`…/chat_runtime/store.rs`
- 通道映射 `runtime_channel`：`…/chat_runtime/mod.rs`

学习对照（不照搬）：Orca L0 终端可启动 → L3 结构化；见 Context 分析文档 A1。AgentHub 主路径是结构化宿主，不是 PTY 标题爬虫。

## 档位说明

| 档 | 含义 | 进入条件（证据） |
|---|---|---|
| **D0 可探测/可启动** | detect + 安装/打开对方程序；CLI `run` / headless `build_run_spec` | adapter + integrations 已注册 |
| **D1 一次性 Chat** | 对话走 Channel 事件流 / print·headless；过程多为内存 | 未进持续 runtime 白名单，或旧会话保留原路径 |
| **D2 持续通道** | 同进程多轮；`RuntimeChannel` 非 `legacy` | `is_runtime_chat_agent` 且**新空**会话（有历史的旧会话可能仍 D1） |
| **D3 宿主加深** | 允许/拒绝卡、斜杠原生命令目录、终端宿主卡、「用于本次」技能、模型/思考 Options 等 | 各家协议真能力；未声明则 fail-closed，不造假按钮 |

深度可独立加深：新家可以先 D0+D1，再按协议证据爬到 D2/D3。不要为了「目录好看」把 Planned 写成 Full，也不要用本表替代 `capability()`。

## 现行矩阵（`dev`）

| Agent | D0 | Chat 主档 | `RuntimeChannel`（新空） | D3 摘要（新空持续会话） | 备注 |
|---|---|---|---|---|---|
| Claude | 有 | **D2** 新空；有历史仍 **D1** print+resume | `stream-json` | 模型/思考、图片；TodoWrite / Task 任务清单进计划条；**无**允许/拒绝卡（`dontAsk` / 危险 `bypassPermissions`）；无「用于本次」 | 见 STATUS / Claude B3 |
| Codex | 有 | **D2** 新空 | `app-server` | 允许/拒绝/一直允许、模型/思考、图片、「用于本次」、斜杠动态菜单；计划模式未做；CU 无产品路径 | 最深 |
| Grok | 有 | **D2** 新空 | `acp` | 允许/拒绝（对方选项）、模型/思考、图片；**无**「用于本次」；生成中不可补充（可排队） | ACP 族 |
| Kiro | 有 | **D2** 新空 ACP；旧会话 **D1** | `acp` | 允许/拒绝、停止；模型/思考/权限启动后固定；生成中不可补充 | ACP 族；旧 headless 不切 ACP |
| Kimi | 有 | **D1** | `legacy` | — | 技能读 `~/.agents/skills` 原生，无 Hub 投影目标 |
| Pi | 有 | **D1** | `legacy` | — | |
| WorkBuddy | 有 | **D1** | `legacy` | — | |
| ZCode | 有 | **D1** | `legacy` | — | |
| DSH | 有 | **D1** | `legacy` | — | |
| Cursor | 有（软隐藏） | **D1** | `legacy` | — | 默认软隐藏 ≠ 删除适配器；见 [身份兼容族](../concepts/agent-identity-families.md) |

「斜杠：Hub 动作 vs 对方命令」等宿主加深事实以 `docs/STATUS.md` 与 Chat 概念页为准；本表只标深度档，不重复操作说明。

## 与能力矩阵的分工

| 问题 | 看哪 |
|---|---|
| 能否装、探测、写配置、Skills 投影、MCP、用量？ | [capabilities.md](capabilities.md) |
| Chat 是否持续、有没有确认卡？ | **本页** + STATUS |
| 本机路由 / 登录池能否接到这家？ | [route-compatibility.md](route-compatibility.md) |
| 启动 argv 散落在哪？ | [agent-launch-inventory.md](agent-launch-inventory.md) |
| Options / 模型目录从哪来？ | [chat-session-options.md](chat-session-options.md) |

## 扩家时怎么用

1. 先诚实填 `capability()`（14 键穷尽）。  
2. 在本表为新 `AgentId` 增加一行：至少 D0；Chat 未接持续通道则写 D1 + `legacy`。  
3. 只有协议与真窗证据齐备时，才把白名单扩进 `is_runtime_chat_agent` 并升到 D2/D3。  
4. 失败后不得在持续通道与一次性之间**静默**切换。

相关：[添加 Agent](../guides/adding-an-agent.md)、[多 Agent 扩家硬约束](../guides/multi-agent-support-rules.md)、[身份兼容族](../concepts/agent-identity-families.md)。
