---
title: Codex Chat 第一批实施与交接
type: status
status: current
owner: maintainers
audience: chat implementers
updated: 2026-09-05
---

# Codex Chat 第一批实施与交接

承接 [统一体验方案](../proposals/chat-unified-experience.md) 与 [S0 协议检查](chat-codex-s0.md)。用户授权连续使用 GPT‑5.6 Terra / Luna 子 Agent 实施，并尽量合并批次。本记录区分代码实现、自动化验证和真实 Codex 实验。

## 批次与当前边界

剩余工作合并为 B1 会话基础、B2 模型/菜单/附件/扩展操作、B3 Claude/其他 Agent 与平台验收。B1 不等于全部完成；B2/B3 不因同一套 UI 而自动获得原生能力。

起点：`dev` / `db40f6fbf9c86a184707f7ff8259a5bac361bb24`，保留上一轮未提交的 S0 文件。本次没有自动提交或推送；换电脑前需将最终文件提交到可获取的分支，不能依赖 `temp/` 或聊天记录。

分工：Luna 实现 Codex 进程通信与后台会话；Terra 完成前端并独立审查后台；另一位 Luna 独立审查和修复前端；主 Agent 负责 IPC、联调、真实实验与最终验收。

## 接口与恢复方式

新增 `chat_runtime_snapshot/start/reply/steer/cancel`，与旧 `chat_send` 分开；所有 invoke 保持在 `src/lib/backend/tauri/`。Rust/TypeScript 对应定义在 `services/chat_runtime/types.rs` 与 `contracts/chat-runtime.ts`。

快照带 conversationId、runId、phase、lastSequence、有序事件、pendingRequests、currentMessage 和 gap。currentMessage 是同一数据库读取中的当前完整回复；正文不能靠字符串前缀/后缀猜测增量是否重复。确认回复带 requestId/clientRequestId；后台负责校验当前运行、请求有效性和重复提交。停止与回答在同一后台队列按接收顺序处理。

B1 页面采用约 400ms 的快照轮询，后台生命周期独立于页面。轮询必须防止重叠请求的旧结果覆盖新状态，切 A→B→A 使用会话世代隔离。读取失败要明确报错，不得自动调用旧发送。历史与事件合并必须按消息标识去重，gap 不能静默丢失。

## 真实协议实验

环境：Codex CLI 0.153.0、macOS arm64；使用本机已有 ChatGPT 登录，没有输出登录内容或修改用户配置。工作目录为仓库忽略的临时示例目录。下表是直接 app-server 实验，不能替代 AgentHub 自身的端到端验收。

| 实验 | 实际结果 | 限制 |
| --- | --- | --- |
| 固定正文，无工具 | 收到预期文本，`turn/completed: completed`，无 error | 仅覆盖基础正文，不等于 A02 读取文件验收 |
| 请求执行无害命令，拒绝 | 收到 command approval，回 `decline` 后 resolved 与 completed | 没有验证文件写入审批 |
| 请求执行无害命令，允许 | 一次返回 failed；复跑收到 approval，回 `accept` 后 resolved 与 completed，无 error | 首次未采集错误分类，原因未知；不能宣称稳定性全部通过 |
| 输出中停止 | `turn/interrupt` 后收到 `turn/completed: interrupted`，无 error | 允许/停止的两种竞态仍需确定性自动化覆盖 |
| 输出中补充要求 | `turn/steer` 返回成功，最终正文包含新要求的标记并 completed，无 error | 不等于页面操作与补充历史保存已验收 |

临时协议脚本不是交接依赖；后续以仓库内固定测试和可复跑的真实 runtime 测试为准。问答、文件审批、真实恢复与补充等能力必须分别记录，不能从 schema 推定已通过。

## 数据库升级前备份

已有聊天数据库首次应用 `00031_chat_runtime` 前，用 SQLite `VACUUM INTO` 生成包含已提交 WAL 的一致性副本。文件位于数据库同目录，名称为 `<数据库名>.before-chat-runtime-<UUID>.sqlite`；不覆盖已有文件、不自动恢复。备份失败阻止本次迁移。新数据库和已应用此迁移的数据库不重复备份。

每次独立升级尝试保存新的副本；同一次打开中的 migration busy 重试不重复生成。并发打开旧库，或失败后另一次启动，可能保存多份，避免复用已过时的备份。Unix 同步文件及父目录；其他平台仍需真实文件持久性验收。备份恢复只在关闭应用后的隔离副本中验证，不覆盖用户正在使用的数据。

## 已执行的产品链路验证

真实后台测试使用本机已有 Codex 登录，只把 AgentHub 数据库与工作目录放在临时目录，不输出登录信息；会产生正常的 Codex 原生会话。默认测试跳过，明确运行方式：

```sh
AGENTHUB_RUN_CODEX_RUNTIME_TEST=1 cargo test -p agenthub-core --locked --lib services::chat_runtime::tests::real_codex_runtime_start_and_resume -- --ignored --exact
```

本机实际通过，1/1，约 29 秒。第一轮要求记住随机标记，释放 ChatService，重开同一个 AgentHub 数据库后发第二轮。第二轮输入不含标记，持久化的最终回复仍正确包含该标记，验证了真实 `thread/resume` 与历史连续性。

此测试不覆盖所有 UI 操作、真实问答、文件审批或全部崩溃窗口。Windows/Linux 尚未实测，不能将 macOS 成功视作三平台通过。

## 验证与剩余工作

B1 核心代码已落地，后台与前端的最终独立审查通过。启动失败、重启恢复、删除会话的进程清理、持久化幂等、有限重放、消息与事件事务一致性、跨会话取消和回放重复问题已修复。非重试错误统一终结；会重试的上游错误保持运行；失败或结果未知的重复操作不能假报成功。原生完成状态不因本地停止请求被改写成取消。

主线程实际执行结果：

| 检查 | 结果 |
| --- | --- |
| `pnpm exec vitest run src/pages/chat src/dev/mocks/chat-runtime.test.ts src/lib/backend/tauri/chat-runtime.test.ts src/lib/backend/boundary-imports.test.ts src/lib/api/backend-features.test.ts` | 13 个文件、154 个测试通过 |
| `pnpm typecheck` | 通过 |
| `pnpm typecheck:test` | 未通过：未修改的 `page-chrome-model.test.ts` 两项、`page-help-tour.test.ts` 三项类型错误；没有本批新增文件错误 |
| `cargo check -p agenthub-gui --locked` | 通过，存在 unused/dead-code warnings |
| `cargo test -p agenthub-core --locked --lib chat_runtime` | 21 通过、1 默认忽略；包含备份测试；真实测试另按上文显式运行通过 |
| `cargo test -p agenthub-core --locked --test chat_runtime_contract` | 5 通过 |
| `cargo test -p agenthub-core --locked --lib services::chat_service` | 36 通过，旧发送行为回归检查 |
| `cargo test -p agenthub-core --locked --lib storage::migrations::tests` | 6 通过，包括事务回滚与并发打开 |
| `pnpm check:docs` / `git diff --check` | 通过 |

未运行全量应用测试和生产打包，也未完成桌面 GUI 全流程真人冒烟。B1 的核心实现与上述验证通过，不等于原方案全部 A01–A17 发布门槛通过。待补的真实场景包括文件审批、问答、不同权限/登录失败、崩溃与恢复失败的完整产品路径，以及 Windows/Linux。已有 fake/状态测试和 macOS 协议实验不能替代这些实际场景。

后续 B2 优先：以会话参数接入模型与思考强度，统一菜单动作，按真实输入能力添加附件，区分 Skills/插件发现与实际调用。B3 单独核对 Claude 登录及接口边界；ZCode 保持待验证，不扩大国产 OAuth 范围。

下一位开发 Agent 从 B2 开始，并将上述 B1 真实验收尾项并入后续批次验证，不需要重新设计已通过的会话底层；遇到具体失败时再定向扩大范围。不得将该安排解释为允许跳过发布门槛。
