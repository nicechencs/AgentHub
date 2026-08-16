# AgentHub 模块化审查与改进方案

> 状态：稳定改进方案（审查结论 + 分阶段收口）。不替代现有真源。
> 审查日期：2026-08-15
> 方法：主 Agent 汇总目录/体量/依赖证据；5 个 Grok Agent 分域深读（adapters+platform、services+models、前端 backend、页面组件、Tauri/CLI/Bridge）。
> 适用范围：模块化单体。不引入微服务、DDD、CQRS、事件总线、动态插件 ABI。凭据落盘加密为**项目范围外**。

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

当前模块化的主矛盾不是「缺分层」，而是 **生产路径仍以胖 `AgentAdapter` + 多入口服务拓扑为枢纽**，目标文档里的稀疏端口、Ticket 唯一写入、`integrations/agents/<key>/`、sidecar control contract **尚未成为生产组合方式**。

一句话：

```text
骨架是模块化单体；生产组合仍是 Adapter-centric。
下一步最有杠杆的是消灭双真源 / 统一写入入口 / 抽 control contract，
而不是再拆 crate、再引入框架、或一次性搬家目录。
```

建议按三层推进：

| 层 | 目标 | 典型动作 |
|---|---|---|
| **P0 止血** | 双真源、依赖倒挂、产品写入分叉 | 收口 install/config、bind 唯一写、修 tauri→api 倒挂、冻结跨页 import |
| **P1 削胖** | 上帝文件按已有模式切开 | 拆 adapters/mod、Account/Adapter*、Chat/Skills/Projects、ports.ts |
| **P2 收口** | 对齐目标布局与进程边界 | `integrations/agents/<key>/`、use-case 门面、sidecar 前置契约 |

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
| 稀疏端口、新增 Agent 只加一目录 | `integrations/agents/<key>/` **不存在**；生产改 8–13 处 | 未落地 |
| Ticket + Binding 唯一写入 | 读模型 / `plan` / `bind` 已有；`apply_adapter` 与 host saga 仍并行 | 半落地 |
| 前端 backend 分层 | transport 干净；contracts 过厚、api 夹映射、页面 VM 交叉 | 骨架健康 |
| 页面 = 编排 + 本地 state | Connections / Bridges 接近；Chat / Skills / Projects / AgentCard 仍是上帝页 | 不均 |
| GUI/CLI 共用 control contract | CLI **无** adapter/bridge；saga 只在 Tauri | 不对称 |
| sidecar | 设计已决策；`adapter_control` / `agenthub-adapterd` **不存在** | 未开始 |

### 3.1 体量信号（2026-08-15 工作区）

生产侧（不含 `tests.rs`）热点：

| 层 | 文件 | 行数 | 角色 |
|---|---|---:|---|
| core | `services/skill_service.rs` | 2432 | 厚 façade，实现已部分下沉 `platform/skills` |
| core | `usage/session_jsonl.rs` | 2342 | 解析器（隔离合理，体量大） |
| core | `services/account_service.rs` | 2046 | 池 + live saga + import |
| core | `services/project_service/scan.rs` | 1860 | 项目扫描仍在 services |
| core | `bridge/protocol/responses.rs` | 1842 | 协议转换 |
| core | `bridge/host.rs` | 1769 | listener + HTTP + 转发 |
| core | `services/account_quota.rs` | 1629 | 配额附属 |
| core | `services/adapter_{apply,secret,bridge,install}` | 1414–1495 | 按商品规则膨胀 |
| core | `adapters/mod.rs` | 1308 | trait + 写工具 + registry + detect |
| core | `adapters/cursor.rs` / `workbuddy.rs` | 1095 / 1018 | 安装探测与 auth 混杂 |
| tauri | `adapter_bridge_controller.rs` | 1028 | **壳层持有完整 local_bridge saga** |
| 前端 | `dev/mocks/adapter.ts` | 1795 | mock 内嵌第二套路由引擎 |
| 前端 | `pages/chat/index.tsx` | 1398 | UI + 编排一体 |
| 前端 | `pages/skills/index.tsx` | 1231 | 预览 + 写路径上帝页 |
| 前端 | `components/connect/ConnectFlowDialog.tsx` | 1168 | 状态机已抽，UI 未拆 |
| 前端 | `pages/projects/index.tsx` | 1135 | 树 + mutation 同页 |
| 前端 | `pages/agents/agent-card.tsx` | 964 | 生命周期副作用全集中 |

行数本身不是罪；**同一文件多个变化原因、双真源、依赖倒挂** 才是。

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
platform/{detection,skills,agent_catalog,lifecycle,config/dsh} → adapters
```

硬证据：`platform/config/sources/dsh.rs` `use crate::adapters::dsh::{...}`。

### 4.2 双真源：Install 与 Config

| 主题 | 轨 A | 轨 B | 风险 |
|---|---|---|---|
| 安装渠道 | `AgentAdapter::install_channels()` | `InstallContribution` + `catalog/install::channels_for` | npm 包名 / 顺序 / 渠道集合漂移 |
| 配置写入 | `ProviderService` → `AgentAdapter::write_config` | `ConfigurationService` → projector | 同一 Agent 两套 apply；dsh 被 projector 反向 import |

新增 Agent 必须同时改 adapter、`AgentId`、`agent_bind_capability`、各 `platform/*/sources`、常还要改 `project_service/sources` 与前端类型。理想改动面 `integrations/agents/<key>/` 物理目录为 0%。

### 4.3 连接域：过渡别名未收口 + Binding 双义

目标对象是 **Ticket + Binding + 协议图**。存储仍是 `accounts` + `providers` + `adapter_profiles` + `agent_active_bindings`。这可以接受，但对外入口和命名没有收口。

- **写入双入口**：产品文档说 `bind` / `unbind` 是唯一写入；代码仍暴露 `apply_adapter`，且 `LocalBridge` 必须走 Tauri `bind_ticket_inner` → `AdapterBridgeSagaCoordinator`。`TicketBindService::bind` 对 `LocalBridge` 直接拒绝。
- **Binding 双义**：`TicketBinding`（票→Agent 路线）vs `ConnectionService::ActiveBinding`（Agent→当前行指针）。同词不同物。
- **规划真理三处**：`models/adapter_capability_matrix.rs` 的 `decide_adapter_capability`、`AdapterRouteService` 的 `write_gate` / `actions_for`、`AdapterApplyService::apply` 再 match。plan 可点、apply 失败（或反之）的漂移面。
- **`models/` 已不是纯数据**：`adapter_capability_matrix.rs`（~970 行）含决策与文案，与 `models/mod.rs`「纯数据结构」注释冲突。
- **装配重复**：`TicketBindService` / `AdapterApplyService` 各自再 `ProviderService::with_live`，与 `AgentHub.providers` 不是同一实例。

### 4.4 壳层持有本不该有的业务真相

`src-tauri/src/lib.rs` 写明 *Business logic stays in core*，但：

- `adapter_bridge_controller.rs`（1028）持有完整 `local_bridge` saga 与 **target-agent 进程内锁**。
- `commands/adapter.rs`（508）持有 bind/unbind 路由与 wallet `bridge.running` 富化。
- `provider` / `account` / `oauth` / `configuration` / `backup` / `install` 即使不做桥，也抢 `bridge_saga_coordinator.lock_target`。CLI **没有**对应门禁 → GUI/CLI 并发改同一 Agent live 时，壳层串行只对 GUI 有效。
- Skill market 搜索/安装路由在 Tauri 与 CLI **各写一份且已漂移**。
- `adapter_control/`、`agenthub-adapterd/`、sidecar client **均不存在**。没有这些，无法把 controller 从 Tauri 撕下。

`AgentHub` 门面挂 20+ 服务字段。对桌面单体可接受；对 sidecar 组合与双客户端一致性，缺 **use-case 门面**（尤其 Adapter / local_bridge）。

### 4.5 前端：骨架对、依赖网交叉

**已落地**：`#backend` 按命令装配；页面业务调用走 `@/lib/api/*`；pages/components 零 `invoke` / `isTauriApp`。

**债**：

1. **依赖倒挂**：`lib/backend/tauri/agent.ts` → `@/lib/api/agent-connection`；`tauri/doctor.ts` → `@/lib/api/doctor-map`。箭头应是 `contracts ← tauri/mocks ← api ← pages`。
2. **跨页耦合**：`pages/bridges/adapter-model` ↔ `pages/connections/*`；`Sidebar` / `App` import `pages/bridges` 路径常量。
3. **`ports.ts`（480）+ `types.ts`（503）+ 多套页面 VM** 叠层；鉴权展示、连接标签、票行模型多源。
4. **`dev/mocks/adapter.ts`（1795）** 复刻 classify/plan/apply。Port 形状稳定，**语义（哪条边可 apply）不稳定**，用巨型 mock 对冲。
5. 上帝页：Chat / Skills / Projects / AgentCard / Dashboard 半拆。**可复用样板**已存在：`connection-model` + 薄 `index.tsx`、`adapter-view-model`、`agentOverviewModel`、`connect-flow-state`、`providerSaveFlow`。不要再造框架。

---

## 5. 改进方案

每条含：问题、建议、风险、验收。实施时一次只做一条或紧密相关的一小簇，保持可构建、可测试、可回退。

### P0 — 止血（正确性 / 认知坍缩 / 依赖箭头）

#### P0-1 安装渠道单一真源

- **问题**：`install_channels()` 与 `InstallContribution` 并行。
- **建议**：adapter 的 `install_channels` 改为从 `builtin_install_registry()` 派生，或删除 trait 方法、detect 只读 contribution。禁止两处字面量（含 `NATIVE_PS1_URL`）。
- **风险**：`channels[0]` 顺序被 detect/`env_ready` 依赖。
- **验收**：改一处 npm 包名，adapter 单测与 `list_install_catalog` 同时变；`rg` 无第二套渠道字面量。

#### P0-2 配置写路径收口（有 projector 的 Agent）

- **问题**：`write_config` 与 projector 双轨；`platform/config` → `adapters::dsh` 反向依赖。
- **建议**：Claude/Codex/Kimi/Grok/Dsh 的 live apply 只走 projector；adapter `write_config` 变薄委托。共享 YAML/JSON helpers 下沉到 `platform/config/sources/<id>/`。
- **风险**：Provider envelope 与 schema 字段映射。
- **验收**：provider switch 与 GenericConfigForm 走同一 apply；`rg 'use crate::adapters::' platform/config` 为空。现有 provider/config 测试全绿。

#### P0-3 产品写入只走 `plan_ticket` / `bind_ticket` / `unbind_ticket`

- **问题**：`apply_adapter` 与 `bind_ticket` 双暴露；bridge 必须走 host。
- **建议**：Tauri/CLI 产品路径只留 ticket API；`apply_adapter` 标 deprecated 或仅测试。bridge 仍由 host 调内部 `AdapterBridgeService`，对上包装成 `bind`。
- **风险**：旧脚本/前端依赖 apply。
- **验收**：契约测试只经 ticket API；产品代码无新增 `apply_adapter` 调用。

#### P0-4 钉死 matrix ∩ write_gate ∩ apply 一致性

- **问题**：三处规则可漂移。
- **建议**：先加 **同一 fixtures 表驱动** 测试：每条 `rule_id` 断言 `matrix.can_apply ∧ write_gate ⇒ apply 有臂`。P1 再合并实现。
- **风险**：测试维护成本。
- **验收**：新增边必须先改矩阵 + 常量 + 一测三断言。

#### P0-5 Binding 命名铁律（可不改类型名）

- **问题**：`ActiveBinding` vs `TicketBinding` 同叫 Binding。
- **建议**：文档与代码注释固定：`ActiveBinding` = Agent 当前行指针（禁止简称 Binding）；`TicketBinding` = 票→Agent 路线。`ConnectionService` 文件头写明「非 connection-binding-model 的 Binding」。
- **风险**：重命名 churn，P0 只做注释/文档即可。
- **验收**：本文件 + [connection-binding-model.md](connection-binding-model.md) 有对照表；review 禁止混用。

#### P0-6 下沉 `doctor-map` / `agent-connection`，修依赖倒挂

- **问题**：`tauri/**` import `@/lib/api`。
- **建议**：纯映射迁到 `lib/backend/contracts/`（或并列纯模块）；`lib/api` 只 re-export。
- **风险**：改 import 面广但机械。
- **验收**：`src/lib/backend/tauri/**` grep 无 `@/lib/api`；boundary 测试加断言；相关 vitest 绿。

#### P0-7 冻结页面交叉 import

- **问题**：bridges ↔ connections；布局层依赖 `pages/bridges`。
- **建议**：抽 `BRIDGES_PATH` / `bridgesHrefForProfile` / 共享行类型到 `src/lib/`（可旁路现有 `connection-kind.ts`）。页面只依赖 `lib`。
- **风险**：一次挪文件。
- **验收**：`Sidebar` / `App` 不再 import `pages/bridges/*`；`pages/bridges` 不 import `pages/connections/*`。

#### P0-8 Skill market 编排下沉 core

- **问题**：GUI/CLI 各写搜索 + installed 标记 + 市场源路由。
- **建议**：`SkillService`（或薄 helper）统一 `search_market` / `install_market_listing`；两壳各一行。
- **风险**：市场源行为微调。
- **验收**：同一 fixture 下 CLI/GUI installed 标记一致。

#### P0-9 Dashboard 复用钱包读模型

- **问题**：`activeWalletBinding` 与 `ticket-wallet-model.activeBindingForAgent` 双份。
- **建议**：Dashboard 删除本地副本，改 import 已有纯函数；连接/桥轮询可抽 `useDashboardConnect`。
- **风险**：低。
- **验收**：钱包绑定逻辑只在 `ticket-wallet-model`；Dashboard 相关测试绿。

### P1 — 按已有模式削胖（不改对外行为）

#### P1-1 拆 `adapters/mod.rs`

- **建议**：`adapter_trait.rs`、`registry.rs`、`detect_binary.rs`、`auth_revision.rs`、`config_write.rs`。`mod.rs` 只 re-export。
- **验收**：`mod.rs` < 200 行；`cargo test -p agenthub-core --lib adapters::` 通过。

#### P1-2 Lifecycle executor 真正消费 `InstallContribution`

- **建议**：`install_service` 按 contribution 取 npm/URL/flags；或 allowlist 执行搬进 `platform/install`。`BuiltinLifecycleInstallExecutor` 不得忽略 `_contribution`。
- **验收**：非 `AgentId` 的 fake contribution 经 coordinator install 成功；builtin 现有 install 测试全绿。
- **标签**：也是 sidecar/开闭的前置。

#### P1-3 Detect / SkillsTarget 摆脱「必须先有胖 adapter」

- **建议**：builtin 可为独立 `ClaudeDetector` 等注册；skills 用 `StaticSkillTarget` 或 paths contribution。adapter 过渡期只留 account/run。
- **验收**：可注册生产 detector 而不实现完整 `AgentAdapter`；doctor 仍覆盖现有八家。

#### P1-4 Project sources 迁入 `platform/projects`

- **建议**：移动 `services/project_service/sources.rs` 实现；`ProjectService` 只编排。`scan.rs` 按 source 再切，不一次搬 1860 行逻辑重写。
- **验收**：`platform/projects` 含各 Agent source；services 无 agent 专属 `list_*`；project 测试绿。

#### P1-5 拆 Account / Adapter* 上帝文件

```text
account/          pool_crud · live_reconcile · switch_saga · import_live · surface
adapter/route/    classify · plan · actions
adapter/apply/    saga · specs/<target>
adapter/bridge/   prepare · finalize · removal · rules
adapter/secret/   按 source product
```

- **建议**：`TicketBindService::from_parts(...)` 注入 `hub` 已有实例，禁止再 `with_live`。`ConnectionService` 的 trash SQL 抽 `ConnectionTrashRepo`。
- **验收**：公开 API 签名不变；`account_*` / `adapter_*` / `ticket_*` 过滤测试绿；`open` 后无第二套 `ProviderService::with_live`（测试除外）。

#### P1-6 前端契约与 mock 瘦身

- **建议**：`ports.ts` 按域拆，自身只 re-export `Backend`（目标 < 80 行）。mock `adapter.ts` 先按 classify/analyze/plan/apply 拆文件，再用 ruleId fixture 表替换部分分支；文档写明「mock 非规则真源」。
- **验收**：`ports.ts` 变薄；mock 主文件 < ~400；ConnectFlow 关键路径仍绿。

#### P1-7 上帝页按 Connections 样板拆文件

只拆文件、不改产品流。样板：纯函数进 `*-model`/`*-format`；副作用进 page hook；JSX 进同目录组件。

| 文件 | 下一刀 |
|---|---|
| `pages/chat/index.tsx` | 已拆 `chat-model` / `use-chat-page` / `ChatSessionRail` / `ChatSessionHeader` / `ChatTranscript` / `ChatMessageBubble` / `ChatSettingsDialog`（`ChatComposer` / `ChatProcessPanel` / `chat-format` 保留打磨） |
| `pages/skills/index.tsx` | preview-split + Library/Market；`SkillMarkdownPreviewPanel` 迁出 `shared` |
| `pages/projects/index.tsx` | format/prompts/filter + `ProjectTree` |
| `pages/agents/agent-card.tsx` | lifecycle hook + uninstall dialog |
| `ConnectFlowDialog.tsx` | Select / Preview / Result；**不要硬拆** `connect-flow-state.ts` |
| `pages/bridges/adapter-model.ts` | copy / resources / labels；旧创建流符号离开运行时页 |
| `pages/settings/index.tsx` | 按 tab 面板（Backups 已拆） |

- **验收**：各页 `index.tsx` 以编排为主；现有 `*.test.ts(x)` 跟着纯函数走。
- **清理**：确认后删除无生产引用的 `SwitchConfirmDialog`；评估 `OAuthFlowDialog`（ConnectFlow 已接管，mock 若仍用则移入 `dev/mocks` 或 `connect/`）。

#### P1-8 `bridge/host.rs` 内拆 + 协议交叉

- **建议**：`host/{lifecycle,http,dispatch}.rs`；Grok 特例进 protocol selector。`protocol/chat.rs` 只留 Chat→IR；Responses SSE 编码归 `responses`。host 内私有 `AppState` 改名 `ListenerState`。
- **验收**：现有 bridge protocol fixtures / host 测试全绿。
- **标签**：可与 sidecar 并行，不阻塞 P0。

### P2 — 朝目标布局与进程边界收口

#### P2-1 物理目录 `integrations/agents/<key>/`

- **建议**：先 `mod` 重导出再搬文件；每 Agent 一目录：`paths` / `install` / `config` / `usage` / `stream` / `project` / 过渡 `adapter_facade`。
- **验收**：新增第九个 **test-only** agent 只加一个目录 + 一处 `register_integrations`；不改 platform service / 页面分支。

#### P2-2 `catalog/` 与 `agent_catalog` 消歧

- **建议**：`catalog/install` API 并入 `platform/install` 导出；`catalog` 只留 limits/market，或改名 `product_constants`。
- **验收**：文档与 `pub use` 一致；无第二套 install 字面量。

#### P2-3 `AgentId` 继续降级为兼容 DTO（不删）

- **建议**：`AgentCatalogService` 不再 `for id in AgentId::ALL`；生产注册改 descriptor + key。与 remediation「暂缓删除」一致。
- **验收**：未知 key → unavailable；旧 API/DB 仍可用 `AgentId`。

#### P2-4 规划图正名

- **建议**：`domain/protocol_graph`（或 `services/adapter_graph`）收 matrix + `agent_capability` + route classify；`models` 只留 wire DTO，或删除「纯数据」宣称。
- **验收**：`models/mod.rs` 与内容一致；plan 单测仍过。

#### P2-5 use-case 门面 + sidecar 前置契约

按 [adapter-sidecar-design.md](adapter-sidecar-design.md) 的既有阶段，模块化侧只补 **Tauri-neutral 前置**：

1. `agenthub_core::adapter_control`（或 `services::local_bridge_app`）：apply/start/stop/remove/status/restore。
2. `lock_target` / profile gate 迁出 Tauri 类型，进 core 或 control 模块。
3. bind/unbind use-case 进 core；command 只 parse + 调 contract。
4. 再做 `agenthub-adapterd` + IPC + schema lease（已有专文，本文不重复展开）。

- **验收**：GUI 行为不变；mutation 只走 contract；AppState 最终不再持有 saga 实现类型（末期才删 `BridgeRuntimeHost`）。
- **不做**：把 Connections 迁进 sidecar；把 `native_endpoint` / `config_sync` 塞进 sidecar。

#### P2-6 SkillService 瘦到 API façade

- **建议**：YAML / hash / 投影分类继续进 `platform/skills`；`skill_service.rs` 目标 < ~400 行。
- **验收**：职责清单与行数达标；skills 测试绿。

#### P2-7 凭据行读模型收敛（前端）

- **建议**：以 Ticket 读模型为轴，`ConnectionEntry` / `TicketWalletRow` / ConnectFlow `SourceOption` 从同一投影函数生成。`types.ts` 不一次拆完；新字段：wire 进 contracts，纯 UI 进 page/lib view。
- **验收**：单一 `toCredentialRow()`（或等价）被 Connections 与 ConnectFlow 共用。

---

## 6. 执行顺序

```text
可并行（低风险）
  P0-5 命名铁律
  P0-6 依赖倒挂
  P0-7 跨页 import
  P0-8 skill market
  P0-9 Dashboard 钱包去重
  P1-1 adapters/mod 拆文件
  P1-6 ports / mock 拆文件
  P1-7 上帝页拆文件
  P1-8 bridge host 内拆

必须串行（正确性）
  P0-1 install 单真源
  P0-2 config 写路径收口
  P0-3 bind 唯一产品写  ──►  P0-4 matrix/plan/apply 一致性测试
  P1-2 lifecycle 吃 contribution
  P1-3 detect/skills 稀疏端口
  P1-4 project sources 搬家
  P1-5 Account/Adapter 切分 + 共享 Provider 实例

sidecar 前门禁（已有专文）
  P2-5 adapter_control + 门禁下沉 + bind 下沉
        → schema lease → adapterd + IPC → 退出语义 → CLI 同一 client

最后
  P2-1 integrations/ 物理收口
  P2-2 catalog 消歧
  P2-3 AgentId 降级
  P2-4 规划图正名
  P2-6 SkillService 瘦身
  P2-7 前端凭据行收敛
```

派工原则（与 [AGENTS.md](../AGENTS.md) 一致）：

- 一条 P0/P1 对应一次可独立 PR；禁止「顺便」改无关目录。
- 行为不变的拆文件优先；双真源收口必须带契约测试。
- 新增 Agent 在 P2-1 落地前，仍按 [adding-an-agent.md](adding-an-agent.md) 生产兼容轨，但 **禁止** 再复制 install 字面量或在 platform service 里加 `match AgentId`。

---

## 7. 对照表：Binding / 写入 / 配置

| 名称 | 是什么 | 不是什么 |
|---|---|---|
| `Ticket` | 钱包读模型（`account:<id>` / `provider:<id>`） | 新表 |
| `TicketBinding` | 票接到某 Agent 的路线（native / reshape / bridge） | Agent 当前指针 |
| `ActiveBinding` | `ConnectionService` 的 current 指针 | 产品「绑定」 |
| `AdapterProfile` | reshape/bridge 的持久化痕迹 | 钱包里的第二套票 |
| `apply_adapter` | 内部 reshape 实现 | 产品写入入口（应降为内部） |
| `bind` / `unbind` | 产品唯一写入 | — |
| `ConfigurationService` | schema / 校验 / 通用配置 UI | 连接切换 live owner |
| `ProviderService` saga | 连接/bind 的 live owner | 通用配置表单 |

---

## 8. 新增 Agent：现状 vs 目标改动面

**现状（生产兼容轨）** 仍须改：`adapters/<id>.rs`、`register_all`、`AgentId`、`agent_bind_capability`、paths/install/config/usage/stream/project 各 sources、前端过渡类型。约 8–13 处。

**目标（P2-1 后）**：

```text
integrations/agents/<agent_key>/
  descriptor
  实际支持的稀疏端口
  fixtures
```

不修改平台 service、不修改页面业务分支、不新增表。`demo-agent` 已证明这条轨在测试里可行；生产组合跟上之前，不要宣称「加 Agent 只改一处」。

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
| 胖 trait | `crates/agenthub-core/src/adapters/mod.rs` |
| 平台入口 | `crates/agenthub-core/src/platform/mod.rs` |
| 反向依赖 | `crates/agenthub-core/src/platform/config/sources/dsh.rs` |
| 连接写入 | `services/{ticket_bind,ticket_read,adapter_*,account,provider,connection}_service.rs` |
| 规划矩阵 | `models/adapter_capability_matrix.rs`、`models/agent_capability.rs` |
| 壳层 saga | `src-tauri/src/adapter_bridge_controller.rs`、`commands/adapter.rs`、`state.rs` |
| 前端装配 | `src/lib/backend/current.ts`、`tauri/create-backend.ts`、`app/runtime` |
| 倒挂 | `src/lib/backend/tauri/{agent,doctor}.ts` |
| 跨页 | `src/pages/bridges/adapter-model.ts`、`src/pages/connections/*` |
| mock 双真源 | `src/dev/mocks/adapter.ts` |
| 页面样板 | `src/pages/connections/connection-model.ts`、`src/lib/connect-flow/` |
