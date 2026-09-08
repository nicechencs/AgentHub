---
title: Codex Chat 第二批实施与交接
type: archive
status: archived
owner: maintainers
audience: chat implementers
updated: 2026-09-08
---

# Codex Chat 第二批实施与交接

> **归档（2026-09-08）**。本页是一次性审查、批次记录或复核，不是现行契约。当前实现见 [当前实现状态](../STATUS.md)。下文保留原文，不要按此页派工。
>
> **Archived / 已归档**: Historical review or batch record. Do not use it as the current implementation contract.

承接 [B1 实施记录](chat-codex-b1.md)。当时的 [B2 交接提示词](chat-b2-handoff.md) 已归档。本记录只覆盖 **TRIMMED B2 / B2.1**（Codex 优先），不含计划模式、Claude B3、完整 A11–A17 剧场或无关全量失败修复。

## 交付范围与结果

| 项 | 状态 | 说明 |
| --- | --- | --- |
| 会话 model + effort（`model/list`） | 已完成 | `runtimeOptions` / `runtimeSetSettings`；活动轮次冻结；校验失败保留原值；不走旧 `setChatModel` |
| 最小操作菜单 | 已完成 | 菜单按钮与 `/` 搜索共用 `chat-actions.ts`；草稿示例任务；普通路径中的 `/` 不触发 |
| 已验证图片附件 | 已完成 | 仅 `localImage`；选择/移除/类型与大小校验；成功发送后清空草稿附件 |
| Skills/插件发现与状态 | 已完成 | `skills/list` + `plugin/installed` 状态展示；显式 skill 需名称+路径且对照目录；安装≠启用≠加载≠可调用 |
| 计划/协作模式 | 未做（明确 OUT） | — |
| Claude 持续聊天（ChatRuntime） | 未做（B3 探测为否定） | Grok / Kiro 不在本批、已另接线；见 [STATUS](../STATUS.md) 与 [Claude B3](chat-claude-b3.md) |
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
- Claude 持续聊天（ChatRuntime）/ 假确认：仍阻塞。Grok / Kiro 已接线，不在此缺口。

## Follow-up：model × reasoning effort 兼容（本分支）

真实验收曾出现：`gpt-5.3-codex-spark` + `medium` 发送后失败（「这个模型不支持当前思考设置」），控件仍展示不兼容强度。

根因（2026-09-06 复核）：不是解析形状漏字段。本机 Codex `models_cache.json`（client 0.150.1）里 spark 的 `supported_reasoning_levels` 明确列出 `low/medium/high/xhigh`（default `high`），app-server `model/list` 同形对象（`reasoningEffort` + `description`）。目录**多报**了会在 turn/start 失败的 effort；`effortsForModel` 无法单靠 catalog 去掉 medium。

已补整包：

- 解析：`supportedReasoningEfforts` 支持 string / `{reasoningEffort|effort}`；`available:false` / `supported:false` 跳过；去重保序。
- UI effort 菜单来自 **effective** 支持集（catalog − 已学习拒绝）；切换模型时重置为 default / 首个有效值。
- 空闲 `runtimeOptions` reconcile 不兼容 pair；冻结轮次仍展示当轮有效 pair。
- `runtimeSetSettings` / `start` 仍拒绝 effective 集外的 pair。
- **Learn-from-reject**：turn 失败文案命中 thinkingUnsupported（含「不支持当前思考设置」）时，把该 model×effort 写入 sqlite `chat_runtime_denied_efforts`（+ 前端 session），之后菜单不再提供；并 coerce 到 default/剩余首项。

agenthub-2 复测要点：

1. 切到 `gpt-5.3-codex-spark` → effort 自动落到 `high`（PASS 项保持）。
2. 若首次仍见 medium：选 medium 发送应失败一次；失败后菜单应去掉 medium，当前 effort 回到 high/low/xhigh 之一；再发应成功。
3. 重启应用后 medium 仍应被过滤（sqlite 持久化）。
