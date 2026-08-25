---
title: Adapter 单一内核与查表投影（历史提案）
type: archive
status: historical
owner: maintainers
updated: 2026-08-25
---

# Adapter 单一内核与查表投影

> **Archived / 已归档**: Historical proposal, completed implementation record, and dated evaluation. Do not use as a current implementation contract or TODO list.
> **Status**: historical
>
> **归档（2026-08-25）**。切片 0–D 已落地；切片 E 已评估且不落地 sccache；切片 F 已评估且不拆 crate。现行契约见 [Adapter 路线内核](../architecture/adapter-route-kernel.md)。下文保留实施切片、历史基准和 E/F 测量，其中的测试数量与墙钟时间是当次快照，不是当前固定规模。不授权把 JSON 当规则真源，也不再从本文派工。

## 0. 结论与适用范围

长期方向由两层互补决策组成：

1. **架构层：单一内核 + 薄投影。** `AdapterRouteService::plan()` 是 Adapter / route 产品规则的唯一决策者；Tauri、mock 和契约 JSON 只传输、解释或投影内核结果。
2. **研发内环：风险分级 + 最小验证。** 单文件、局部 UI 和纯函数改动不默认启动多 Agent 或全量门禁；跨层 contract、Rust 核心规则和持久化变更才升级到完整规划、审查与验证。

两层不能互相替代：只精简工作流会保留 core / mock 双规则引擎；只改成单一内核，也无法解释或消除不涉及 Adapter 的页面小改所承担的 Agent 启动、重复探索和全量验证成本。

本文的架构方案只适用于已经确认存在重复决策的 Adapter / route 子系统。不得把“单一内核”推广成所有 UI 状态都进入 Rust，也不得因为两个实现形状相似就建立新的全局内核。普通展示、页面瞬时状态和 mock 内存 CRUD 可以继续留在前端。

当时的实施顺序是：先批准并对齐研发内环，再按 A → B → C → D 消除第二套规则引擎，最后依据热缓存测量决定是否增加编译缓存或拆叶子 crate。A–D 已完成；E/F 的测量结论是不落地。

## 1. 当前基线

AgentHub 仍是模块化单体：GUI 和 CLI 共用 `agenthub-core`。产品写入走 `plan` / `bind` / `unbind`。前端路径仍是页面 → runtime → `#backend` → Tauri 或 browser mock；生产构建不加载 mock。

规则真源在 Rust：`adapter_capability_matrix` 加上 `AdapterRouteService` 的私有 write gate。`can_apply` 只是矩阵层标志；实际写入还要过 write gate、来源凭据和目标 writer。

浏览器 mock 的 analyze / plan 只读取 golden.expect；未命中 fail-closed 为 `unsupported`。`src/dev/mocks/fixtures/adapter-capability-contract.json` 是 `AdapterRouteService::plan()` 对冻结入参的只读快照；Rust 测试在快照与内核输出不一致时失败。C1 的公开 rule ID 覆盖仍然有效。apply 只解释 plan 写入内存 profile / 假 bridge，不再 classify，也不重放 write gate。

验证方面，[测试与验证](../guides/testing-and-validation.md) 已按改动类型给出最小命令，并与 [AGENTS.md](../../AGENTS.md) 使用同一套风险分级：局部改动跑过滤测试，全量门禁留给提交前或 CI。PR CI 仍跑全量 typecheck、Vitest 和三个 Rust crate。Grok / Agent worktree 通常没有共享的 `target/` 编译产物；crate.io 与 pnpm store 是全局的，Rust 增量缓存不是。

`agenthub-core` 仍是单一 crate。把 core 拆成多个 Cargo 包、把 planner 编成 WASM/napi、或引入类型生成框架，都不是当前实现。

### 1.1 2026-08-25 实测基线（历史快照）

以下数据来自同一现有工作区的热缓存只读基准，用于区分“工具执行时间”和“Agent 工作流时间”。它是 2026-08-25 的一次性快照，不是跨机器性能承诺，也不是当前测试规模的固定值：

| 检查 | 当前规模 | 墙钟时间 |
|---|---:|---:|
| 单个页面 model Vitest | 1 文件、16 tests | 约 2.1 秒 |
| `pnpm typecheck` | 全应用 TypeScript | 约 3.6 秒 |
| `pnpm test` | 182 文件、1,643 tests | 约 15.7–20.4 秒 |
| `cargo test -p agenthub-core --locked --no-run` | 热缓存 | 约 0.44 秒 |
| `cargo test -p agenthub-core --locked` | 1,792 tests | 约 30.5 秒 |

历史 subagent 元数据显示，8 次运行累计约 2.59 小时，平均约 19.4 分钟；其中 3 次失败或超时。一个成功任务总耗时约 6.7 分钟，而验证约 2.5 秒。该样本不能代表每次任务，但足以说明：对普通小改，Agent 启动、上下文交接、重复调查和等待可能远高于编译或定向测试本身。

基准当次还发现两个会触发重复诊断的问题：`pnpm typecheck:test` 遇到一个既有类型错误；一个依赖运行环境探测的 Rust 测试首次失败、立即重跑通过。它们不作为本提案的长期基线，应作为独立缺陷处理，不能通过扩大每次本地验证范围掩盖。

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

需要区分两个层级的问题：

- **全项目开发耗时：** 主要固定成本来自任务被过早升级为多 Agent 流程、重复传递上下文，以及把提交前或 CI 的全量门禁搬进每次本地改动。它影响页面、文档、纯函数和跨层功能，不由本提案单独解决。
- **Adapter / route 结构成本：** 提案撰写时的主要原因是**同一条产品规则被决定两次**：core 决定一次，mock 再决定一次。JSON 当时已是内核快照而不再是第三份手写 expect；切片 C 之后 mock 不再是第二台引擎，只查 golden。

第二个问题使一条 route 或一个 plan 字段的改动文件数远大于行为增量；第一个问题又把这个较大的改动面交给多个 Agent 重复探索和验证。两者叠加后，耗时与 diff 不成比例。

页面里把筛选/多选抽成 `*-model` 可以让纯函数可测，但若作为常设搬运任务，会在行为不变时固定增加文件、测试和 Agent 回合。这不是 C1 的替代，也不应从「文件较大」自动派生。

## 3. 长期架构目标

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

验证和 Agent 协作都按风险升级：

| 风险级别 | 典型改动 | 默认执行方式 | 最小验证 |
|---|---|---|---|
| 局部 | 文案、样式、单页面状态、纯函数，且不改共享 contract | 主 Agent 在同一回合完成 | 对应 Vitest；必要时 `pnpm typecheck` |
| 模块 | 单个功能目录内的逻辑，不改 Rust / wire / 持久化 | 主 Agent 或一个实现 Agent | 相关测试 + `pnpm typecheck` |
| 跨层 | backend port、wire DTO、Tauri command、共享 service | 明确范围后再用实现与独立审查角色 | contract test + 对应 typecheck / Cargo filter |
| 高风险 | 数据迁移、写入补偿、锁、安全边界、发布 | 完整规划、实现、审查与全量门禁 | 提交前矩阵和 CI 全量 |

写测试和跑测试拆成两个 Agent，只适合确实可以并行、文件集不重叠且等待时间足以覆盖 Agent 启动成本的任务。单文件小改在同一回合跑过滤测试。reviewer 只检查最终 diff、受影响调用方和对应验证，不重新进行全仓调查。

切片 0 已把这套风险分级写入 [AGENTS.md](../../AGENTS.md)、[CONTRIBUTING.md](../../CONTRIBUTING.md) 与 [测试与验证](../guides/testing-and-validation.md)。后续切片不要再发明第二套协作规则。

页面抽取 model / hook：仅当不抽就写不了针对性测试，或两处已在复制同一判断时进行。文件大只是调查信号，不是拆分理由。F1 / F2 仍以 [模块化提案](../proposals/modularity.md) 写明的文件范围为限，不从本文再派生新的页面拆分卡。

### 3.5 编译缓存先于拆 crate

冷 worktree 上的全量 `cargo test -p agenthub-core` 不能代表增量成本。候选顺序：

1. 内容寻址的编译缓存（例如 `sccache`）。不要在 Windows 上让多个 worktree 共享同一份 `target/`：增量缓存并发和文件锁会引入随机构建失败。
2. 内环过滤到具体测试名。
3. 开发机保留一个长命 checkout 给 `pnpm tauri:dev`；Agent worktree 使用缓存，而不是每份自己从零编译 rusqlite。
4. 仅当热缓存上、过滤测试仍经常超过可接受阈值时，才评估拆 crate。

若拆 crate，只切已经独立、依赖向下的叶子（例如本机协议转换或纯 protocol graph），并且 GUI 不得再通过一个 façade crate 把所有包 re-export 回来。按目录或 DDD 命名拆包不在候选范围内。

### 3.6 工作流优化先于结构重写

工作流分级成本低、覆盖全项目，应先于 A–D 实施；但它不是跳过架构治理的理由。建议先建立以下停止条件：

- CodeGraph 已在 1–2 次查询内给出调用链和影响面时，不再用 grep / 重读重复证明同一结构事实。
- 只有至少两个真正独立、各自有明确文件范围的任务才并行多个 Agent。
- 测试 Agent 接收已确定的命令，只执行和回报原始结果，不重新探索需求与架构。
- 全量 `pnpm test`、完整 Rust crate 矩阵和生产 build 默认留给提交前或 CI；边界和发布相关改动除外。
- reviewer 从最终 diff 开始，仅在发现具体风险时扩大读取范围。

这些规则应写入协作治理文档，而不是由本架构提案长期充当流程真源。

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
| 现在上 sidecar 或按进程拆 Connections / Accounts / Providers | 见 [sidecar 提案](../proposals/adapter-sidecar.md)；改一次变成跨进程，爆炸半径更大 |

## 5. 候选切片与顺序

以下是评估切片，不是排期，也不是当前可执行任务。任一切片派工前都要写明文件范围和回滚方式。C1 补齐 fixture 覆盖可以独立继续，**不要把 C1 扩大成本文**。

### 切片 0：研发内环对齐 — `已落地`

[AGENTS.md](../../AGENTS.md)、[CONTRIBUTING.md](../../CONTRIBUTING.md) 与 [测试与验证](../guides/testing-and-validation.md) 已使用同一套风险分级。局部改动允许主 Agent 在同一回合完成实现与过滤测试；跨层和高风险改动仍保留独立审查与完整验证。

该切片不修改产品规则、mock 或 Rust。

### 切片 A：内核生成 golden — `已落地`

用冻结入参调用 `AdapterRouteService::plan()`，序列化到现有 `adapter-capability-contract.json`。Rust 测试在 JSON 与内核输出不一致时失败。现有手写 cases 变成内核快照，不降低覆盖。必须包含 write gate 挡住的边，避免 mock 显示错误的 `canApply`。内核输出变化后，用 `UPDATE_ADAPTER_CAPABILITY_CONTRACT=1` 重新生成快照。

### 切片 B：mock 查表优先 — `已落地`

mock 的 analyze / plan 先查 golden；命中则用 expect 覆盖 route / support / ruleId / gateKind / canApply / reason / reusePath。切片 B 的绞杀期曾允许未命中回退旧 classify；切片 C 已删除该回退，未命中 fail-closed 为 `unsupported`。已知种子必须命中；未命中次数由 `getGoldenLookupStats()` 统计，测试可断言。`dev:mock` 的种子账号行为保持可演示。

### 切片 C：删除第二套引擎 — `已落地`

已知种子命中率为 100% 后，已删除 mock 的启发式 classify、与 core 锁步的决策常量和 `rule-fixtures` 投影表。留下内存 profile / bridge CRUD。未知来源继续 fail-closed。

### 切片 D：收缩 Vitest 领域套件 — `已落地`

`adapter.test.ts` 中断言「这条来源应走哪条 route」的用例已迁回或并入 Rust。Vitest 保留：查表、密钥不泄漏、内存 apply、未知 unsupported、界面听从 plan。不得在删除 mock 引擎的同一变更里丢掉 Rust 覆盖。

### 切片 E：编译环境 — `已评估，不落地`

2026-08-25 在 Windows、rustc 1.89.0、全局 `~/.cargo` registry 约 173 MB、本机未安装 sccache 的条件下，对 `cargo test -p agenthub-core --locked adapter_route_service` 做了冷/热拆分。两个 worktree 使用各自的 `target/`，并行 `--no-run` 无文件锁。CI 已用 `Swatinem/rust-cache`。

| 场景 | 下载 | 编译+链接 (`--no-run`) | 测试执行 | 墙钟合计 |
|---|---:|---:|---:|---:|
| 当前 checkout，热 `target/` | 0（`--offline`） | 0.55–0.68 s | 2.61 s（39 tests） | 约 3.2 s |
| 新 Agent worktree，空 `target/` | 5.9 s（补缺 crate） | 41.5 s | 2.80 s（随后热跑） | 首次约 50 s |
| 该新 worktree 第二次 `--no-run` | 0 | 0.49–0.65 s | 2.80 s | 约 3.4 s |
| 两 worktree 并行热 `--no-run` | 0 | 各约 0.8 s | — | 墙钟 1.1 s，无锁 |

结论：热内环的主要时间是测试执行，不是编译。冷 worktree 第一次会花约 42 s 编依赖，但内容寻址缓存需要安装 `sccache`（系统级工具，本切片未获安装授权），且仓库强制 `RUSTC_WRAPPER` 会让未安装机器和当前 CI 失败。因此不修改 Cargo、CI 或发布配置。若以后要做，应是开发机可选包装，且 Windows 仍禁止共享 `target/`。

回滚：无配置可回滚。测量用的独立 worktree 已删除。

### 切片 F：按测量结果考虑拆 crate — `已评估，不拆`

2026-08-25 在切片 A–D 完成、已用测试名过滤、且 E 已评估不落地 sccache 的前提下，复测热缓存并调查叶子模块。

热缓存、无改文件时：`cargo test -p agenthub-core --locked --offline --no-run adapter_route_service` 约 0.47 s；过滤跑 39 个 `adapter_route_service` 测试约 3.5 s；精确一名 golden 测试约 2.3 s；`domain::protocol_graph` 30 个测试约 0.62 s；`bridge::protocol` 73 个测试约 0.60 s。只改 mtime 后的增量 `--no-run` 约 7.2–7.4 s，仍是整颗 core 测试二进制的增量编译+链接。

叶子结论：`protocol_graph` 依赖 `models` 中的 `AgentId` / `TicketSurface` / `AdapterRoute` 等类型，且已被 `models/mod.rs` `pub use` 全量转口，不是向下叶子。最接近的叶子是 `bridge::protocol` + `bridge::types`（无 GUI/Tauri/ rusqlite），但 GUI/CLI 仍依赖整颗 `agenthub-core`，core 的 host 仍会依赖该叶子；改 rlib 仍带动 core。热缓存过滤测试已够快，不满足「仍过慢才拆」的开门条件。

因此不拆 crate，也不派试点。以后只有热缓存过滤测试经常明显慢于当前 3–4 秒时才重新评估，且 GUI 不得通过 façade 全量 re-export。Windows 仍禁止共享 `target/`。

## 6. 与模块化提案的关系

| [模块化提案](../proposals/modularity.md) 项 | 关系 |
|---|---|
| 持续约束 2（规则一个真源） | 本文是该约束的落地设计；切片 C 之后 mock 不再维护第二份规则表 |
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

切片 A–D 完成的量化验收是：

- 已知演示种子的 golden 命中率为 100%；未知组合仍稳定返回 `unsupported`。
- route、support、公开 rule ID、gate kind、`canApply`、reason 和 apply path 的产品决策只在 Rust 内核产生；mock 不保留启发式 classify 或手写第二份决策常量。
- 新增或修改一条 route 规则时，不需要手工同步 mock 决策分支或在 Vitest 再写一套“应走哪条 route”的领域断言。
- 页面测试只验证“给定 plan 后 UI 是否服从”，Rust 测试验证 plan 本身是否正确。
- 定向验证继续使用第 3.4 节矩阵；没有边界变化时，不因该提案默认升级到全量 GUI build 或全部 Rust crate。

切片 0 的验收是：局部改动、跨层改动和高风险改动在协作治理文档中有唯一且一致的分级；不存在“一方面要求最小验证、另一方面又要求每次另起 Agent 跑完整检查”的冲突。

任一切片合入后，再更新 [测试参考](../reference/testing.md)、[前端与 Backend 边界](../architecture/frontend-backend.md) 或 [Route 兼容性](../reference/route-compatibility.md) 中被实现改变的句子。未实现前，那些页面保持当前描述。

## 8. 非目标

本文不包含：全目录重写、微服务、动态插件 ABI、凭据落盘加密、国产 OAuth 开边或 OAuth 转 API、把 Connections / Accounts / Providers 拆到其他进程、以及 [sidecar 提案](../proposals/adapter-sidecar.md) 中的运行时进程迁移。这些不得写入本提案的里程碑、前置条件、风险或后续任务。
