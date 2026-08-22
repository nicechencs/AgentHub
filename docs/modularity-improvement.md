# AgentHub 模块化审查与改进方案

> **现行状态（2026-08-19）**：sidecar（`agenthub-adapterd`）仍是目标、未迁。官方船经 `release` 三文件 bump。
> 状态：稳定改进方案（审查结论 + 分阶段收口）。不替代现有真源。
> 审查日期：2026-08-15
> 2026-08-16 回写进度（对照当前工作区，不是再审查一遍）。
> 方法：主 Agent 汇总目录/体量/依赖证据；5 个 Grok Agent 分域深读（adapters+platform、services+models、前端 backend、页面组件、Tauri/CLI/Bridge）。
> 适用范围：模块化单体。不引入微服务、DDD、CQRS、事件总线、动态插件 ABI。凭据落盘加密为**项目范围外**。国产 OAuth 适配 / 转 API 为**产品不做**。

相关真源：

| 主题 | 真源 |
|---|---|
| 目录与分层 | [architecture.md](architecture.md) |
| 平台能力改造（已收口） | [platform-capability-refactor.md](platform-capability-refactor.md) / [platform-capability-remediation.md](platform-capability-remediation.md) |
| 票 / 绑定 / 协议图 | [connection-binding-model.md](connection-binding-model.md) / [product-decisions.md](product-decisions.md) |
| Sidecar 进程边界 | [adapter-sidecar-design.md](adapter-sidecar-design.md) |
| 实现状态 | [agenthub-plan.md §8](agenthub-plan.md#8-当前实现状态以代码与测试为准) |
| 新增 Agent | [adding-an-agent.md](adding-an-agent.md) |

---

## 1. 结论先行

AgentHub **不需要推倒重来**。三 crate 边界、`core` 无 Tauri、前端 `invoke` 单点、平台 registry + `AgentKey` 开闭测试轨，都已经立住。

当前模块化的主矛盾不是「缺分层」。下列目标**已经落地**，不能再写成「尚未成为生产组合」：

- `integrations/agents/<key>/` **已落地**（八家生产 Agent + test-only `demo_agent`）
- Ticket `bind` / `unbind` **已是产品写口**（ConnectFlow 走 `bindTicket`；`applyAdapter` 已 `@deprecated`）
- `adapter_control` **契约已落地**（`agenthub-core` + `DesktopAdapterControl` in-process host）

仍未成为生产组合的是：**删掉 `AgentAdapter` 厚表面、sidecar 二进制、CLI adapter 对称**。生产路径仍以胖 `AgentAdapter` 为过渡枢纽；`local_bridge` 仍必须走 desktop host saga。

一句话：

```text
骨架是模块化单体；integrations / Ticket 写口 / adapter_control 契约已落地。
下一步最有杠杆的是削掉 AgentAdapter 厚表面、补 sidecar 二进制与 CLI 对称，
而不是再拆 crate、再引入框架、或一次性搬家目录。
```

建议按三层推进：

| 层 | 目标 | 典型动作 |
|---|---|---|
| **P0 止血** | 双真源、依赖倒挂、产品写入分叉 | 收口 install/config、bind 唯一写、修 tauri→api 倒挂、冻结跨页 import |
| **P1 削胖** | 上帝文件按已有模式切开 | 拆 adapters/mod、Account/Adapter*、Chat/Skills/Projects、ports.ts |
| **P2 收口** | 对齐目标布局与进程边界 | 削 `AgentAdapter` 厚表面、sidecar 二进制 / IPC / schema lease、CLI 同一 client |

**明确不做**：微服务、Connections 拆进程、凭据落盘加密、大爆炸目录搬家、为拆而引入 React Query / DDD 战术模式。

---

## 2. 已经正确、必须保住的边界

这些不变量后续改动不得破坏：

1. **`agenthub-core` 不依赖 Tauri**；GUI / CLI 是两种壳。
2. **仅 `src/lib/backend/tauri/` 可 `invoke`**；`pnpm build` 强制 Tauri adapter；mock 不得进生产 module graph。
3. **页面不直接 `invoke`，不用 `isTauriApp()` 选 transport**。护栏：`boundary-imports.test.ts`、`boundary.test.ts`、Vite `generateBundle`。
4. **平台能力按能力分模块**（`platform/{detection,install,config,lifecycle,paths,projects,skills,stream,usage,agent_catalog}`），Agent 只贡献差异。
5. **`ConnectionService` 是 current / `agent_active_bindings` 的唯一事务协调者**。
6. **`local_bridge` 可迁 sidecar；Connections / Account / Provider / live 事务不拆进程**。
7. **test-only `demo-agent`** 证明开闭可行，且不进入生产 registry。

---

## 3. 当前健康度（对照目标）

| 目标 | 现状 | 判定 |
|---|---|---|
| core / gui / cli 三壳 | 已落地 | 健康 |
| 平台 registry + AgentKey | registry 在；生产仍经 `AgentAdapter` 包装 | 半落地 |
| 稀疏端口、新增 Agent 只加一目录 | `integrations/agents/<key>/` **已落地**（八家 + test-only `demo_agent`）；`AgentAdapter` 仍在 `adapters/` 过渡 | 半落地 |
| Ticket + Binding 唯一写入 | 读模型 / `plan` / `bind` 已有；`apply_adapter` 已 deprecated，但 host saga 仍并行（`LocalBridge` 必须走 desktop host） | 半落地 |
| 前端 backend 分层 | transport 干净；contracts 过厚、api 夹映射、页面 VM 交叉 | 骨架健康 |
| 页面 = 编排 + 本地 state | Connections / Routes 接近；Chat 已按 P1-7 拆完（`index.tsx` 约 147 行）；Skills / Projects / AgentCard 仍偏厚 | 不均 |
| GUI/CLI 共用 control contract | `adapter_control` 契约 + Desktop in-process host 已落地；CLI **无** adapter/bridge 对称 | 不对称 |
| sidecar | `adapter_control` + in-process host **已落地**；`agenthub-adapterd` / IPC / schema lease **未开始** | 契约落地 / 二进制未开始 |

### 3.1 体量信号（2026-08-15 快照，多数热点已切开，勿按旧行数派工）

下表是审查当日快照。**已切开的不要再按旧行数派工**：

| 层 | 文件 | 2026-08-16 回写 | 角色 |
|---|---|---|---|
| core | `adapters/mod.rs` | **已收口**（薄 façade，约 52 行） | trait / registry / detect / auth / config_write 已拆出 |
| core | `services/account_service` | **已收口**（按域拆目录；`pool_crud` 于 2026-08-22 再切） | `pool_crud/{query,api_key,create,refresh,merge,compensate,types}` / `live_reconcile` / `switch_saga` / `import_live` / `surface` |
| core | `services/adapter_{apply,route,bridge,secret}` | **已收口**（按域拆目录） | classify / plan / saga / prepare / finalize 等 |
| core | `bridge/host.rs` | **已收口** | 已拆 `host/{lifecycle,http,dispatch}` |
| 前端 | `pages/chat/index.tsx` | **已收口**（约 147 行编排） | `use-chat-page` + 同目录组件 |
| 前端 | `SwitchConfirmDialog` | **已删除** | 无生产引用 |
| core | `platform/config/sources/dsh.rs` | **不存在** | 无 `use crate::adapters`；dsh 配置在 `integrations/agents/dsh/` |

仍偏厚、派工时以**当前工作区行数**为准（不含 `tests.rs`）：

| 层 | 文件 | 角色 |
|---|---|---|
| core | `usage/session_jsonl.rs` | 解析器（隔离合理，体量大） |
| core | `services/project_service/scan.rs` | 项目扫描仍在 services |
| core | `bridge/protocol/responses.rs` | 协议转换 |
| core | `services/account_quota.rs` | 配额附属 |
| core | `adapters/cursor.rs` / `workbuddy.rs` | 安装探测与 auth 仍混杂 |
| tauri | `adapter_bridge_controller.rs` | **壳层仍持有 local_bridge saga**（经 `DesktopAdapterControl`） |
| 前端 | `pages/skills/index.tsx` | 预览 + 写路径仍偏厚 |
| 前端 | `pages/projects/index.tsx` | 树 + mutation 同页 |
| 前端 | `pages/agents/agent-card.tsx` | 生命周期副作用仍集中 |

行数本身不是罪；**同一文件多个变化原因、双真源、依赖倒挂** 才是。`skill_service` / `ports.ts` / mock `adapter.ts` / `ConnectFlowDialog` 已按 P1/P2 切开，勿再按 2026-08-15 旧行数派工。

---

## 4. 主问题（按杠杆）

### 4.1 生产仍是 Adapter-centric，platform 多为包装

`platform/detection` 的 builtin 是 `AdapterDetector` 转发 `AgentAdapter::detect`。  
`platform/skills` 的 target 从 `AdapterRegistry` 派生。  
`platform/lifecycle` 的 executor 丢掉 `InstallContribution`，再走 `install_service` + builtin `AgentId`。  
`platform/projects` 只有 port，实现仍在 `services/project_service/sources.rs`。

`AgentAdapter` 对生产 Agent 仍是厚表面：`detect` / `install_channels` / `read_config` / `write_config` / `read_auth` / `read_account` / `apply_account` / `skills_dir` / `live_backup_paths` / `build_run_spec` / `capability`。这与「只实现支持的稀疏端口」不一致。

**循环依赖（同 crate 可编译，边界已泄漏）**：

```text
adapters → utils/paths → platform/paths
platform/{detection,skills,agent_catalog,lifecycle} → adapters
```

`platform/config/sources/dsh.rs` **已不存在**，不要再把它写成反向 import 证据。`platform/config` 不再 `use crate::adapters`。detection / skills / lifecycle / agent_catalog 仍经 `AdapterRegistry` / `AgentAdapter` 包装。

### 4.2 双真源：Install 与 Config

| 主题 | 轨 A | 轨 B | 风险 |
|---|---|---|---|
| 安装渠道 | `AgentAdapter::install_channels()` | `InstallContribution` + `catalog/install::channels_for` | npm 包名 / 顺序 / 渠道集合漂移 |
| 配置写入 | `ProviderService` → `AgentAdapter::write_config` | `ConfigurationService` → projector | 同一 Agent 两套 apply 语义仍不同（P0-2 剩余）；`platform/config` 反向 import 已消失 |

新增 Agent 的贡献已进 `integrations/agents/<key>/`；胖 `adapters/<id>.rs` 仍是过渡表面，常还要改 `AgentId`、`agent_bind_capability` 与前端过渡类型。理想改动面是只加一个目录 + 一处 `register`，在削完厚表面之前不要宣称已达到。

### 4.3 连接域：过渡别名未收口 + Binding 双义

目标对象是 **Ticket + Binding + 协议图**。存储仍是 `accounts` + `providers` + `adapter_profiles` + `agent_active_bindings`。这可以接受，但对外入口和命名没有收口。

- **写入入口（已收口产品写口）**：页面 / ConnectFlow 只走 `planTicket` / `bindTicket` / `unbindTicket`；`applyAdapter` 已 `@deprecated`。`LocalBridge` 仍必须走 desktop host saga（`TicketBindService::bind` 对 `LocalBridge` 直接拒绝，由 `DesktopAdapterControl` 转 `apply_local_bridge`）。
- **Binding 双义（P0-5 已收口注释）**：`TicketBinding`（票→Agent 路线）vs `ConnectionService::ActiveBinding`（Agent→当前行指针）。同词不同物；`ConnectionService` 文件头已写明。
- **规划真理三处（P2-4 已正名目录）**：`domain/protocol_graph/` 的 `decide_adapter_capability`、`AdapterRouteService` 的 `write_gate` / `actions_for`、`AdapterApplyService::apply` 再 match。plan 可点、apply 失败（或反之）的漂移面仍在（P0-4）。
- **`models/`**：规划表已迁 `domain/protocol_graph/`；`models/mod.rs` 只 re-export 兼容路径，不再宣称「纯数据」。
- **装配重复（P1-5 已收口注入口）**：`TicketBindService::from_parts` / `AdapterApplyService::from_parts` 注入 hub 已有实例；`new()` 仍会 `with_live`（测试 / 兼容构造）。

### 4.4 壳层持有本不该有的业务真相

`src-tauri/src/lib.rs` 写明 *Business logic stays in core*，但：

- `adapter_bridge_controller.rs`（1028）持有完整 `local_bridge` saga 与 **target-agent 进程内锁**。
- `commands/adapter.rs`（508）持有 bind/unbind 路由与 wallet `bridge.running` 富化。
- `provider` / `account` / `oauth` / `configuration` / `backup` / `install` 即使不做桥，也抢 `bridge_saga_coordinator.lock_target`。CLI **没有**对应门禁 → GUI/CLI 并发改同一 Agent live 时，壳层串行只对 GUI 有效。
- Skill market 搜索/安装（P0-8 已收口）：GUI / CLI 均走 `AgentHub::search_skill_market` / `install_market_listing`。
- `adapter_control` **契约已落地**（core + `DesktopAdapterControl` in-process）。`agenthub-adapterd/`、IPC client、`DataStoreBootstrap` / `SchemaGenerationLease` **未开始**。没有 sidecar 二进制，无法把 controller 从 Tauri 进程撕下。

`AgentHub` 门面挂 20+ 服务字段。对桌面单体可接受；对 sidecar 组合与双客户端一致性，缺 **use-case 门面**（尤其 Adapter / local_bridge）。

### 4.5 前端：骨架对、依赖网交叉

**已落地**：`#backend` 按命令装配；页面业务调用走 `@/lib/api/*`；pages/components 零 `invoke` / `isTauriApp`。

**债（对照 2026-08-16 工作区）**：

1. **依赖倒挂（P0-6 已收口）**：`src/lib/backend/tauri/{agent,doctor}.ts` 已改 import `contracts`（`agent-connection` / `doctor-map`）。
2. **跨页耦合（P0-7 已收口）**：`src/lib/bridges-path.ts` 已被 Sidebar / Settings / Dashboard / App 使用；布局不再 import `pages/bridges/*`。
3. **`ports.ts`（P1-6 已收口）**：自身只 re-export `Backend`（约 78 行）；`TicketPort` 独立，不塞进 `AdapterPort`。
4. **mock adapter（P1-6 已收口）**：已按 classify/analyze/plan/apply 拆到 `dev/mocks/adapter/`；主文件约 308 行。语义真源仍是协议图，不是 mock。
5. 上帝页：Chat **已按 P1-7 拆完**；Skills / Projects / AgentCard 仍偏厚。Dashboard 钱包读模型走 `activeBindingForAgent`（P0-9 已收口）。**可复用样板**已存在：`connection-model` + 薄 `index.tsx`、`adapter-view-model`、`agentOverviewModel`、`connect-flow-state`、`providerSaveFlow`。不要再造框架。

---

## 5. 改进方案

每条含：问题、建议、风险、验收。实施时一次只做一条或紧密相关的一小簇，保持可构建、可测试、可回退。

### P0 — 止血（正确性 / 认知坍缩 / 依赖箭头）

#### P0-1 安装渠道单一真源

- **状态（2026-08-16）**：**仍待办**。`AgentAdapter::install_channels()` 与 `InstallContribution` 仍并行。
- **问题**：`install_channels()` 与 `InstallContribution` 并行。
- **建议**：adapter 的 `install_channels` 改为从 `builtin_install_registry()` 派生，或删除 trait 方法、detect 只读 contribution。禁止两处字面量（含 `NATIVE_PS1_URL`）。
- **风险**：`channels[0]` 顺序被 detect/`env_ready` 依赖。
- **验收**：改一处 npm 包名，adapter 单测与 `list_install_catalog` 同时变；`rg` 无第二套渠道字面量。

#### P0-2 配置写路径收口（有 projector 的 Agent）

- **状态（2026-08-16）**：整块 apply 语义统一仍暂缓。已做最小切口：Codex/Kimi/Grok 的 provider-managed TOML 键与 Codex `auth.json` 写入收成单一真源（`integrations/agents/<key>/managed.rs`），双轨引用同一常量；契约测试断言 projector 写入的 native key ⊆ managed keys。`platform/config` 反向 import `adapters` 已随 P2-1 消失。
- **问题（剩余）**：`write_config` 整表替换与 projector 逐字段 merge 语义仍不同。
- **建议（后续，勿整块当 P0）**：live apply 只走 projector 仍是大改，单独排期。
- **风险**：Provider envelope 与 schema 字段映射。
- **验收（本切口）**：改一份 managed keys，provider switch 与契约测试同时变；Codex auth 只有一个写入函数。

#### P0-3 产品写入只走 `plan_ticket` / `bind_ticket` / `unbind_ticket`

- **状态（2026-08-16）**：**已收口**产品写口。ConnectFlow 走 `bindTicket`；`applyAdapter` 已 `@deprecated`，页面不得调用。`LocalBridge` 仍由 desktop host saga 执行（`TicketBindService` 拒绝，`DesktopAdapterControl` 转 `apply_local_bridge`）。
- **问题（剩余）**：host saga 与 ticket bind 仍并行；CLI 无 adapter 对称。
- **建议（后续）**：CLI 对 `local_bridge` 走同一 `adapter_control`；不要再把 `apply_adapter` 当产品入口。
- **风险**：旧脚本/前端依赖 apply。
- **验收（本切口）**：契约测试只经 ticket API；产品页面无 `applyAdapter` 调用。

#### P0-4 钉死 matrix ∩ write_gate ∩ apply 一致性

- **状态（2026-08-16）**：**仍待办**。规划表已迁 `domain/protocol_graph/`，但未见「同一 fixtures 表驱动、一测三断言」收口。
- **问题**：三处规则可漂移。
- **建议**：先加 **同一 fixtures 表驱动** 测试：每条 `rule_id` 断言 `matrix.can_apply ∧ write_gate ⇒ apply 有臂`。P1 再合并实现。
- **风险**：测试维护成本。
- **验收**：新增边必须先改矩阵 + 常量 + 一测三断言。

#### P0-5 Binding 命名铁律（可不改类型名）

- **状态（2026-08-16）**：**已收口**。`ConnectionService` 文件头与 [connection-binding-model.md](connection-binding-model.md) 已固定：`ActiveBinding` = Agent 当前行指针（禁止简称 Binding）；`TicketBinding` = 票→Agent 路线。
- **问题**：`ActiveBinding` vs `TicketBinding` 同叫 Binding。
- **建议**：后续只守注释/文档，不改类型名。
- **风险**：重命名 churn，P0 只做注释/文档即可。
- **验收**：本文件 + [connection-binding-model.md](connection-binding-model.md) 有对照表；review 禁止混用。

#### P0-6 下沉 `doctor-map` / `agent-connection`，修依赖倒挂

- **状态（2026-08-16）**：**已收口**。`src/lib/backend/tauri/{agent,doctor}.ts` 已改 import `contracts`（`agent-connection` / `doctor-map`）；`tauri/**` 不再依赖 `@/lib/api`。
- **问题**：`tauri/**` import `@/lib/api`。
- **建议**：保持 `contracts ← tauri/mocks ← api ← pages`。
- **风险**：改 import 面广但机械。
- **验收**：`src/lib/backend/tauri/**` grep 无 `@/lib/api`；boundary 测试加断言；相关 vitest 绿。

#### P0-7 冻结页面交叉 import

- **状态（2026-08-16）**：**已收口**。`src/lib/bridges-path.ts` 已被 Sidebar / Settings / Dashboard / App 使用。用户表面是 Routes / `/routes`，不是 Bridges。
- **问题**：bridges ↔ connections；布局层依赖 `pages/bridges`。
- **建议**：页面只依赖 `lib`；目录仍可叫 `pages/bridges/`。
- **风险**：一次挪文件。
- **验收**：`Sidebar` / `App` 不再 import `pages/bridges/*`；`pages/bridges` 不 import `pages/connections/*`。

#### P0-8 Skill market 编排下沉 core

- **状态（2026-08-16）**：**已收口**。GUI / CLI 均走 `AgentHub::search_skill_market` / `install_market_listing`（core `skill_market`）。
- **问题**：GUI/CLI 各写搜索 + installed 标记 + 市场源路由。
- **建议**：两壳各一行，不再复制市场源路由。
- **风险**：市场源行为微调。
- **验收**：同一 fixture 下 CLI/GUI installed 标记一致。

#### P0-9 Dashboard 复用钱包读模型

- **状态（2026-08-16）**：**已收口**。Dashboard 用 `activeBindingForAgent`（`src/lib/ticket-wallet.ts`），不是本地副本。
- **问题**：`activeWalletBinding` 与 `ticket-wallet-model.activeBindingForAgent` 双份。
- **建议**：连接/桥轮询若再抽 hook，仍复用同一纯函数。
- **风险**：低。
- **验收**：钱包绑定逻辑只在 `ticket-wallet` / `ticket-wallet-model`；Dashboard 相关测试绿。

### P1 — 按已有模式削胖（不改对外行为）

#### P1-1 拆 `adapters/mod.rs`

- **状态（2026-08-16）**：**已收口**。`mod.rs` 已是薄 façade（约 52 行）；`adapter_trait` / `registry` / `detect_binary` / `auth_revision` / `config_write` 已拆出。
- **建议**：`mod.rs` 只 re-export。
- **验收**：`mod.rs` < 200 行；`cargo test -p agenthub-core --lib adapters::` 通过。

#### P1-2 Lifecycle executor 真正消费 `InstallContribution`

- **状态（2026-08-16）**：**已收口**。`BuiltinLifecycleInstallExecutor` 按 `InstallContribution` allowlist 取 npm / URL / flags；未知 key 可不经完整 `AgentId` 执行。
- **建议**：builtin 现有 install 测试保持绿；不要再忽略 `_contribution`。
- **验收**：非 `AgentId` 的 fake contribution 经 coordinator install 成功；builtin 现有 install 测试全绿。
- **标签**：也是 sidecar/开闭的前置。

#### P1-3 Detect / SkillsTarget 摆脱「必须先有胖 adapter」

- **状态（2026-08-16）**：**已收口** detect 切口。生产注册走 `FnDetector`（`integrations/shared/register.rs`），不必先实现完整 `AgentAdapter`。`AdapterDetector` 仍是兼容包装。
- **建议（剩余）**：skills target 仍可从 `AdapterRegistry` 派生；过渡期 adapter 只留 account/run。
- **验收**：可注册生产 detector 而不实现完整 `AgentAdapter`；doctor 仍覆盖现有八家。

#### P1-4 Project sources 迁入 `platform/projects`

- **状态（2026-08-16）**：**已收口**。`platform/projects/sources.rs` 是兼容 façade，真源在 `integrations/agents/<key>/`；`services/project_service/sources.rs` 已不存在。`scan.rs` 仍偏厚。
- **建议（剩余）**：`scan.rs` 按 source 再切，不一次搬扫描逻辑重写。
- **验收**：`platform/projects` 含各 Agent source；services 无 agent 专属 `list_*`；project 测试绿。

#### P1-5 拆 Account / Adapter* 上帝文件

- **状态（2026-08-16 / 核对 2026-08-22）**：**已收口**按域拆目录。`account_service/{pool_crud,live_reconcile,switch_saga,import_live,surface}`；`pool_crud` 已再切为 `query` / `api_key` / `create` / `refresh` / `merge` / `compensate` / `types`（2026-08-22）。`adapter_{route,apply,bridge,secret}` 已按 classify / plan / saga / prepare / finalize 等切开。`TicketBindService::from_parts` / `AdapterApplyService::from_parts` 注入 hub 实例；`ConnectionTrashRepo` 已落地。
- **建议**：`new()` 兼容构造仍可 `with_live`；生产 `AgentHub::open` 走 `from_parts`。
- **验收**：公开 API 签名不变；`account_*` / `adapter_*` / `ticket_*` 过滤测试绿；`open` 后无第二套 `ProviderService::with_live`（测试除外）。

#### P1-6 前端契约与 mock 瘦身

- **状态（2026-08-16）**：**已收口**。`ports.ts` 约 78 行，只 re-export `Backend`；mock 已拆 `dev/mocks/adapter/{classify,analyze,plan,apply,rule-fixtures}`，主文件约 308 行。
- **建议**：文档写明「mock 非规则真源」；语义以协议图为准。
- **验收**：`ports.ts` 变薄；mock 主文件 < ~400；ConnectFlow 关键路径仍绿。

#### P1-7 上帝页按 Connections 样板拆文件

只拆文件、不改产品流。样板：纯函数进 `*-model`/`*-format`；副作用进 page hook；JSX 进同目录组件。

| 文件 | 下一刀 |
|---|---|
| `pages/chat/index.tsx` | **已收口**（约 147 行）。已拆 `chat-model` / `use-chat-page` / `ChatSessionRail` / `ChatSessionHeader` / `ChatTranscript` / `ChatMessageBubble` / `ChatSettingsDialog`（`ChatComposer` / `ChatProcessPanel` / `chat-format` 保留打磨） |
| `pages/skills/index.tsx` | 仍待办：preview-split + Library/Market；`SkillMarkdownPreviewPanel` 迁出 `shared` |
| `pages/projects/index.tsx` | 仍待办：format/prompts/filter + `ProjectTree` |
| `pages/agents/agent-card.tsx` | 仍待办：lifecycle hook + uninstall dialog |
| `ConnectFlowDialog.tsx` | **已收口** Select / Preview / Result（`ConnectFlow{Select,Preview,Result}Step`）；**不要硬拆** `connect-flow-state.ts` |
| `pages/bridges/adapter-model.ts` | **已收口** copy / resources / labels / create-flow / components；旧创建流符号离开运行时页 |
| `pages/settings/index.tsx` | **已收口** 按 tab 面板（General / Data / Security / Backups / About） |

- **验收**：各页 `index.tsx` 以编排为主；现有 `*.test.ts(x)` 跟着纯函数走。
- **清理**：`SwitchConfirmDialog` **已删除**。`OAuthFlowDialog` 仍在 `components/connect/`（shared 再导出；mock 另有一份），ConnectFlow 已接管产品流。

#### P1-8 `bridge/host.rs` 内拆 + 协议交叉

- **状态（2026-08-16）**：**已收口**内拆。`bridge/host/{lifecycle,http,dispatch}.rs` 已落地；`host/mod.rs` 只 re-export。
- **建议（剩余）**：Grok 特例进 protocol selector。`protocol/chat.rs` 只留 Chat→IR；Responses SSE 编码归 `responses`。
- **验收**：现有 bridge protocol fixtures / host 测试全绿。
- **标签**：可与 sidecar 并行，不阻塞 P0。

### P2 — 朝目标布局与进程边界收口

#### P2-1 物理目录 `integrations/agents/<key>/`

- **状态（2026-08-16）**：已落地。生产贡献经 `integrations::register_integrations`；`builtin_*_registry` 只读该集合。`AgentAdapter` 仍在 `adapters/`（过渡 `adapter_facade`）。test-only `demo-agent` 在 `integrations/agents/demo_agent/`，不进生产注册。
- **建议（后续）**：再把胖 `adapters/<id>.rs` 迁进各目录的 `adapter_facade`。
- **验收**：新增第九个 **test-only** agent 只加一个目录 + 一处 `register`；不改 platform service / 页面分支。

#### P2-2 `catalog/` 与 `agent_catalog` 消歧

- **状态（2026-08-16）**：兼容 façade **已落地**（`catalog/install.rs` 只 `pub use platform::install`）。消歧改名仍待办。
- **建议**：`catalog` 只留 limits/market，或改名 `product_constants`。
- **验收**：文档与 `pub use` 一致；无第二套 install 字面量。

#### P2-3 `AgentId` 继续降级为兼容 DTO（不删）

- **状态（2026-08-16）**：**已收口** catalog 组合。`AgentCatalogService` 按 registry 注册序走 `AgentKey`，不再 `for id in AgentId::ALL`。`AgentId` 仍是兼容 DTO，不删。
- **建议**：未知 key → unavailable；旧 API/DB 仍可用 `AgentId`。
- **验收**：未知 key → unavailable；旧 API/DB 仍可用 `AgentId`。

#### P2-4 规划图正名

- **状态（2026-08-16）**：**已收口**。规划表在 `domain/protocol_graph/`（`adapter_capability_matrix` + `agent_capability`）；`models/mod.rs` 只 re-export 兼容路径，不再宣称「纯数据」。
- **建议**：新规划调用走 domain 路径。
- **验收**：`models/mod.rs` 与内容一致；plan 单测仍过。

#### P2-5 use-case 门面 + sidecar 前置契约

- **状态（2026-08-16）**：控制契约 **已落地（in-process）**；sidecar 二进制 **未开始**。
  - 已有：`crates/agenthub-core/src/adapter_control/{mod,contract,coordinator,status}.rs` + `src-tauri/src/adapter_control_host.rs`（`DesktopAdapterControl`）
  - 未有：`crates/agenthub-adapterd/`、IPC client、`DataStoreBootstrap` / `SchemaGenerationLease` 实现（仅文档出现）

按 [adapter-sidecar-design.md](adapter-sidecar-design.md) 的既有阶段，模块化侧 **Phase 1 前置已完成**：

1. `agenthub_core::adapter_control`：apply/start/stop/remove/status/restore（in-process host）。
2. `lock_target` / profile gate 已进 control 模块。
3. bind/unbind use-case 进 core；command 只 parse + 调 contract。
4. **仍待办**：`agenthub-adapterd` + IPC + schema lease（已有专文，本文不重复展开）。

- **验收**：GUI 行为不变；mutation 只走 contract；AppState 最终不再持有 saga 实现类型（末期才删 `BridgeRuntimeHost`）。
- **不做**：把 Connections 迁进 sidecar；把 `native_endpoint` / `config_sync` 塞进 sidecar。

#### P2-6 SkillService 瘦到 API façade

- **状态（2026-08-16）**：**已收口**。`skill_service/` 已是编排 façade（`mod.rs` 约 222 行）；YAML / hash / 分类在 `platform/skills`。
- **建议**：新增逻辑进子模块或 platform，不要再堆回单文件。
- **验收**：职责清单与行数达标；skills 测试绿。

#### P2-7 凭据行读模型收敛（前端）

- **状态（2026-08-16）**：**已收口**。`toCredentialRow()`（`src/lib/credential-row.ts`）被 Connections 与 ConnectFlow 共用；`ConnectionEntry` 在其上加 UI 字段。
- **建议**：`types.ts` 不一次拆完；新字段：wire 进 contracts，纯 UI 进 page/lib view。
- **验收**：单一 `toCredentialRow()`（或等价）被 Connections 与 ConnectFlow 共用。

---

## 6. 执行顺序

```text
已收口（2026-08-16 回写，勿再派工）
  P0-3 产品写口 bind/unbind（host saga 仍并行）
  P0-5 命名铁律
  P0-6 依赖倒挂
  P0-7 跨页 import
  P0-8 skill market
  P0-9 Dashboard 钱包去重
  P1-1 adapters/mod 拆文件
  P1-2 lifecycle 吃 contribution
  P1-3 detect 走 FnDetector
  P1-4 project sources 搬家
  P1-5 Account/Adapter 切分 + from_parts
  P1-6 ports / mock 拆文件
  P1-7 Chat / ConnectFlow / Settings / bridges 拆文件
  P1-8 bridge host 内拆
  P2-1 integrations/ 物理收口
  P2-3 AgentId catalog 不再扫 ALL
  P2-4 规划图正名
  P2-5 adapter_control 契约（in-process）
  P2-6 SkillService 瘦身
  P2-7 前端凭据行收敛

仍待办
  P0-1 install 单真源
  P0-2 config 写路径整表 vs projector（最小切口已做）
  P0-4 matrix/plan/apply 一致性测试
  P1-7 Skills / Projects / AgentCard
  P2-2 catalog 消歧改名
  P2-5 sidecar 二进制 + IPC + schema lease + CLI 同一 client
```

派工原则（与 [AGENTS.md](../AGENTS.md) 一致）：

- 一条 P0/P1 对应一次可独立 PR；禁止「顺便」改无关目录。
- 行为不变的拆文件优先；双真源收口必须带契约测试。
- 新增 Agent 按 [adding-an-agent.md](adding-an-agent.md) 走 `integrations/agents/<key>/`；**禁止** 再复制 install 字面量或在 platform service 里加 `match AgentId`。胖 `adapters/<id>.rs` 仍是过渡 façade，不要宣称「只改一处」。

---

## 7. 对照表：Binding / 写入 / 配置

| 名称 | 是什么 | 不是什么 |
|---|---|---|
| `Ticket` | 钱包读模型（`account:<id>` / `provider:<id>`） | 新表 |
| `TicketBinding` | 票接到某 Agent 的路线（native / reshape / bridge） | Agent 当前指针 |
| `ActiveBinding` | `ConnectionService` 的 current 指针 | 产品「绑定」 |
| 前端 `connection-pool-store` | accounts + providers 列表缓存（`src/app/runtime/connection-pool-store.ts`） | `ConnectionService`（ActiveBinding 事务 owner）；同名不同物，禁止混称（核对日期 2026-08-22） |
| `AdapterProfile` | reshape/bridge 的持久化痕迹 | 钱包里的第二套票 |
| `apply_adapter` | 内部 reshape / host 兼容运输 | 产品写入入口（已 `@deprecated`；页面不得调用） |
| `bind` / `unbind` | 产品唯一写入 | — |
| `ConfigurationService` | schema / 校验 / 通用配置 UI | 连接切换 live owner |
| `ProviderService` saga | 连接/bind 的 live owner | 通用配置表单 |

---

## 8. 新增 Agent：现状 vs 目标改动面

**现状（P2-1 已落地）**：生产贡献经 `integrations/agents/<key>/` + `integrations::register_integrations`。八家生产 Agent + test-only `demo_agent`（不进生产 registry）。`AgentAdapter` 仍在 `adapters/` 作过渡 façade。

**目标（削厚表面之后）**：

```text
integrations/agents/<agent_key>/
  descriptor
  实际支持的稀疏端口
  fixtures
```

不修改平台 service、不修改页面业务分支、不新增表。`demo-agent` 已证明这条轨在测试里可行；胖 `adapters/<id>.rs` 迁完之前，不要宣称「加 Agent 只改一处」。

---

## 9. 验收北极星

做完一轮收口后，应同时为真：

1. 改一个 install 包名或一条 bind 边，只碰 **一个真源** + 契约测试。
2. `platform/config` 不再 import `adapters::*`。
3. 产品写路径只有 `plan` / `bind` / `unbind`；GUI 与（未来）CLI 对 `local_bridge` 走同一 contract。
4. `src/lib/backend/tauri` 不依赖 `@/lib/api`。
5. 页面模块不互相 import；布局不依赖 `pages/*` 实现。
6. 新上帝文件不再出现：新增逻辑进已有子模块或 `*-model.ts`，而不是继续往 2000 行文件堆。
7. 凭据落盘加密仍不在范围；Connections 仍不拆进程。

---

## 10. 审查证据索引

| 域 | 关键路径 |
|---|---|
| 门面装配 | `crates/agenthub-core/src/lib.rs` |
| 胖 trait（过渡 façade） | `crates/agenthub-core/src/adapters/mod.rs`（薄 re-export） |
| 平台入口 | `crates/agenthub-core/src/platform/mod.rs` |
| 反向依赖 | `platform/config` 不再 import `adapters`；`sources/dsh.rs` **不存在** |
| 连接写入 | `services/{ticket_bind,ticket_read,adapter_*,account,provider,connection}_service.rs` |
| 规划矩阵 | `domain/protocol_graph/{adapter_capability_matrix,agent_capability}.rs` |
| 控制契约 | `crates/agenthub-core/src/adapter_control/`、`src-tauri/src/adapter_control_host.rs` |
| 壳层 saga | `src-tauri/src/adapter_bridge_controller.rs`、`commands/adapter.rs`、`state.rs` |
| 前端装配 | `src/lib/backend/current.ts`、`tauri/create-backend.ts`、`app/runtime` |
| 倒挂（已收口） | `src/lib/backend/tauri/{agent,doctor}.ts` → `contracts` |
| 跨页（已收口） | `src/lib/bridges-path.ts` |
| mock 双真源 | `src/dev/mocks/adapter/`（按域拆文件；非规则真源） |
| 页面样板 | `src/pages/connections/connection-model.ts`、`src/lib/connect-flow/` |
