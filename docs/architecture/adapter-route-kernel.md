---
title: Adapter 路线内核与查表投影
type: architecture
status: current
owner: maintainers
audience: adapter, mock, and route contributors
source-of-truth: crates/agenthub-core/src/services/adapter_route_service, src/dev/mocks/adapter, and src/dev/mocks/fixtures/adapter-capability-contract.json
updated: 2026-09-29
---

# Adapter 路线内核与查表投影

本页说明「一份登录能不能接到某个 Agent、走哪条路线」由谁决定，以及浏览器 mock 怎样复用这个结果。

一句话：`AdapterRouteService::plan()`（`crates/agenthub-core/src/services/adapter_route_service/plan.rs`）是唯一决策者。Tauri 只传输它的结果；mock 只查它生成的快照；页面不重新决定路线。

## 唯一内核

`plan()` 返回 `AdapterApplyPlan`，其中这些字段是真源：

- `analysis` 里的 `route`、`support`、`ruleId`、`gateKind`、`reason`、`actions`；
- 外层的 `canApply`、`maturity`、`reusePath`、`serviceImpact`、`changes`。

只看路线矩阵格子上的 `can_apply` 不够：被私有写入门挡住的边（例如某些 Account 来源），演示里也必须显示不可写。

## 快照（golden）

`src/dev/mocks/fixtures/adapter-capability-contract.json` 是 `plan()` 对一组冻结入参的输出快照（下称 golden）。冻结入参用 preset、accountKind、extra 等种子形状，不含真实密钥。

Rust 测试 `shared_capability_contract_is_kernel_plan_projection` 要求 JSON 等于当场跑出的 `plan()` 结果；内核变了或手改 JSON 都会失败。更新快照只能用：

```text
UPDATE_ADAPTER_CAPABILITY_CONTRACT=1 cargo test -p agenthub-core --locked shared_capability_contract_is_kernel_plan_projection
```

不得把 JSON 当规则真源，也不得引入 WASM、napi 或类型生成框架在前端「再跑一遍 planner」。

## mock 只查表

`pnpm dev:mock` 和 Vitest 需要可演示的状态和内存写入，但不需要第二套路线选择器。实现在 `src/dev/mocks/adapter/`：

- `golden-lookup.ts` 按 `(来源 kind, 来源特征, 目标 Agent, 是否有可用凭据)` 查 golden。凭据可用性必须精确匹配，不能只参与打分。
- 未命中一律返回 `unsupported`，不回退启发式分类。
- `project.ts` 按 `ruleId` 生成演示用的 actions、changes、serviceImpact 等预览细节；`route`、`support`、`reason`、`gateKind`、`canApply` 都直接取自 golden。这不是第二套路线决策。
- 写入（`src/dev/mocks/adapter.ts` 的 apply 路径，物化在 `adapter/apply.ts`）只照 golden 执行：`canApply` 为假就拒绝；为真则写内存 profile 和假的本机转发状态，不重新分类，也不重放写入门。

**凭据是否可用：** mock 里的登录可能已脱敏，所以先看状态字段，再看内容。

1. `tokenValid === false`，或 `liveAuthHealth` / `authHealth` 为 `needs_login`、`missing` → 不可用。
2. `tokenValid === true`，或上述字段为 `verified`、`renewable`、`configured` → 可用。
3. 状态未知时才看 `credentials`：空对象为不可用；API Key 或 access token 非空为可用。

golden 行本身没有状态字段，只按 `credentials` 内容建索引。需要「能预览但不能写」的场景时，补一行由 `plan()` 生成的无凭据 golden，不在 TypeScript 手写规则。

测试分工：页面测试只断言「给定一份 plan，界面是否照做」；路线本身对不对只在 Rust 测试里判定。已知演示种子必须命中 golden，plan / apply 不得泄漏凭据占位值。

## 传输层

Tauri command 的数据形状沿用 core 的 serde 结构。`src/lib/backend/contracts` 描述该形状并映射 unavailable 与错误码；前端与 Rust 平行的结构只保留界面格式化。`src/lib/api` 是过渡层，不再加厚。生产构建不加载 mock，边界见 [前端与 Backend 边界](frontend-backend.md)。

## 验证范围

风险分级以 [AGENTS.md](../../AGENTS.md) 为准，命令见 [测试与验证](../guides/testing-and-validation.md)。按改动选内环：

| 改了什么 | 内环 | 不要默认升级为 |
|---|---|---|
| 页面或页面 model | 对应 `vitest run <file>`，必要时 `pnpm typecheck` | 整个 `agenthub-core` 的 `cargo test`、`pnpm test:pr` |
| 路线矩阵 / planner | `cargo test -p agenthub-core --locked <filter>`，并确认 golden 未过期 | 在 mock 里再写一套分类 |
| 数据形状 / port | 对应契约测试 + typecheck | 为了安心编译 GUI crate |

页面抽 model / hook 的时机：不抽就写不了针对性测试，或两处已在复制同一判断。文件大只是调查信号，不是拆分理由。

`agenthub-core` 保持单一 crate，不按目录拆包，也不启用 sccache。否决过程见 [单一内核提案归档](../archive/single-kernel-projections.md)。

## 相关页面

- [前端与 Backend 边界](frontend-backend.md)
- [Core 与 Runtime](core-runtime.md)
- [Adapters 与本机 Bridge](../concepts/adapters-and-bridges.md)
- [测试与验证](../guides/testing-and-validation.md)
- [当前实现状态](../STATUS.md)
