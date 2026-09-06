---
title: Codex Chat 第二批实施与交接
type: status
status: current
owner: maintainers
audience: chat implementers
updated: 2026-09-06
---

# Codex Chat 第二批实施与交接

承接 [B1 实施记录](chat-codex-b1.md) 与 [B2 交接提示词](../guides/chat-b2-handoff.md)。本记录只覆盖 **TRIMMED B2 / B2.1**（Codex 优先），不含计划模式、Claude B3、完整 A11–A17 剧场或无关全量失败修复。

## 交付范围与结果

| 项 | 状态 | 说明 |
| --- | --- | --- |
| 会话 model + effort（`model/list`） | 已完成 | `runtimeOptions` / `runtimeSetSettings`；活动轮次冻结；校验失败保留原值；不走旧 `setChatModel` |
| 最小操作菜单 | 已完成 | 菜单按钮与 `/` 搜索共用 `chat-actions.ts`；草稿示例任务；普通路径中的 `/` 不触发 |
| 已验证图片附件 | 已完成 | 仅 `localImage`；选择/移除/类型与大小校验；成功发送后清空草稿附件 |
| Skills/插件发现与状态 | 已完成 | `skills/list` + `plugin/installed` 状态展示；显式 skill 需名称+路径且对照目录；安装≠启用≠加载≠可调用 |
| 计划/协作模式 | 未做（明确 OUT） | — |
| Claude / 其他 Agent | 未做（B3） | — |
| 完整扩展安装/MCP | 未做（A17 后续） | — |

## 关键面与持久化

- 新增 IPC：`chat_runtime_options`、`chat_runtime_set_settings`、`pick_chat_images`、`save_chat_paste_image`；`chat_runtime_start` 增加可选 `extras`。
- 迁移 `00032_chat_runtime_turn_settings`：`chat_runtime.next_model` / `next_effort`。
- 实现主要在 `services/chat_runtime/{ops,mod,store,types}.rs` 与 `src/pages/chat/*`。

## B2.1 整包（本 PR `feat/chat-b2-followup`）

在 B2 之上继续装整包能力，而不是零碎 polish：

| 项 | 状态 | 说明 |
| --- | --- | --- |
| Composer / 下一轮设置 UX | 已完成 | runtime 开启时 model + effort **始终可见**；冻结/目录空/加载中/未选模型给出禁用原因；拒绝设置后恢复原值 |
| 目录预热与空列表 | 已完成（含 follow-up） | 空闲预热缓存；冻结轮次不 spawn；前端保留同会话上次非空目录 |
| 操作菜单整包 | 已完成 | 新建/历史/历史搜索聚焦/复制回复/设置/Agents/Connections；更多中文示例草稿；`/` 中文友好过滤；方向键+Enter/Esc；禁用原因；不把未实现原生命令伪装成提示词 |
| 附件整包 | 已完成 | 多选 `pick_chat_images`；前后端 8 张 / 10MB；类型错误提示；桌面路径粘贴图片 → `save_chat_paste_image`；非 localImage 明确禁用说明（不发明 path-string 附件） |
| Skills / 插件 | 已完成 | 有稳定 path 的 skill 可「用于本次任务」并进入 `turn/start` extras；enabled/loaded/unknown/插件仅状态 如实展示；插件不假装可调用 |
| 结果 / 错误 UX | 已完成 | 失败/中断/停止/超时横幅 + 填回草稿 + 重试；结果只认 `message.status`/结构化错误，**不**把模型正文里的 “tests passed” 当成验收 |
| Claude B3 | 仍阻塞 | 见 [chat-claude-b3.md](chat-claude-b3.md)，本批不接线 |

## 本机定向验证（工作区实跑）

| 检查 | 结果 |
| --- | --- |
| `tsc -p tsconfig.app.json` | 通过 |
| `tsc -p tsconfig.test.json` | 通过 |
| `vitest run`（chat / mocks / tauri runtime） | 14 文件、150 通过 |
| `cargo test -p agenthub-core --locked --lib chat_runtime` | 30 通过、1 忽略（真实 Codex opt-in） |
| `cargo test -p agenthub-core --locked --test chat_runtime_contract` | 6 通过 |
| `cargo test -p agenthub-gui --locked paste_image` | 2 通过 |

未用 mock 冒充桌面真实验收；B1 记录的全量前端/Rust 失败仍按用户要求本轮不追。

## 剩余缺口

- 真实 Codex：带图片/指定 skill/模型切换的桌面端到端仍待 opt-in 验收。
- Windows/Linux 与 B1 尾项真实验收仍属后续批次。
- 目录缓存按会话驻留；活动轮次不二次拉起 Codex 进程拉目录。
- 插件仍为状态展示，不可伪装为本轮可调用。
- 普通文件/音频附件：协议未验证，保持禁用。
- Claude ChatRuntime / 假确认：仍阻塞。

## Follow-up：model × reasoning effort 兼容（本分支）

真实验收曾出现：`gpt-5.3-codex-spark` + `medium` 发送后失败（「这个模型不支持当前思考设置」），控件仍展示不兼容强度。

已补整包：

- UI effort 菜单只来自该模型的 `supportedReasoningEfforts`；切换模型时重置为 default / 首个支持值。
- 空闲 `runtimeOptions` 会 reconcile 掉库存里的不兼容 pair；冻结轮次仍展示当轮有效 pair，不提供无效选项。
- `runtimeSetSettings` / `start` 一致拒绝不支持的 (model, effort)；`defaultReasoningEffort` 若不在支持列表则回退到首个支持值。
