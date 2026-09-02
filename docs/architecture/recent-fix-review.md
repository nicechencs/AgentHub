---
title: 最近提交修复复核
type: architecture
status: historical
owner: maintainers
updated: 2026-08-26
---

# 最近提交修复复核

本页是 2026-08-26 的一次性复核记录，不是现行契约。当前实现事实见 [STATUS](../STATUS.md)。

## 结论

最近提交形成了连续的修复链，但不能判定为“全部修复完毕”。多项问题已经收窄或修复，仍有高风险的状态刷新、Usage 数据迁移和 Core 封装边界问题需要处理。

本次复核范围为 `d0dad29e^..17a9edf0` 的审查文档提交及其后的修复提交，当前工作区另有未提交改动，不计入已提交修复结论。

## 验证结果

- `pnpm typecheck`：通过。
- `pnpm test -- --run`：通过，194 个测试文件、1721 个测试。
- `pnpm check:docs`：通过，58 个 Markdown 文件。
- `cargo test -p agenthub-core --locked`：编译通过，1953 个测试通过、1 个失败、1 个忽略；失败用例因当前环境缺少 `nodejs` / `npm`，不是断言失败，因此 Core 全量验证不能标记为完全通过。

## 仍未完整修复的问题

### R-01｜Core 允许外部替换单个 Service

- **位置：** `crates/agenthub-core/src/lib.rs:315-323` 的 `set_providers`、`set_accounts`
- **问题：** 公开 setter 可以替换单个 Service，外部容易构造出与同一数据库、Registry 或锁不一致的依赖图。
- **建议：** 移除生产公开 setter；测试需要时放入 test-support 构造器或由 Builder 一次性组装完整依赖图。
- **影响/风险：** 状态 owner 被绕过，运行时可能出现“Service 看似已替换、关联状态仍来自旧实例”的不一致。

### R-02｜Core 数据库和 Repository 仍暴露过宽

- **位置：** `crates/agenthub-core/src/lib.rs:221-225`、`crates/agenthub-core/src/storage/mod.rs:136`、`crates/agenthub-core/src/services/provider_service.rs:1191`
- **问题：** `Core::db`、`Database::with_conn` 和 Provider 的 `repo` 仍可被外部直接使用，数据库写入和内部结构可以绕过领域 Service。
- **建议：** 收窄为 `pub(crate)`，对跨层需求提供领域级读写方法；测试专用能力隔离到 test-support。
- **影响/风险：** 外部代码可绕过状态管理、缓存失效和业务约束，O-01/O-02 只能算部分处理。

### R-03｜Provider 批量删除后连接池可能保留旧数据

- **位置：** `src/lib/api/provider.ts:64-80`、`src/app/runtime/connection-pool-store.ts:122-130`
- **问题：** `deleteProviders` 在批量变更期间抑制通知，但结束时只有收到变更通知才会强制重载连接池；后端若未发通知，删除成功后连接池仍可能显示已删除 Provider。部分失败时也可能留下旧快照。
- **建议：** 批量删除成功或失败收尾时显式刷新连接池，并把刷新结果纳入统一读模型状态。
- **影响/风险：** Provider 列表、连接选择和后续写入使用过期状态。

### R-04｜Usage v5 迁移会清空全部 Agent 历史

- **位置：** `crates/agenthub-core/src/services/usage_service.rs:449-478`、`crates/agenthub-core/src/storage/usage_repo.rs:103-108`
- **问题：** 首次进入 v5 时 `maybe_repair_token_layout` 调用 `clear_all_records`。如果本次只收集一个 Agent，其他 Agent 的历史会被删除且不会在同一次调用中重建。
- **建议：** 使用幂等的列级迁移或按 Agent 迁移；必须保留其他 Agent 历史，并补充“单 Agent collect 首次触发迁移”的回归测试。
- **影响/风险：** 高风险数据丢失。

### R-05｜切换确认/预览仍可能触发写入型同步

- **位置：** CLI 切换确认路径及 `AccountService::list` / `ProviderService::list`
- **问题：** 确认前读取列表可能触发 live reconcile、修复或同步；用户取消操作后，系统仍可能已经改变状态。
- **建议：** 提供明确的只读 `switch_preview`，确认阶段只读取快照，真正的同步和写入放到确认后的执行阶段。

### R-06｜实时认证探测吞掉投影查询错误

- **位置：** `crates/agenthub-core/src/services/account_service/live_reconcile.rs:501-517`
- **问题：** `live_is_adapter_projection(...).unwrap_or(false)` 把查询失败当成“不是投影”，随后可能继续按不存在处理。
- **建议：** 传播错误或返回带完整性的探测结果，不能将读取失败降级成缺失状态。

### R-07｜Bridge 路由池登记尚未纳入完整补偿边界

- **位置：** `src-tauri/src/bridge/controller.rs` 的 enrollment、Provider projection/finalize 路径
- **问题：** 路由池 v2 登记和 Provider 投影分属不同步骤；后续失败时，补偿未完整恢复 `v2_enrolled`、`gateway_port` 和 revision。
- **建议：** 由 Core saga 统一持有快照、提交和回滚，或为每个外部写入定义明确的补偿动作和回归测试。

### R-08｜Mock Backend 工厂仍共享模块级状态

- **位置：** `src/dev/mocks/create-backend.ts`、`src/dev/mocks/skill.ts`、`src/dev/mocks/usage.ts`、`src/dev/mocks/account.ts`
- **问题：** `createBackend` 虽然提供 reset，但多个 Backend 实例仍共享模块级状态；重置一个实例会影响另一个实例。
- **建议：** 把状态移入 `createBackend` 创建的端口实例，或明确只允许单例并禁止伪装成多实例工厂。
- **影响/风险：** 测试相互污染，页面或场景之间出现跨实例状态泄漏。

### R-09｜Mock OAuth 没有完整校验和消费会话

- **位置：** `src/dev/mocks/account.ts`
- **问题：** PKCE/device 会话虽记录了 flow，但后续方法未校验 flow；完成后也未消费会话，且仅用 `Date.now()` 生成状态。
- **建议：** 按 flow 校验入口、完成后删除或标记一次性会话，并使用稳定的唯一状态生成方式。

### R-10｜Agent catalog 仍是浅冻结

- **位置：** `src/config/agents.ts` 的 `applyAgentCatalog`
- **问题：** 只冻结顶层集合和对象，嵌套的 channel、requires、capabilities 等仍可能被外部修改，部分数组还直接沿用 DTO 引用。
- **建议：** 在目录 owner 内完成深层不可变快照，外部只取得只读查询结果。

### R-11｜路由移除提示仍把读取失败当成来源已移除

- **位置：** `src/pages/routes/pool/index.tsx`、`src/pages/bridges/adapter-view-model.ts:203`（当前兼容路径）
- **问题：** Ticket wallet 或连接池读取失败时，视图仍可能按空结果分类为 orphan/source removed，掩盖真实读取错误。
- **建议：** 保留各读取源的完整性和错误状态；只有在相关读取都成功时，才判定来源确实已移除。

## 状态对照

- **已确认修复：** O-06、O-09、O-10、O-43、O-49、O-51、O-52、O-53、O-55、O-57、O-60、O-65。
- **部分修复：** O-01、O-02、O-08、O-18、O-21、O-36、O-40、O-50、O-54、O-56、O-61、O-62。
- **仍未修复或本轮未证明已修复：** O-17、O-20、O-31、O-32、O-33、O-34、O-44、O-58、O-59、O-63、O-64、O-66、O-67。

## 处理顺序

优先处理 R-04、R-03、R-01/R-02；随后处理 R-05 至 R-11。完成前，不应把 O-01、O-02、O-08 或 Usage v5 迁移标记为“已处理”。
