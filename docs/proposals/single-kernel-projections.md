---
title: 单一内核与查表投影
type: proposal
status: proposed
owner: maintainers
updated: 2026-08-25
---

# 单一内核与查表投影

> 状态：提案
>
> 本文记录如何降低「改动很小、耗时很长」的结构成本。它不是当前实现契约，也不授权立即改 mock、拆 crate 或改协作流程。派工前必须指定负责人、文件范围、必须保持的行为和验证命令。

## 1. 当前基线

AgentHub 仍是模块化单体：GUI 和 CLI 共用 `agenthub-core`。产品写入走 `plan` / `bind` / `unbind`。前端路径仍是页面 → runtime → `#backend` → Tauri 或 browser mock；生产构建不加载 mock。

规则真源在 Rust：`adapter_capability_matrix` 加上 `AdapterRouteService` 的私有 write gate。`can_apply` 只是矩阵层标志；实际写入还要过 write gate、来源凭据和目标 writer。

浏览器 mock 与 Vitest 目前**重新实现**同一套 classify / plan / apply，并与 core 锁步维护 reason 字符串和投影表。`src/dev/mocks/fixtures/adapter-capability-contract.json` 已被 Rust 与 mock 测试同时读取，用于发现漂移；fixture 尚未覆盖全部公开 rule ID。补齐覆盖属于 [模块化提案](modularity.md) 的 C1，**不等于** mock 已经不再做决策。

验证方面，[测试与验证](../guides/testing-and-validation.md) 已按改动类型给出最小命令；PR CI 仍跑全量 typecheck、Vitest 和三个 Rust crate。协作规则另要求过滤测试之外由测试 subagent 跑完整检查。Grok / Agent worktree 通常没有共享的 `target/` 编译产物；crate.io 与 pnpm store 是全局的，Rust 增量缓存不是。

`agenthub-core` 仍是单一 crate。把 core 拆成多个 Cargo 包、把 planner 编成 WASM/napi、或引入类型生成框架，都不是当前实现。

## 2. 要解决的问题

一次可见的行为变化经常要穿过：

```text
页面或页面 model
  → lib/api 兼容层（若仍走 façade）
  → backend contract
  → Tauri adapter 与 command
  → agenthub-core planner / apply
  → 平行的 mock classify / plan / apply
  → Rust 测试、Vitest 领域测试、共享 JSON
```

慢的主因不是某一行业务代码，而是**同一条产品规则被决定两次**：core 决定一次，mock 再决定一次。JSON 目前是第三份对照，用来抓漂移，但没有取消第二台引擎。结果是：改一条 route 或一个 plan 字段，文件数远大于行为增量；再叠上偏大的验证范围和冷 worktree 编译，耗时与 diff 不成比例。

页面里把筛选/多选抽成 `*-model` 可以让纯函数可测，但若作为常设搬运任务，会在行为不变时固定增加文件、测试和 Agent 回合。这不是 C1 的替代，也不应从「文件较大」自动派生。

## 3. 候选目标

只保留一个会做路线决策的内核，其余层变成该内核的投影：

```text
唯一内核：AdapterRouteService::plan()
    │
    ├── 生产投影：Tauri command 传输 core 的 serde 形状
    ├── 演示投影：mock 按种子特征查表，并维护内存 profile / 假 bridge 状态
    └── 契约投影：由内核跑冻结入参得到的 JSON；禁止手写第二套 expect
```

产品语义保持不变：fail-closed、write gate、mock 不进生产、页面不直接 `invoke`、本机路由仍在当前桌面进程内。

### 3.1 内核是唯一决策者

`plan()` 的输出（route、support、rule ID、gate kind、`canApply`、reason、apply path）才是真源。不能只用矩阵格子生成 mock 结果：否则 Account 等被 write gate 挡住的边会在 `dev:mock` 里显示可写，和生产不一致。

冻结入参使用 preset、accountKind、extra 等已有种子形状，不写入真实密钥。内核序列化结果覆盖 `adapter-capability-contract.json`。测试断言工作区 JSON 等于刚跑出来的结果；不等则失败。JSON 的身份从「第三份真源」变成内核的只读快照。

reason 字符串与公开 rule ID 只出现在 Rust。mock 与前端不再复制 `Keep in lockstep with agenthub-core` 的决策常量；传输层错误码对齐可以保留。

### 3.2 mock 是查表机，不是第二套 planner

`pnpm dev:mock` 和 Vitest 需要的是可演示的状态和可编排的内存写入，不是完整协议图。

- 种子账号/供应商继续带 preset 或等价特征（当前 fixture 已满足）。
- `(来源特征, target) → golden.expect`。
- 未知组合 fail-closed 为 `unsupported`，与产品一致。
- `apply` 只解释 expect：`canApply` 为假则拒绝；为真则按 route / ruleId 写入内存 profile 和假 bridge 状态。不重新 classify，不重放 write gate。

页面测试断言「给定一份 plan，界面是否听从」，例如 `canApply: false` 时动作不可用。路线本身对不对只在 Rust 测试。

### 3.3 传输层不养第二套领域模型

Tauri command 的 wire 使用 core 的 serde 形状。`src/lib/backend/contracts` 描述该形状并映射 unavailable / 错误码。与 Rust 平行的前端结构体只保留 UI 格式化；字段真源回 core。

`src/lib/api` 兼容层停止加厚，允许旧调用方继续工作。不要先做一次「四档分类」但不删除任何导出。

### 3.4 验证跟爆炸半径走

继续使用现行最小验证表，不要另造一套命令：

| 改了什么 | 内环 | 不要默认升级为 |
|---|---|---|
| 页面或页面 model | 对应 `vitest run <file>`，必要时 `pnpm typecheck` | 整个 `agenthub-core` 的 `cargo test`、`pnpm test:pr` |
| matrix / planner | `cargo test -p agenthub-core --locked <filter>`，以及 golden 是否过期 | 再手写一套 mock classify |
| wire DTO / port | 该契约测试 + 相应 typecheck | 为了安心编 GUI crate |
| 提交与 CI | 现有 PR CI 全量 | 把 CI 全量搬进每一次本地改动 |

写测试和跑测试拆成两个 Agent，只适合文件集不重叠的并行实现。单文件小改在同一回合跑过滤测试。是否修改 [AGENTS.md](../../AGENTS.md) 的测试 subagent 规则，属于本提案批准后的流程变更，不在未批准前改现行协作红线。

页面抽取 model / hook：仅当不抽就写不了针对性测试，或两处已在复制同一判断时进行。文件大只是调查信号，不是拆分理由。F1 / F2 仍以 [模块化提案](modularity.md) 写明的文件范围为限，不从本文再派生新的页面拆分卡。

### 3.5 编译缓存先于拆 crate

冷 worktree 上的全量 `cargo test -p agenthub-core` 不能代表增量成本。候选顺序：

1. 内容寻址的编译缓存（例如 `sccache`）。不要在 Windows 上让多个 worktree 共享同一份 `target/`：增量缓存并发和文件锁会引入随机构建失败。
2. 内环过滤到具体测试名。
3. 开发机保留一个长命 checkout 给 `pnpm tauri:dev`；Agent worktree 使用缓存，而不是每份自己从零编译 rusqlite。
4. 仅当热缓存上、过滤测试仍经常超过可接受阈值时，才评估拆 crate。

若拆 crate，只切已经独立、依赖向下的叶子（例如本机协议转换或纯 protocol graph），并且 GUI 不得再通过一个 façade crate 把所有包 re-export 回来。按目录或 DDD 命名拆包不在候选范围内。

## 4. 明确拒绝的替代方案

这些做法看起来能加快反馈，但会引入新的真源、工具链或失败模式，不得作为本提案的实施步骤、前置条件或「顺手做」：

| 做法 | 新问题 |
|---|---|
| 按 DDD 或目录把 `agenthub-core` 拆成一堆 crate | 类型被迫公开；改 DTO 要改多个清单；若再 re-export 全家桶，编译图不变 |
| 把 planner 编成 WASM 或 napi 给 Vitest 调用 | 多一条原生工具链；Windows / CI 更慢；mock 不再轻量 |
| 用 specta / ts-rs 一次生成全部 TS 类型 | 生成物入仓则 diff 噪音；不入仓则本地与 CI 必须先 codegen；行为仍可能两套 |
| 让 JSON 成为规则真源 | 丢掉 Rust 类型与 fail-closed 不变量；非法格子可进表 |
| 多个 worktree 共享 Windows `target/` | rustc 增量缓存损坏、文件锁、随机构建失败 |
| 取消 mock，测试一律走 Tauri | Vitest 与 `pnpm dev:mock` 绑死桌面，前端内环更慢 |
| 把每个大页面预拆成 model 当模块化进度 | 行为不变、文件变多；Agent 小改的爆炸半径不降反升 |
| 引入 Pact 或新的契约框架 | 又多一个要对齐的系统；现有 JSON 只缺「由谁写出」 |
| 现在上 sidecar 或按进程拆 Connections / Accounts / Providers | 见 [sidecar 提案](adapter-sidecar.md)；改一次变成跨进程，爆炸半径更大 |

## 5. 候选切片

以下是评估切片，不是排期，也不是当前可执行任务。任一切片派工前都要写明文件范围和回滚方式。C1 补齐 fixture 覆盖可以独立继续，**不要把 C1 扩大成本文**。

### 切片 A：内核生成 golden

用冻结入参调用 `AdapterRouteService::plan()`，序列化到现有 `adapter-capability-contract.json`。Rust 测试在 JSON 与内核输出不一致时失败。现有手写 cases 变成内核快照，不降低覆盖。必须包含 write gate 挡住的边，避免 mock 显示错误的 `canApply`。

### 切片 B：mock 查表优先

mock 的 analyze / plan 先查 golden；命中则返回 expect。未命中暂时走旧 classify（绞杀期）。已知种子必须命中；未命中次数在测试中可见。`dev:mock` 的种子账号行为保持可演示。

### 切片 C：删除第二套引擎

已知种子命中率为 100% 后，删除 mock 的启发式 classify、与 core 锁步的决策常量和 `rule-fixtures` 投影表。留下内存 profile / bridge CRUD。未知来源继续 fail-closed。

### 切片 D：收缩 Vitest 领域套件

`adapter.test.ts` 中断言「这条来源应走哪条 route」的用例迁回或并入 Rust。Vitest 保留：查表、密钥不泄漏、内存 apply、未知 unsupported、界面听从 plan。不得在删除 mock 引擎的同一变更里丢掉 Rust 覆盖。

### 切片 E：环境与流程（可选、可分开批准）

配置 sccache 或等价内容寻址缓存；确认不共享 Windows `target/`。若批准，再把 [AGENTS.md](../../AGENTS.md) / [CONTRIBUTING.md](../../CONTRIBUTING.md) 的测试 subagent 要求与 [测试与验证](../guides/testing-and-validation.md) 的最小矩阵对齐。不在本切片修改产品规则。

### 切片 F：按测量结果考虑拆 crate（可选、最后）

仅在切片 A–D 之后、且热缓存过滤测试仍过慢时打开。先对一个叶子 crate 做依赖与重编测量，再决定是否切。

## 6. 与模块化提案的关系

| [模块化提案](modularity.md) 项 | 关系 |
|---|---|
| 持续约束 2（规则一个真源） | 本文是该约束的落地设计；当前 mock 引擎仍违反它 |
| C1 补齐契约覆盖 | 仍然有效，且应先做或并行做覆盖。C1 完成后不要把 mock 重写成第二套 planner |
| F1 / F2 页面局部抽取 | 范围以模块化提案已写文件为限。本文不授权新的 F 类任务 |
| D1 共享 Backend 契约测试 | 仍是 transport 层设计；不要用它替代内核 golden |
| D2 稳定 wire DTO | 与本文 3.3 一致，但须单独设计试点 command，不在本文一次做完 |
| D3 后端 Use Case 拆分 | 仍是 service 职责设计，不是为了加快编译而拆 crate |
| Sidecar | 仍延期。本文不改变进程边界 |

## 7. 晋升门槛

在更新现行架构/测试文档并把任务标成可执行之前，必须同时具备：

- 具名负责人，以及与 C1 不重叠的文件范围（若 C1 未完成，golden 生成不得毁掉现有覆盖）。
- 切片 A 的失败行为：内核变了而 JSON 未更新时测试失败；JSON 被手改而不等于 `plan()` 时也失败。
- mock 查表后，`dev:mock` 连接演示与现有 adapter Vitest 仍 fail-closed，且不泄漏凭据占位值。
- 不把 matrix 单独当作 `canApply` 真源。
- 回滚方式：只回滚投影（JSON / mock 查表），不改产品规则。

任一切片合入后，再更新 [测试参考](../reference/testing.md)、[前端与 Backend 边界](../architecture/frontend-backend.md) 或 [Route 兼容性](../reference/route-compatibility.md) 中被实现改变的句子。未实现前，那些页面保持当前描述。

## 8. 非目标

本文不包含：全目录重写、微服务、动态插件 ABI、凭据落盘加密、国产 OAuth 开边或 OAuth 转 API、把 Connections / Accounts / Providers 拆到其他进程、以及 [sidecar 提案](adapter-sidecar.md) 中的运行时进程迁移。这些不得写入本提案的里程碑、前置条件、风险或后续任务。
