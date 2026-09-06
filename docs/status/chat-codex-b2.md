---
title: Codex Chat 第二批实施与交接
type: status
status: current
owner: maintainers
audience: chat implementers
updated: 2026-09-06
---

# Codex Chat 第二批实施与交接

承接 [B1 实施记录](chat-codex-b1.md) 与 [B2 交接提示词](../guides/chat-b2-handoff.md)。本记录只覆盖 **TRIMMED B2**（Codex 优先），不含计划模式、Claude B3、完整 A11–A17 剧场或无关全量失败修复。

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

- 新增 IPC：`chat_runtime_options`、`chat_runtime_set_settings`、`pick_chat_images`；`chat_runtime_start` 增加可选 `extras`。
- 迁移 `00032_chat_runtime_turn_settings`：`chat_runtime.next_model` / `next_effort`。
- 实现主要在 `services/chat_runtime/{ops,mod,store,types}.rs` 与 `src/pages/chat/*`。

## 本机定向验证（工作区实跑）

| 检查 | 结果 |
| --- | --- |
| `tsc -p tsconfig.app.json` | 通过 |
| `tsc -p tsconfig.test.json` | 通过 |
| `vitest run`（chat / mocks / tauri runtime / boundary / backend-features） | 14 文件、156 通过 |
| `cargo test -p agenthub-core --locked --lib chat_runtime` | 26 通过、1 忽略（真实 Codex opt-in） |
| `cargo test -p agenthub-core --locked --test chat_runtime_contract` | 6 通过 |

未用 mock 冒充桌面真实验收；B1 记录的全量前端/Rust 失败仍按用户要求本轮不追。

## 剩余缺口

- 真实 Codex：带图片/指定 skill/模型切换的桌面端到端仍待 opt-in 验收。
- Windows/Linux 与 B1 尾项真实验收仍属后续批次。
- 目录缓存按会话驻留；活动轮次不二次拉起 Codex 进程拉目录。
- 插件仍为状态展示，不可伪装为本轮可调用。

## Follow-up（本仓 `feat/chat-b2-followup`）

- 空闲进入 runtime 会话 / `start` 前预热目录缓存；冻结轮次只读缓存、不中途 spawn。
- 前端在冻结且目录为空时保留同会话上次非空列表，避免 UI 被刷空。
- Claude B3：探测后 **不接线**；见 [chat-claude-b3.md](chat-claude-b3.md)。
