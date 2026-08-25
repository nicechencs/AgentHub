---
title: Adapter 路线内核与查表投影
type: architecture
status: current
owner: maintainers
audience: adapter, mock, and route contributors
source-of-truth: crates/agenthub-core AdapterRouteService, src/dev/mocks/adapter, and adapter-capability-contract.json
updated: 2026-08-25
---

# Adapter 路线内核与查表投影

`AdapterRouteService::plan()` 是 Adapter / route 产品规则的唯一决策者。Tauri 传输内核结果；`adapter-capability-contract.json` 是对冻结入参的只读投影，携带内核生成的路线决策结果；browser mock 按来源特征查表，并按 `ruleId` 在 `project.ts` 里维护演示所需的内存投影。页面不重新决定路线。

## 唯一内核

`plan()` 的输出才是真源：`route`、`support`、`ruleId`、`gateKind`、`canApply`、`reason`、reuse path 和 apply path。矩阵格子上的 `can_apply` 不够：Account 等被私有 write gate 挡住的边，演示里也必须显示不可写。

冻结入参使用已有的 preset、accountKind、extra 等种子形状，不写入真实密钥。Rust 测试要求工作区 JSON 等于刚跑出来的 `plan()` 结果；内核变化或手改 expect 都会失败。更新快照只允许：

```text
UPDATE_ADAPTER_CAPABILITY_CONTRACT=1 cargo test -p agenthub-core --locked shared_capability_contract_is_kernel_plan_projection
```

不得把 JSON 当成规则真源，也不得引入 WASM、napi 或类型生成框架来“再跑一遍 planner”。mock 与前端不得复制 classifier fallback 或第二套路由选择器。`ruleId` 会随 golden.expect 进入 mock；`project.ts` 仍按 `ruleId` 生成演示用的 materialization、actions、changes、maturity、serviceImpact。这不是第二套路由决策，也不等于完整 plan 已经没有任何 TypeScript 投影。

## mock 只查表

`pnpm dev:mock` 和 Vitest 需要可演示的状态和可编排的内存写入，不需要 classifier fallback 或第二套路由选择器。

- 种子账号/供应商继续带 preset 或等价特征。
- 查表键是 `(来源 kind, ticket 特征, target, 凭据是否可用)`。
- 凭据可用性必须精确匹配：只参与打分不够，候选唯一也不能忽略不匹配。
- 未命中 fail-closed 为 `unsupported`，不得回退启发式 classify。
- `apply` 只解释 expect：`canApply` 为假则拒绝；为真则按 route / `ruleId` 写入内存 profile 和假 bridge。不重新 classify，不重放 write gate。

live mock Account 可能已经脱敏。凭据是否可用以状态字段为准，优先级如下：

1. `tokenValid === false` 或负面 `authHealth` / `liveAuthHealth`（`needs_login`、`missing`）→ 无可用凭据
2. `tokenValid === true` 或正面 `authHealth` / `liveAuthHealth`（`verified`、`renewable`、`configured`）→ 有可用凭据
3. 状态未知时才检查 credentials 内容
4. `credentials: {}` 且状态未知 → 无可用凭据
5. 明确存在有效 API Key 或 access token → 有可用凭据
6. 明确存在空 token slot → 无可用凭据

冻结/测试行没有 `tokenValid` / `authHealth`，golden 索引只看 credentials 内容。需要“路线可预览但不可写”时，补充由 `plan()` 生成的无凭据 golden 行，不在 TypeScript 手写规则。

页面测试只断言“给定一份 plan，界面是否听从”。路线本身对不对只在 Rust 测试。已知演示种子必须命中，且 plan / apply 不泄漏凭据占位值。

## 传输层

Tauri command 的 wire 使用 core 的 serde 形状。`src/lib/backend/contracts` 描述该形状并映射 unavailable / 错误码。与 Rust 平行的前端结构体只保留 UI 格式化。`src/lib/api` 兼容层停止加厚，允许旧调用方继续工作。

生产构建不加载 mock。非 Tauri 的生产页面必须明确 unavailable，禁止静默回退。

## 验证跟爆炸半径走

继续使用 [测试与验证](../guides/testing-and-validation.md) 与 [AGENTS.md](../../AGENTS.md) 的同一套风险分级，不要另造命令表。

| 改了什么 | 内环 | 不要默认升级为 |
|---|---|---|
| 页面或页面 model | 对应 `vitest run <file>`，必要时 `pnpm typecheck` | 整个 `agenthub-core` 的 `cargo test`、`pnpm test:pr` |
| matrix / planner | `cargo test -p agenthub-core --locked <filter>`，以及 golden 是否过期 | 再手写一套 mock classify |
| wire DTO / port | 该契约测试 + 相应 typecheck | 为了安心编 GUI crate |
| 提交与 CI | 现有 PR CI 全量 | 把 CI 全量搬进每一次本地改动 |

局部改动由主 Agent 在同一回合完成实现和过滤测试。跨层 contract、Rust 核心规则和持久化才升级到独立审查。全量 `pnpm test`、完整 Rust crate 矩阵和生产 `pnpm build` 默认留给提交前或 CI。

页面抽取 model / hook：仅当不抽就写不了针对性测试，或两处已在复制同一判断时进行。文件大只是调查信号，不是拆分理由。

## 编译与 crate 边界

`agenthub-core` 保持单一 crate。不落地 sccache 仓库配置，也不按目录或 DDD 名称拆包。Windows 上多个 worktree 不得共享同一份 `target/`。历史测量与否决过程见 [单一内核提案归档](../archive/single-kernel-projections.md)。

## 相关页面

- [前端与 Backend 边界](frontend-backend.md)
- [Core 与运行时](core-runtime.md)
- [Adapter 与本机路由](../concepts/adapters-and-bridges.md)
- [测试与验证](../guides/testing-and-validation.md)
- [当前实现状态](../STATUS.md)
