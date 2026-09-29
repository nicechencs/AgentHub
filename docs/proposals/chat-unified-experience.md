---
title: Chat 统一体验与跨环境开发方案
type: proposal
status: proposed
owner: maintainers
audience: product owners and implementation agents
updated: 2026-09-29
---

# Chat 统一体验与跨环境开发方案

> 状态：proposed。Codex、Grok、Kiro、Claude 的新空会话已接持续通道；Claude 允许/拒绝、其余 Agent 接入和三平台验收仍未完成。现行事实见 [STATUS](../STATUS.md) 与 [Chat 体验标杆](../ui/chat-experience-bar.md)。

目标：用户在 AgentHub 用同一套聊天、确认和文件查看操作使用不同 Agent；换电脑或换开发 Agent 后，实现能复现、验收和继续推进。对标 Claude Code、Cursor Chat、Codex app。宿主加深（ACP 目录、`/` 命令、宿主终端）的切片已全部落地，记录见 [Chat 宿主加深（归档）](../archive/chat-host-depth.md)；它剩下的真窗验收和拆分持续通道白名单并入本页。

## 当前基线

| Agent / 项 | 现状 | 证据 |
| --- | --- | --- |
| Codex | 新空会话走 app-server 常驻进程：持续回复、命令与文件确认、补充/停止、同机重开、模型/思考强度、本地图片、「用于本次」技能。Linux 真窗已验，macOS 重开有效 | [STATUS](../STATUS.md) Codex 段；[B1](../archive/chat-codex-b1.md)、[B2](../archive/chat-codex-b2.md) |
| Grok | ACP 持续通道；无「用于本次」技能 | STATUS Grok 段 |
| Kiro | ACP 新对话；生成时不能中途补充；旧对话保留原方式 | STATUS Kiro 段 |
| Claude | 新空会话 stream-json 多轮 + 图片；**没有可点的允许/拒绝**；旧会话 print + resume | STATUS Claude 段；[Claude B3](../archive/chat-claude-b3.md) |
| 其余 Agent、所有旧会话 | 一次性发送 | STATUS Chat 表 |
| 持续通道白名单 | Codex / Grok / Kiro / Claude | `crates/agenthub-core/src/services/chat_runtime/store.rs` 的 `is_runtime_chat_agent` |
| 页面读取方式 | 后台持有会话，SQLite 保存事件，页面约 80ms 读快照，不用推送订阅 | STATUS「通用行为」 |

## 剩余目标

| 项 | 门槛 |
| --- | --- |
| Claude 允许/拒绝 | 有真实确认通道（官方 Agent SDK 或其他已验证接口）后才画卡片；先确认登录与计费边界，不假定 SDK 等同订阅登录 |
| Pi / Kimi / ZCode 等持续通道 | 每家单独给出协议证据、最小样例、停止与恢复测试；验证一家开放一家。仅有 stdout 不算完整交互 |
| Codex 文本问答 | 已能处理 `item/tool/requestUserInput`，但当前默认模式下上游不发。等上游稳定默认再跟，不开实验开关 |
| Codex 计划模式、完整扩展管理 | 未做；扩展管理在 [插件管理](plugin-management.md) |
| 三平台真实验收 | Windows 上的 Codex 对话未宣称；未验证平台不宣称相同支持 |
| 跨电脑迁移原生会话 | 不在当前范围；有需求时单独定范围 |

## 用户操作约定

| 操作 | 共用体验 | 能力不足时 |
| --- | --- | --- |
| 开始任务 | 选目录、Agent、连接，输入需求 | 说明未安装、未登录或不支持；不自动换 Agent |
| 执行过程 | 「正在读取 / 执行 / 修改」，详情可展开 | 无结构化过程则显示简洁状态，日志放详情 |
| 确认 | 原因、目标文件或命令、允许/拒绝 | 无真实确认通道不画可点的卡片 |
| 回答问题 | 单选、多选或文字；选中不等于已提交 | 不支持则不显示 |
| 补充 | 支持时即时补充并显示已接收 | 不支持时保留草稿，不标成已交给 Agent |
| 停止 | 先「正在停止」，确认后「已停止」 | 超时显示结果未知，不伪造成功 |
| 模型与模式 | 显示实际可用选项 | 不支持的隐藏；未知模式不翻译成「计划」 |
| 重开会话 | 先显示历史，再恢复真实状态 | 恢复失败保留原记录，给新建会话入口 |
| 切换 Agent | 新建所选 Agent 的会话 | 不把原生会话 ID 交给另一家 |

一个会话固定一个 Agent 和工作目录。权限默认用 Agent 自己的确认方式，不靠跳过确认让演示通过。

## 接入新 Agent 的不变量

1. 页面只认统一事件和能力，不解析各家 JSON，不按 Agent 名散布分支。
2. 能力逐项标 supported / unsupported / unknown，未知不启用。
3. 重复发送、重复点击按 requestId 去重；旧按钮在进程结束或实例变化后失效，迟到回复不交给新进程。
4. 每个 run 单一状态 owner：确认、回答、停止按同一顺序处理。停止先被接受时作废待处理请求；断线后结果不明不自动重放「允许」。
5. 事件先持久化再发布；切页、重挂载不终止后台任务；崩溃后残留运行标为中断，不自动重提交。
6. 失败后不在持续通道与一次性发送之间自动切换。
7. 数据库只做增量迁移；旧会话默认继续原方式，失败不清空原生会话 ID。
8. 进程参数按 argv 传递，清理整个子进程树；日志脱敏。

## 验收场景

真实验证记录 Agent/版本、OS/架构、commit、登录类别（不记密钥）、场景 ID。mock 不能当真实 Agent 验收。

| ID | 场景 | 必须看到的结果 |
| --- | --- | --- |
| A01 | 未安装 / 未登录 / 无目录 / 不支持版本 | 阻止发送并给下一步；不回退 mock |
| A02 | 读取文件并解释 | 正文与过程分开；完成后可重开 |
| A03 | 修改并跑测试 | 确认与实际请求对应；拒绝后不执行 |
| A04 | 单选 / 多选 / 文字提问 | 提交准确，重复点击不重复处理 |
| A05 | 执行中补充 | 支持时确认接收；不支持只保留草稿 |
| A06 | 确认与停止竞态、晚到回复 | 停止先接受则允许绝不发出 |
| A07 | 切会话、断开重连 | 不漏、不串、不重复 |
| A08 | 程序重启、子进程崩溃 | 不遗留无限运行，不自动重发 |
| A09 | 旧数据库、迁移失败 | 不丢消息；失败回滚 |
| A10 | 超大 / 未知事件、慢 UI | 有界处理，不漏确认和终态 |
| A11 | 模型 / 思考强度 / 模式切换 | 实际生效与界面一致；拒绝时保留原值 |
| A12 | 三平台、中文与空格路径、输入法 | 路径和停止正确；组字不误发送 |
| A13 | 添加 / 移除附件 | 发送前可核对；失败保留草稿 |
| A14 | `/` 菜单、示例任务 | 示例只填草稿；本地操作不发给模型 |
| A15 | Skills / 插件可用、缺失、同名 | 按稳定 ID 和真实加载状态调用 |
| A16 | 完成 / 失败 / 测试未运行 | 结果有证据，未知不当通过 |

## 换电脑与换开发 Agent

- 可复现的是界面、协议处理、错误行为和验收步骤，不是模型回答。
- 固定场景放 `src/dev/mocks/`；脱敏协议 fixture 放各对接实现的测试目录；不依赖私人项目或个人插件。
- 方案、fixture、版本记录与代码进同一提交或 PR；只在本机的文件不能交接。
- 验证命令按 [测试与验证](../guides/testing-and-validation.md) 选择。

## 非目标

- 复制 VS Code 插件完整界面、IDE 选区同步、全量原生斜杠菜单、通用文件回滚、默认内嵌终端、自动云同步、跨 Agent 原生会话转换。
- 以伪终端作为对话底座。
- 凭据落盘加密、国产 OAuth 对接或转 API。API Key 可分享规则沿用现行，不按 Agent 名排除。

## 未决问题

- Claude 允许/拒绝走官方 Agent SDK 是否可行（登录与计费边界未定）。
- ZCode 尚无可承诺的公开聊天接口证据。

## 相关页面

- [Chat 宿主加深](../archive/chat-host-depth.md)
- [Chat 与 Agent](../concepts/chat-and-agents.md)、[Chat 体验标杆](../ui/chat-experience-bar.md)
- 历史：[S0](../archive/chat-codex-s0.md)、[B1](../archive/chat-codex-b1.md)、[B2](../archive/chat-codex-b2.md)、[Claude B3](../archive/chat-claude-b3.md)
