---
title: Chat 独立审查交接提示词
type: how-to
status: current
owner: maintainers
updated: 2026-09-05
---

# Chat 独立审查交接提示词

以下提示词默认审查本次 B1 提交；审查后续 B2 时明确替换范围与目标提交。验证记录见 [B1 实施记录](../status/chat-codex-b1.md)。

```text
请对 AgentHub Chat 的本次提交做独立、只读 review。先报告问题，不直接修改代码，不复述实现者的“已通过”作为审查证据。

范围：
- 优先采用用户提供的 B1 提交 SHA。未提供时，git log --diff-filter=A --format=%H -- docs/guides/chat-review-handoff.md 可定位首次新增本文的交付提交。
- B1 之前基线是 db40f6fbf9c86a184707f7ff8259a5bac361bb24；看基线到目标提交的实际 diff。不要无意把后来的 B2 改动混入，也不要重置当前工作区。
- 读 AGENTS.md、docs/status/chat-codex-b1.md 和 docs/proposals/chat-unified-experience.md，再从 diff/调用方/相关测试进入；结构问题优先 CodeGraph，不可用时定向读取。

重点审查：
1. 公共入口：空 Codex snapshot 是否选择 runtime；旧历史/非 Codex 是否仍兼容；不支持安装/登录/协议错误是否明确，不回退 mock 或静默 legacy。
2. 正文和回放：currentMessage 是否与事件/请求同一数据库视图；是否用 sequence 去重，绝不按文本相似性吞掉 ha+ha 等合法增量；A→B→A、非活动快照、gap、慢轮询和旧 DB 加载是否会覆盖新正文或重复过程。
3. 确认/问答：真实 runId/requestId/clientRequestId；过期请求、重复提交、错误重试、秘密输入、完整答案校验；请求卡失败后可重试；不把普通聊天文本识别成批准。
4. 停止和幂等：分别验证 stop-first 与 allow-first 的实际 wire 顺序，不能拿 transport=None 的失败测试冒充允许成功；pending/accepted/failed 结果不能互相伪装；停止不表示已执行的副作用被撤销。
5. 状态和进程：启动每个失败点、willRetry、原生 completed/interrupted、进程崩溃、删除/退出；消息与终态事件和 pending 清理是否同事务，是否遗留无限 running 或进程。
6. 持久化：迁移前备份是否包括 WAL、失败不升级、不覆盖用户数据；事务回滚、恢复、中断、有限回放/gap；独立升级尝试可能多份 UUID 备份是已记录取舍，不假定最多一份。
7. Backend 分层：invoke 只在 tauri adapter；mock 不进入生产；UI 禁用不能代替后端校验；活动会话的 Agent/目录不能混用旧原生 thread。
8. 证据边界：真实 macOS 续聊、直接协议实验、fake 与纯状态测试分别覆盖什么；不能把 schema 存在或模型/插件列表当作可调用证明。检查跨平台命令/进程清理和路径边界。

验证参考：
pnpm typecheck
pnpm typecheck:test
pnpm exec vitest run src/pages/chat src/dev/mocks/chat-runtime.test.ts src/lib/backend/tauri/chat-runtime.test.ts src/lib/backend/boundary-imports.test.ts src/lib/api/backend-features.test.ts
cargo test -p agenthub-core --locked --lib chat_runtime
cargo test -p agenthub-core --locked --test chat_runtime_contract
cargo test -p agenthub-core --locked --lib services::chat_service
cargo test -p agenthub-core --locked --lib storage::migrations::tests
cargo test -p agenthub-gui --locked
pnpm check:docs

已知失败必须处理为证据，不能跳过记录：
- 全量 pnpm test：agent-detail-model.test.ts 的 4 个 markup 用例、WriteClientConfigDialog.test.tsx 的 2 个 markup 用例失败。
- 全量 cargo core：install_progress_events_share_one_nonempty_operation_id；config_purge_accepts_safe_custom_temp_directory；grok_hub_pkce_refresh_writes_auth_json_when_same_identity_row_is_newer；imports_legacy_json_then_removes_it；detached_descendant_pipe_is_bounded_without_post_reap_group_signal。
- 原始现象/准确模块见 B1 实施记录。没有完成 baseline 对照，不要断言均与本次无关。用户明确本轮先记录、后续再处理；审查可以分级归属，未经新授权不要顺便重构相关模块或削弱测试。
- 两个旧测试文件的类型 fixture 修正随本次提交，确认它们不改断言语义。

输出：
- APPROVED 或 CHANGES REQUIRED，P0/P1/P2 按严重程度排列。
- 每项给文件与行、触发条件、影响、证据或最小复现、建议修复；区分实证问题与尚未验证的风险。
- 列出亲自执行的命令和结果，以及未执行/环境受限部分。
- 模型/菜单/附件/扩展仍属于 B2，Claude/其他 Agent 属于 B3，不把已明确未做的后续功能误报成 B1 回归。
- 不自动提交、推送、改用户级配置或使用真实账号发起任务；真实 Codex 实验需按当前用户授权和文档显式 opt-in。
```
