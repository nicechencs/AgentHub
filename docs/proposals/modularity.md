---
title: 模块化与边界收紧
type: proposal
status: proposed
owner: maintainers
updated: 2026-08-25
---

# 模块化与边界收紧

> 状态：提案
>
> 本文记录 AgentHub 当前已经建立的模块边界，以及下一步可以安全推进的最小改造。它不是完整待办，也不授权大范围目录重写。除本文明确列出的任务外，不得自行扩大实施范围。

## 1. 如何阅读本文

每个项目使用一种状态：

| 状态 | 含义 |
|---|---|
| `已建立` | 代码和测试已经实现，后续只需保持。 |
| `持续约束` | 边界已经存在，需要防止回退，不是新的重构任务。 |
| `可执行` | 范围和验收已明确，可以单独派工。 |
| `先设计` | 只能先调查和设计，暂时不能交给开发 Agent 直接改代码。 |
| `延期` | 条件尚未满足，不产生当前任务。 |

`可执行` 不等于自动批准。实际派工时仍需指定唯一负责人、文件范围、必须保持的行为、测试和独立审查。

## 2. 当前基线

AgentHub 是模块化单体：GUI 和 CLI 共用 `agenthub-core`。以下边界已经建立：

- Agent 注册表和运行时 Agent Catalog 是 Agent 顺序、能力和安装渠道的产品真源；前端不再维护第二份静态产品列表。
- 产品写入统一走 `plan` / `bind` / `unbind`。旧 adapter apply 只保留兼容用途，页面不得调用。
- `TicketBinding` 与 `ActiveBinding` 含义不同。`ConnectionService` 负责 active/current 真相；`is_current` 只是兼容镜像。
- `adapter_control` 已有与 Tauri 解耦的契约，当前由进程内 `DesktopAdapterControl` 托管。`local_bridge` 的监听、回滚和生命周期仍在桌面进程中。
- 前端已有 backend contracts、Tauri adapter、browser mock、运行时组合层和兼容 façade。
- `lib/backend/tauri/invoke.ts` 是前端唯一允许导入 Tauri core 并调用 `invoke` 的文件。生产构建不加载 mock；非 Tauri 的生产页面必须明确返回 unavailable。
- Chat 已拆成页面编排、model/format、hook 和组件，但大 hook 仍是热点。
- Skills 库筛选/勾选在页面内 `skills-library-model.ts`；Projects 列表可见性与多选在 `projects-list-model.ts`。两页仍由 `index.tsx` 组合 API、toast 和预览壳。
- Rust route 测试和 browser mock 共用 `adapter-capability-contract.json`。该 JSON 的 `expect` 由 `AdapterRouteService::plan()` 投影生成；公开非空 rule ID 与 fixture 集合必须一致，漂移时测试失败。classify 不会发出的 sibling ID 以 `documentedRuleId` 进入集合，并断言不是 winner。关闭、仅预览和 write gate 挡住的边有拒绝用例。mock analyze / plan 只解释 golden；apply 只解释 plan。route 产品正确性在 Rust；Vitest 覆盖查表、脱敏、内存 apply 和页面听从 plan。

当前主要问题是：transport 契约测试分散、wire 边界不统一、部分 service/page/hook 职责过多、兼容 façade 仍偏宽。

## 3. 持续约束

以下内容适用于所有改动，不应单独立项：

1. 保持模块化单体，不为命名而引入微服务、DDD/CQRS 套件、事件总线或动态插件 ABI。
2. 每条产品规则只能有一个真源，并由契约测试保护。mock 和页面不得维护第二份规则表。
3. 领域逻辑归 core；Tauri 和 CLI 只做入口、传输和展示。
4. 产品写入继续走 `plan` / `bind` / `unbind`。删除兼容入口前，必须先迁移调用方和测试。
5. 代码和文档必须明确区分 `TicketBinding`、`ActiveBinding` 与兼容 `is_current`。
6. 前端调用路径以 [前后端边界](../architecture/frontend-backend.md) 为准：

   ```text
   页面
     → lib/api 兼容 façade 或 backend port
     → app/runtime
     → #backend 构建别名
     → Tauri adapter 或 browser mock
   ```

   桌面端继续走：

   ```text
   lib/backend/tauri/<port>.ts
     → lib/backend/tauri/invoke.ts
     → Tauri command
     → agenthub-core service
   ```

7. 页面只做编排。纯判断和功能副作用优先放到本功能的 model/format/hook，再考虑共享层。
8. 优先提取小而可测的边界。文件大只是调查信号，不是拆分理由。

## 4. 已建立的最小改造

C1、F1、F2 已落地。当前没有新的可执行任务；下一步仍须先设计（§5）。

### C1：Adapter 规则契约 — `已建立`

- **负责人：** capability 与 route 规则。
- **文件范围：**
  - `crates/agenthub-core/src/services/adapter_route_service/tests.rs`
  - `src/dev/mocks/fixtures/adapter-capability-contract.json`
  - `src/dev/mocks/adapter.test.ts`
- **已落地：** 公开非空 rule ID 进入共享 fixture；`shared_capability_contract_rule_ids_match_matrix` 在集合漂移时失败。Rust 与 mock 读取同一 fixture，并校验真实 classify/plan 的 route、support、rule ID、gate kind、`canApply`、reason、reuse path 和 apply path。JSON 的 `expect` 由 `plan()` 生成；内核变化或手改 golden 都会失败。`openai-api-to-grok-v1` 与 `codex-subscription-to-claude-app-server-v0` 以 `documentedRuleId` 进入集合（classify 赢家是 sibling），关闭、仅预览和 write gate 挡住的边仍被拒绝。
- **必须保持：** matrix 的 `can_apply` 不是唯一写入条件；私有 write gate、bind 实现、host-only bridge 路径和 fail-closed 行为继续有效。fixture 与报错信息不包含凭据值。
- **保持测试：**

  ```text
  cargo test -p agenthub-core --locked shared_capability_contract
  cargo test -p agenthub-core --locked open_matrix_cells_have_bind_and_apply_arms
  cargo test -p agenthub-core --locked closed_or_preview_cells_fail_apply_request_supported
  pnpm exec vitest run src/dev/mocks/adapter.test.ts
  ```

C1 只补齐覆盖并让漂移失败。JSON 已由 `plan()` 生成；切片 C 已删除 mock 第二套引擎。收缩 Vitest 领域套件见 [单一内核与查表投影](single-kernel-projections.md) 切片 D。

### F1：Skills 页面局部编排 — `已建立`

- **负责人：** Skills 前端功能。
- **文件范围：** `src/pages/skills/**`。
- **已落地：** `skills-library-model.ts` 承担库筛选计数、过滤和共享行勾选；`index.tsx` 组合这些函数。预览壳、安装、toast 和 backend port 仍在页面。
- **必须保持：** backend port、共享 Skill 归属、私有来源语义、清理行为、文案和视觉结果不变。
- **禁止：** 新增全局 store；把业务逻辑放进通用 `utils`；修改 backend contract；同时重写 Skills 领域。

### F2：Projects 页面局部编排 — `已建立`

- **负责人：** Projects 前端功能。
- **文件范围：** `src/pages/projects/**`。
- **已落地：** `projects-list-model.ts` 承担列表可见性、会话过滤和多选；`index.tsx` 组合这些函数。session 延迟加载、隐藏项目、删除确认、Chat 跳转、恢复/复制、Agent 能力判断和 URL 同步仍在页面。
- **必须保持：** 上述页面行为与现有 project/session API 不变。
- **禁止：** 把项目真相移入前端 store；新增 Agent 硬编码分支；修改 core/Tauri project contract。

F1 / F2 以本节已写文件范围为限。不要因为某个页面文件较大，就从本文再派生新的抽取卡；见持续约束 8 与 [单一内核与查表投影](single-kernel-projections.md)。

## 5. 需要先设计的工作

以下方向合理，但范围过大或条件不足。只能先派调查/规划任务，不能直接派开发任务。

### D1：共享 Backend 契约测试 — `先设计`

- **负责人：** 前端 backend-contract 层。
- **设计产物：** port 清单、通用测试用例接口、可注入的 Tauri invoke/event transport、错误与 unavailable 语义、首个试点 port。
- **覆盖范围：** 成功映射、结构化错误、unsupported/unavailable、写入后的刷新、event/channel 清理。
- **限制：** mock 与 Tauri 实现可以不同，但必须满足同一 port 契约；测试设施不得进入生产模块图。
- **进入开发的条件：** 试点 port 已明确文件、测试、回滚边界和双实现验证命令。

### D2：稳定 wire DTO — `先设计`

- **负责人：** Tauri transport 边界；每次只处理一个领域。
- **设计产物：** 直接序列化 core model 的 Tauri command 清单、对应的前端 mapper、兼容调用方和首个试点领域。
- **限制：** Tauri 不得维护第二套领域模型；wire DTO 只描述传输结构，领域规则仍在 core。
- **进入开发的条件：** 试点 command、DTO、mapper、调用方、兼容范围和契约测试全部明确。

### D3：后端 Use Case 边界 — `先设计`

Provider、Account、Backup 必须分别规划，不能合并成一个重构任务。

| 单元 | 当前问题 | 必须保持 |
|---|---|---|
| Provider | CRUD、live config、switch saga、snapshot/compensation 和兼容入口集中在同一 service | `ConnectionService` 的 current 所有权；live write 前备份；按 Agent 加锁；失败回滚；兼容调用方 |
| Account | Pool CRUD、授权身份、live reconcile、切换和补偿跨多个子模块 | 授权身份语义；current 一致性；锁/revision 保护；能力不足时 fail closed |
| Backup | snapshot、索引、restore/delete、manifest/path 校验和 live-write authority 集中在同一 service | 路径与软链接安全；restore 前快照；live-write authority；数据库与文件系统补偿 |

- **每个单元的设计产物：** 调用方与影响范围、主要职责、建议边界、保持不变的公开 façade、事务/锁边界、测试和回滚方案。
- **进入开发的条件：** 找到一个不改行为、可独立测试且文件范围不重叠的最小提取项。

### D4：兼容 façade 分类 — `先设计`

- **负责人：** 前端 API/runtime 边界。
- **设计产物：** 将 `src/lib/api` 的导出分为纯转发、DTO 映射、允许的 runtime/cache 协调、废弃或仅内部使用四类。
- **限制：** 分类阶段不删除导出，现有页面写入和刷新行为必须保持。
- **进入开发的条件：** 每次只处理一个 façade，并提供调用方清单和删除/迁移条件。

### D5：收窄 Agent contribution façade — `先设计`

- **负责人：** Agent 集成平台。
- **设计产物：** 找出 `AgentAdapter` 中仅为兼容保留的方法、已归 catalog/config/lifecycle/run port 的能力，以及阻止迁移的调用方。
- **限制：** registry 仍有基于 `AgentId` 的封闭兼容路径，不得声称已经具备动态插件 ABI，也不得改变 Agent 行为。
- **进入开发的条件：** 某一组方法可以迁入现有 port，且调用方和契约测试完整。

### D6：单一内核与查表投影 — `已关闭`

- **负责人：** capability / route 规则与 browser mock。
- **设计产物：** [单一内核与查表投影](single-kernel-projections.md)。
- **当前进度：** 切片 0、A、B、C、D 已落地。切片 E 已评估：不落地 sccache。切片 F 已评估：不拆 crate。本项不再派工。
- **限制：** 不得用矩阵格子单独当 `canApply`；不得引入 WASM、napi、类型生成框架或共享 Windows `target/`。
- **进入开发的条件：** 已满足且结论为不实施编译环境改动与拆包。不要从本项再派生开发任务。

## 6. 延期事项

### Sidecar 与统一控制客户端 — `延期`

`local_bridge` 当前仍在桌面进程内运行。改变进程边界只能依据 [Adapter sidecar 提案](adapter-sidecar.md)。

不得从本文生成 sidecar、IPC、schema lease、升级、恢复或 GUI/CLI control client 的当前实现任务。只有 sidecar 提案中的控制契约、schema、升级、恢复和运行时门槛都有证据后，才可重新评估。

## 7. 责任边界

| 事项 | 主要负责人 | 不得变成 |
|---|---|---|
| Agent 差异 | `integrations/agents/<key>/` contribution | 页面中的 Agent 分支表 |
| capability 与 route 决策 | `domain/protocol_graph/` 与 `AdapterRouteService::plan()` | mock 或 UI 的第二份规则表；落地设计见 [单一内核与查表投影](single-kernel-projections.md) |
| 产品写入 | `plan` / `bind` / `unbind` Use Case | 页面直接调用兼容 apply |
| active/current 真相 | `ConnectionService` | Account/Provider 各自维护的 best-effort 状态 |
| 本机 route runtime | 当前进程内 control host；未来仅在正式批准后迁移 sidecar | 凭据仓库或直接 SQL writer |
| UI transport | backend contracts、runtime 与 adapters | 页面调用 `invoke` 或运行时选择 mock |
| 页面纯逻辑 | 功能内的 `*-model` / `*-format` | 跨页面导入、新框架或全局状态层 |

## 8. 每个任务的验收要求

- 移动 symbol 前，先确认调用方、影响范围和公开兼容面。
- 每个任务指定一个主要负责人和明确文件范围；并行任务不得修改同一文件。
- 拆 service 或 effect hook 前，先写清必须保持的行为、事务和锁语义。
- 删除重复或兼容路径前，先补契约测试。
- 测试代码与生产代码分文件。
- 先跑针对性前端/Rust 测试；边界变化较大时再跑 typecheck/build。
- 代码和测试确定新 owner 后，再更新现行文档。
- 不把归档中的历史方案重新变成当前任务。

## 9. 非目标

本提案不包含：全目录重写、微服务拆分、将 Connections/Accounts/Providers 拆成独立进程、凭据落盘加密、国产 OAuth Adapter、OAuth 转 API。这些内容不得出现在本提案的实施里程碑、前置条件、风险或后续任务中。
