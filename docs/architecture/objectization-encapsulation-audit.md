---
title: 对象化与封装审查
type: explanation
status: current
owner: maintainers
updated: 2026-08-26
---

# 对象化与封装审查

本文是对 `dev` 分支当前可维护源码的对象职责、状态归属、行为归属和封装边界审查。它记录问题和调整方向，不直接生成开发任务；涉及跨层或事务行为的调整，仍需先完成单独设计。

本轮在上一版重点审查基础上，补查了 `crates/agenthub-cli`、根目录脚本与发布配置、前端公共组件/配置/样式、core 的 adapter/integration/bridge/platform/runtime/catalog/usage/utils、测试/e2e/mock/fixture。依赖目录、二进制和生成的图标/静态数据不作为对象职责审查对象；纯凭据暴露、复制提示和发布参数等非对象化问题也不混入本记录。

## 核实记录

2026-08-26 对照源码复核：问题描述均能在当前代码中对上，建议方向大体合理。按风险与影响面，只落地了不改变 current / 锁 / 补偿语义的收窄：

| 编号 | 核实 | 处理 |
| --- | --- | --- |
| O-01 | 仍存在：`AgentHub` 字段全部 `pub`，CLI / Tauri 直接读 `db`、`registry` | 暂缓。这是组合根，收窄公开面需要单独列调用方清单 |
| O-02 | 仍存在：`Database::with_conn` 为 `pub`；生产路径曾用 `ProviderService::repo()` 读行 | 部分处理：生产读改为 `get_by_id` / `get_current`；Account / Backup 的 `repo()` 收为 `pub(crate)`。`with_conn` 与 Provider `repo()` 仍给测试和补偿事务用 |
| O-03、O-04、O-05 | 仍存在 | 暂缓。页面拆 Hook / 把保存用例上收属于跨层设计 |
| O-06 | 生产 Hook 已走共享连接池；`loadAdapterPageResources` 只留在测试 | 标注为测试辅助，页面不得再自行拉账号 / Provider |
| O-07 | 仍存在：多个模块级 store，`setBackend` 手工 reset | 暂缓。共享 store 本身保留；coordinator 先只管刷新，不接管生命周期 |
| O-08、O-09 | 三处 façade 各自刷新，ticket bind 曾 `.catch(() => {})` | 已处理：统一刷新；失败留在 snapshot。连接页和 Chat 可看到并重试 |
| O-10 | Chat 曾本地 `listTicketWallet` | 已处理：Chat 订阅共享票夹 |
| O-11–O-14 | 仍存在 | 暂缓。Service / Bridge 内部拆分要单独设计 |
| O-15–O-19 | 仍存在 | 暂缓。宽对象和派生数据收窄会改契约 |
| O-20 | mock Agent 用模块级可变状态 | 暂缓。已有 `resetMockAgentStatuses`；实例化隔离收益低于测试迁移成本 |
| O-21、O-23、O-25 | 仍存在 | 暂缓。Agent catalog、通用组件和展示配置需要先明确唯一 owner |
| O-24 | Sidebar Context 默认 setter 静默失效 | 已处理：缺少 Provider 时抛错 |
| O-22 | `GenericConfigForm` 未把锁定状态传给 `SecretInput` | 已处理：`SecretInput` 接收 `disabled`/`readOnly` |
| O-26–O-30 | 仍存在 | 暂缓。启动组合根、transport façade、Gateway 状态和协议策略需要分别设计 |
| O-31–O-34 | 仍存在 | 暂缓。Usage normalizer、查询 filter 和模型映射的跨层收窄需保持统计语义 |
| O-35–O-38 | 仍存在 | 暂缓。CLI 策略/预览与发布流程需要先统一结构化结果和脚本 owner |
| O-39 | mock `speaks` 与 core 对 GLM/DeepSeek 不一致 | 已处理：mock 补上 `openai-responses`。共享 fixture 仍未做 |
| O-40–O-44 | 仍存在 | 暂缓。mock、fixture 和测试 fake 需先补共享 contract，再收窄依赖 |

## 结论

当前代码已经有 `AgentHub`、各领域 Service、Backend port、运行时 store 和页面 model 等对象化基础，但在补查后仍存在以下主要缺口：

- 核心门面、数据库和 repository 的内部能力暴露过宽，外部可以绕过领域用例；
- 页面 Hook、Tauri controller 和部分 Service 同时承担状态、编排、持久化和补偿；
- 同一业务概念在多个对象中分别保存或派生，尤其是 Agent 状态、连接池、票夹和路线信息。

最高风险曾经是：调用方可以直接操作本应由领域对象维护的状态；写入成功后，多个前端读模型又可能各自刷新、静默失败或相互覆盖。票夹 bind/unbind 与账号 / Provider 变更的读模型刷新已收到 runtime coordinator；其余公开面和领域拆分仍待单独设计。

## 发现的问题

### 一、封装边界过宽

#### O-01｜`AgentHub` 暴露完整内部对象

- **严重程度：高**
- **位置：** `crates/agenthub-core/src/lib.rs:53-93` 的 `AgentHub`；`src-tauri/src/state.rs:79-87` 的 `hub_arc`
- **问题：** `data_dir`、`db`、`registry` 以及几乎所有 Service 都是 `pub`。拿到 `AgentHub` 的 CLI、Tauri 或其他模块可以直接访问数据库、注册表和 Service，绕过统一用例。
- **建议：** 将字段改为私有；通过按领域划分的窄接口暴露必要的查询和用例。`data_dir`、注册表等确需读取的能力也应提供只读方法，而不是暴露整个对象。
- **影响/风险：** 写入可能绕过锁、current 约束和补偿逻辑；替换内部实现时会扩大公开兼容面。

#### O-02｜`Database::with_conn` 和 `repo()` 泄漏持久化细节

- **严重程度：高**
- **状态：部分处理**
- **位置：** `crates/agenthub-core/src/storage/mod.rs:72-144`；`AccountService::repo()`；`ProviderService` 的 `repo()`
- **问题：** `Database::with_conn` 把原始 `rusqlite::Connection` 暴露给上层，多个 Service 又暴露 repository 引用。业务调用方可以绕过 Service 的领域校验、事务和状态补偿直接操作 SQLite。
- **当前：** 生产路径的 Provider 读取改为 `ProviderService::get_by_id` / `get_current`。Account / Backup 的 `repo()` 改为 `pub(crate)`。`with_conn` 仍是 crate 内事务原语，且 Tauri 测试会经过 `hub.db` 调用它；Provider `repo()` 仍给测试写行。
- **建议：** 让 SQL、事务和连接锁只留在 storage/repository 内部；Service 依赖窄的存储接口，并移除面向外部的 `with_conn` 与 `repo()` 暴露。
- **影响/风险：** `is_current`、binding、trash 等状态可能被分开修改；数据库迁移或存储替换的成本增大。

### 二、页面和跨层协调器职责过多

#### O-03｜Chat 页面 Hook 同时承载多个业务对象

- **严重程度：高**
- **位置：** `src/pages/chat/use-chat-page.ts:95-141`、`218-417`、`652-849`
- **问题：** `useChatPage` 同时维护会话、消息、Agent 状态、Provider、票夹、发送状态、流式过程、弹窗、项目跳转和竞态代数，并直接执行加载、创建、切换、发送、取消和恢复。
- **建议：** 拆为会话控制器、消息/流式发送控制器、连接信息查询 Hook 和页面交互状态 Hook；页面只负责组合和展示。
- **影响/风险：** 状态之间的竞态保护集中在一个 Hook，新增需求容易继续堆叠，测试难以区分领域行为和视图行为。

#### O-04｜Connections 页面仍是多业务流程的总编排器

- **严重程度：中高**
- **位置：** `src/pages/connections/index.tsx:145-592`
- **问题：** 一个页面同时维护筛选、票夹加载、连接池同步、登录探测、导入、路线引导、分享、切换、刷新、删除和详情面板状态；页面直接组合多个 account/provider/ticket 写入行为。
- **建议：** 将登录探测与导入、票据切换与绑定、删除确认和列表展示分别封装为功能内 Hook/model；页面只保留路由参数和组件编排。
- **影响/风险：** 一个流程的刷新、错误和取消语义容易影响其他流程；页面成为新的业务 Service。

#### O-05｜Provider 配置保存业务仍在页面目录

- **严重程度：高**
- **位置：** `src/pages/providers/providerSaveFlow.ts:373-503`
- **问题：** 页面侧决定 schema/legacy 分支，并执行 parse、project、validate、materialize、构造 Provider 和 upsert；`ConfigPort` 只提供低层原语。
- **建议：** 将“按 Agent schema 保存 Provider 配置”的完整用例放到 Backend/domain owner；页面只提交表单值并消费结构化结果。保留现有 projector 与 legacy 兼容路径，但统一入口。
- **影响/风险：** 配置不变量分散在页面、contract 和 core；其他入口保存同类配置时容易产生不同语义。

#### O-06｜Adapter 页面资源加载器重复拥有连接数据读取职责

- **严重程度：中**
- **状态：生产路径已收口**
- **位置：** `src/pages/bridges/adapter-resources.ts:26-31`、`107-155`、`174-180`（`bridges` 是源码中的历史兼容目录名，不代表当前用户入口）
- **问题：** 旧的 `AdapterResourceLoaders` 仍要求自行 `listAccounts`/`listProviders` 并合并连接项，后续接口又注明连接行来自共享连接池。
- **当前：** `useAdapterResources` 已从共享连接池取账号和 Provider；`loadAdapterPageResources` 只保留给测试。
- **建议：** 统一由共享连接池提供账号和 Provider；Adapter 资源加载器只负责 profile 与本机路由状态，逐步删除旧加载路径。
- **影响/风险：** 两个读取来源的刷新时序可能不同，导致重复请求或展示旧连接数据。

### 三、共享状态和写入后的行为归属不清

#### O-07｜运行时 store 是多个模块级可变单例，失效策略由调用方拼接

- **严重程度：中高**
- **位置：** `src/app/runtime/connection-pool-store.ts:27-39`、`156-274`；`ticket-wallet-store.ts:21-30`、`75-155`；`backend-runtime.ts:7-27`
- **问题：** snapshot、inflight、epoch、mutationDepth、listeners 和 reset 状态都存在模块级变量中；`setBackend`/`resetBackend` 还要手工依次重置多个 store。store 本身没有一个统一的运行时上下文来承载生命周期和失效关系。
- **建议：** 保留共享 store 作为应用级读模型，但由一个 runtime context/协调器统一管理实例、重置、失效和批处理；对外只暴露查询、订阅和领域级 mutation，而不是暴露多个底层状态操作函数。
- **影响/风险：** store 生命周期长于页面和 Backend 实例；新增读模型时容易遗漏 reset 或刷新关系。

#### O-08｜账号、Provider、票夹 façade 重复维护认证状态同步

- **严重程度：中高**
- **状态：已处理**
- **位置：** `src/app/runtime/mutation-coordinator.ts`；`src/lib/api/account.ts`；`src/lib/api/provider.ts`；`src/lib/api/tickets.ts`
- **问题：** account/provider 各自实现清探测缓存、刷新 Agent 状态、刷新连接池和刷新票夹；批量删除只抑制连接池刷新，票夹和 Agent 状态仍按条刷新。
- **当前：** `refreshRuntimeReadModels` 声明要刷新的读模型。账号 / Provider 变更走同一入口；`deleteProviders` 循环内不再逐条刷新票夹和 Agent 状态。
- **建议：** 后续若新增读模型，只扩 coordinator 的 model 列表，不要在 façade 里再写一套 notify。
- **影响/风险：** 新增读模型时容易只改一处；批量操作产生 N 次读取，异步刷新可能交错覆盖快照。

#### O-09｜票据写入成功后的读模型刷新错误被静默吞掉

- **严重程度：高**
- **状态：已处理**
- **位置：** `src/lib/api/tickets.ts`；`src/app/runtime/mutation-coordinator.ts`
- **问题：** bind/unbind 成功后，连接池和票夹刷新都使用 `.catch(() => {})`。调用方收到成功结果，但页面可能继续看到旧的连接和绑定状态。
- **当前：** coordinator 用 `Promise.allSettled` 刷新；写入成功仍返回给调用方，失败留在连接池 / 票夹 snapshot 的 `error` 字段。连接页和 Chat 都展示该错误并可重试。尚未做自动重试。
- **建议：** 区分“写入成功”和“读模型刷新失败”；至少将刷新状态交给 runtime 统一重试/提示，或返回可观察的刷新结果。
- **影响/风险：** 后端真实状态与多个页面的显示状态不一致，且没有明确的用户重试信号。

#### O-10｜Chat 页面绕过共享票夹 store 保存本地钱包

- **严重程度：中**
- **状态：已处理**
- **位置：** `src/pages/chat/use-chat-page.ts`
- **问题：** Chat 自己维护 `wallet` 并直接调用 `listTicketWallet`，与 `src/app/runtime/ticket-wallet-store.ts` 的共享快照并存。
- **当前：** Chat 使用 `useTicketWallet`，只按当前 Agent 派生连接选择项。
- **建议：** 统一使用 `useTicketWallet`/共享 snapshot；页面只按当前 Agent 派生连接选择项。
- **影响/风险：** 本地快照与共享快照可能不一致，并产生重复请求和额外竞态处理。

### 四、领域 Service 和对象职责过宽

#### O-11｜`ProviderService` 同时负责池 CRUD、投影、切换和补偿

- **严重程度：高**
- **位置：** `crates/agenthub-core/src/services/provider_service.rs:74-84`、`140-177`、`229-251`、`1441-1511`
- **问题：** 一个 Service 同时持有 repository、registry、backup、live-write authority、ConnectionService 和 secret resolver，并处理列表修复、Provider CRUD、身份合并、live 配置写入、current 切换、binding 快照和补偿。
- **建议：** 以 `ProviderPool`、`ProviderProjection`、`ProviderLiveSwitch` 等职责拆分内部 owner；保留 `ProviderService` 作为窄的应用门面，`ConnectionService` 继续唯一拥有 current 指针。
- **影响/风险：** 任一池数据变更都可能牵动文件写入、备份和 binding；事务边界难以单独测试。

#### O-12｜`AccountService` 的账号池、实时同步、OAuth 和切换职责仍聚合在一个对象

- **严重程度：中高**
- **位置：** `crates/agenthub-core/src/services/account_service/mod.rs:8-14`、`65-144`
- **问题：** `AccountService` 通过子模块同时承载 pool CRUD、live import/reconcile、OAuth 文件同步、授权归属、切换 saga，并持有 registry、backup、锁目录和 ConnectionService。
- **建议：** 先按“账号池读写”“本机登录同步”“切换/补偿”划分内部用例对象；外部保留 `AccountService` 兼容门面，明确每个方法的事务和锁 owner。
- **影响/风险：** 账号池状态、文件状态和 current 状态需要多个子模块共同维护，容易出现刷新后不一致。

#### O-13｜`BackupService` 把索引、文件系统策略、快照、恢复和删除补偿集中在一个对象

- **严重程度：中高**
- **位置：** `crates/agenthub-core/src/services/backup_service.rs:109-180`、`187-309`、`311-564`、`740-805`
- **问题：** 同一个 Service 负责 BackupRepo、路径安全、manifest、增量复制、快照、PreRestore、恢复计划、文件替换回滚和 tombstone 删除。
- **建议：** 将 Backup catalog/index、snapshot materializer、restore transaction 和 filesystem safety policy 作为内部组件；由 `BackupService` 保留统一锁和补偿边界。
- **影响/风险：** 安全校验、文件系统副作用和数据库补偿互相耦合，修改一个流程时容易破坏另一个流程的恢复语义。

#### O-14｜Bridge 业务事务分散在 Core Service 和 Tauri controller

- **严重程度：中高**
- **位置：** `crates/agenthub-core/src/services/adapter_bridge_service/mod.rs:903-1021`；`src-tauri/src/adapter_bridge_controller.rs:997-1059`、`1263-1365`
- **问题：** `AdapterBridgeService` 同时持有 route、profile、provider、secret 和 route-pool 对象；Tauri controller 又直接编排 ticket wallet、成员认证、Provider 更新/切换、profile 持久化和回滚。
- **建议：** 保留 Tauri 只负责参数转换、线程调度和 listener 生命周期；把“准备、启动后持久化、恢复端口、失败补偿”封装为一个可复用的 Bridge application use case。
- **影响/风险：** 一次 Bridge 操作的事务边界分布在两层，CLI 与 GUI 难以共享相同补偿路径。

### 五、对象模型和派生数据不足

#### O-15｜`AgentStatus` 混合检测、登录、连接、进程、能力和界面偏好

- **严重程度：中高**
- **位置：** `src/lib/types.ts:99-153`；`src/app/runtime/agent-status-store.ts:29-42`、`163-239`、`243-349`
- **问题：** 一个状态对象同时承载安装检测、版本/进程、登录健康、当前连接、环境、能力、更新信息和 UI 隐藏偏好；store 还负责 live auth probe 和连接池合并。
- **建议：** 分为 `AgentInstallationStatus`、`LiveAuthSnapshot`、`EffectiveConnectionSummary`、`AgentCapabilitySnapshot` 和 UI visibility preference，再由页面 read model 组合。
- **影响/风险：** 不同来源、刷新周期和失败语义混在同一对象中，容易把“未加载”“未登录”和“没有连接”混为一谈。

#### O-16｜前端 `Account`/`Provider` 同时表示持久化行、实时状态和展示摘要

- **严重程度：中**
- **位置：** `src/lib/types.ts:156-249`
- **问题：** `Account` 混合账号池身份、实时认证探测、配额、来源、登录信息摘要和展示字段；`Provider` 也同时包含配置文本、current、延迟、官方模式和登录信息摘要。
- **建议：** 拆为 `SavedAccount`/`SavedProvider`、`LiveAuthSnapshot`、`QuotaSummary` 和展示 read model，通过映射函数组合；不要让页面直接把一个宽对象当作全部事实。
- **影响/风险：** 过期的实时字段可能被误当成持久化真相，状态刷新边界不清。

#### O-17｜绑定路线存在平行枚举和多处标签映射

- **严重程度：中**
- **位置：** `src/lib/backend/contracts/ticket.ts:29-30`、`450-462`；`src/pages/connections/ticket-wallet-model.ts:648-678`
- **问题：** 绑定对象使用 `native | reshape | bridge`，计划/页面又使用 `native_endpoint | config_sync | local_bridge | unsupported`，标签分散在 contract 和页面 model。
- **建议：** 集中定义 route plan、binding route 和用户展示语义之间的转换；页面只消费统一的 route view model。
- **影响/风险：** 新增路线要同步修改多个层，类型系统无法阻止不同语义互相传递。

#### O-18｜票夹 `surfaceGroups` 有多个派生者

- **严重程度：中**
- **位置：** `src/lib/backend/contracts/ticket.ts:285-329`；`src/dev/mocks/ticket.ts:354-383`
- **问题：** `TicketWallet` 同时携带 `tickets` 和派生的 `surfaceGroups`；wire 缺失时前端 fallback 分组，mock 又自行按账号/Provider 分类生成。
- **建议：** 明确唯一 owner：若它是派生数据，只保留 tickets 并由一个 mapper 生成；若它是后端 read model，则 mock 和前端不得重复推导。
- **影响/风险：** 健康状态、未知 surface、排序和分组规则可能在后端、前端和 mock 之间漂移。

#### O-19｜核心 Account/Provider 的配置字段仍是任意 JSON

- **严重程度：中**
- **位置：** `crates/agenthub-core/src/models/account.rs:49-63`；`models/provider.rs:54-72`；`services/account_service/oauth_owner.rs:36-58`
- **问题：** `credentials`、`extra`、`settings_config` 和 `meta` 使用 `serde_json::Value`，领域行为依赖散落的字符串键，数据和不变量没有被对象类型承载。
- **建议：** 保留各 Agent 外部文件格式兼容，在 core 内引入按 Agent/kind 区分的 typed value object；持久化和传输再映射为脱敏 DTO。
- **影响/风险：** 字段拼写、结构和状态不变量只能运行时发现，外部调用方容易构造核心无法正确解释的对象。

#### O-20｜Mock Agent 用模块级可变对象承载状态

- **严重程度：低**
- **位置：** `src/dev/mocks/agent.ts:148`、`308-362`
- **问题：** mock 的模块级 `state` 被安装、升级、卸载方法直接改写；测试和调用方共享同一个可变状态容器。
- **建议：** 将状态封装在 `createMockAgentPort` 实例内，统一通过 `updateState`/不可变更新和 reset 管理；让测试显式创建隔离实例。
- **影响/风险：** 测试之间可能残留修改，调用方也可能观察到跨场景的非预期状态。

### 六、前端公共层和配置对象边界不足

#### O-21｜Agent 配置以可变集合公开，绕过目录状态 owner

- **严重程度：高**
- **位置：** `src/config/agents.ts:40-52`、`89-114` 的 `AGENTS`、`AGENT_MAP`、`AGENT_IDS`
- **问题：** 共享 Agent 配置以可变数组和对象导出，外部可以直接修改集合内容；同时运行时 Agent catalog 也在提供产品 Agent 集合。
- **建议：** 将集合设为模块私有，通过只读快照和查询函数访问；更新只允许由 catalog 同步入口整体替换并通知订阅者。
- **影响/风险：** 页面可能看到不同的 Agent 顺序、能力或显示信息，形成静态配置与运行时 catalog 的双重来源。

#### O-22｜通用配置表单计算出的锁定状态没有传到密钥输入对象

- **严重程度：高**
- **状态：已处理**
- **位置：** `src/components/shared/GenericConfigForm.tsx`；`src/components/shared/SecretInput.tsx`
- **问题：** `GenericConfigForm` 已计算 `disabled/readOnlyKeys`，但渲染 `SecretInput` 时没有传递禁用/只读状态，密钥输入仍可编辑或操作显示切换。
- **当前：** `SecretInput` 接收 `disabled`/`readOnly`；锁定时输入和显示切换都不可用。
- **建议：** 让 `SecretInput` 明确承载 `disabled/readOnly`，由表单统一传递字段状态；保存期间和官方模式的锁定规则只在表单状态 owner 中维护。
- **影响/风险：** 本应锁定的登录信息仍可被修改，导致误提交或显示状态与实际保存语义不一致。

#### O-23｜`GenericConfigForm` 和 `Sidebar` 名义通用，实际聚合业务规则

- **严重程度：中**
- **位置：** `src/components/shared/GenericConfigForm.tsx:31-51`、`165-167`；`src/components/layout/Sidebar.tsx:145-194`、`276-336`
- **问题：** 通用表单直接携带密钥标记、Provider model 建议和 Connections 文案；Sidebar 同时负责导航、路由可见性、catalog 排序、隐藏状态、安装统计、更新标记和 Agent 状态展示。
- **建议：** 将字段渲染器/密钥策略与业务 schema 分开；将 Sidebar 的导航模型、Agent 状态条和安装统计拆为功能内 model/子组件，Sidebar 只组合布局。
- **影响/风险：** 共享组件逐渐变成业务 owner，任一业务状态变化都会扩大 UI 重算和回归范围。

#### O-24｜Sidebar Context 的默认 setter 静默失效

- **严重程度：中**
- **状态：已处理**
- **位置：** `src/components/layout/SidebarContext.tsx`
- **问题：** Context 默认值包含 no-op setter。组件未包裹 `SidebarProvider` 时不会报错，而是表现为“操作成功但状态不保存”。
- **当前：** 缺少 Provider 时 `useSidebar()` 抛错。应用根已包裹 `SidebarProvider`。
- **建议：** 默认值改为 `undefined`，`useSidebar()` 在缺少 Provider 时显式抛错；状态写入由 Provider 统一拥有。
- **影响/风险：** Provider 层级错误被隐藏，调用方无法判断自己是否真正连接到状态 owner。

#### O-25｜Agent ID 集合和展示/安装元数据存在第二套静态来源

- **严重程度：中**
- **位置：** `src/styles/tokens.ts:19-31`；`src/config/agents.ts:40-52`、`68-82`
- **问题：** token 层维护 `TOKEN_AGENT_IDS`，配置层维护 `AGENT_DISPLAY`，运行时又有 catalog；`agentMetaFromCatalogEntry` 还把可执行 `command` 混入 UI `AgentMeta`。
- **建议：** token 层只提供已知 Agent 的颜色查询和 fallback，不再维护产品集合；展示 meta 与安装渠道/命令 DTO 分离。
- **影响/风险：** 新 Agent 或新安装渠道需要同时更新多处，展示对象可能间接携带不应由 UI 处理的执行信息。

### 七、Bridge、协议和 Usage 模块的职责混合

#### O-26｜`AgentHub` 启动函数同时承担组合根、恢复和业务初始化

- **严重程度：中高**
- **位置：** `crates/agenthub-core/src/lib.rs:54-93`、`106-210`
- **问题：** `AgentHub` 同时持有近 30 个领域对象；`open_with_skills_root` 还直接处理数据库恢复、Adapter 注册、服务构造、技能恢复和配置启动。
- **建议：** 将启动恢复、依赖装配和业务初始化拆成明确的 composition/bootstrap 对象；`AgentHub` 只作为组装后的窄门面。
- **影响/风险：** 测试替换依赖困难，启动策略与领域 Service 的生命周期互相耦合。

#### O-27｜Registry 与 Catalog 同时承载 Agent 来源

- **严重程度：中**
- **位置：** `crates/agenthub-core/src/lib.rs:54-60`、`126-127`
- **问题：** `AgentHub` 同时持有 Adapter registry 和由 registry 派生的 Catalog，外部有两个 Agent 入口：一个偏行为，一个偏目录元数据。
- **建议：** 明确 registry 是行为 owner，catalog 是只读投影；由单一装配/同步入口生成 catalog，外部不要同时取得两个可变对象。
- **影响/风险：** 后续分别增加缓存或能力字段时，Agent 行为和目录信息可能不同步。

#### O-28｜`UpstreamChannel` 重复承担协议类型、分派和传输 façade

- **严重程度：中**
- **位置：** `crates/agenthub-core/src/bridge/host/transport/mod.rs:109-124`、`137-237`
- **问题：** 同一模块先把协议映射为枚举，再通过枚举转发到具体 transport；`protocol()`、`path()` 还出现未使用的兼容抽象，说明类型/传输边界未稳定。
- **建议：** 明确“协议选择”和“具体传输实现”只有一个 owner；通过窄 trait 或已解析的 transport 对象交给 host，移除重复转发层。
- **影响/风险：** 新增上游协议需要修改多套分派逻辑，行为和测试容易漏改。

#### O-29｜Gateway/EdgeState 把运行时生命周期和请求路由状态集中到共享可变对象

- **严重程度：中高**
- **位置：** `crates/agenthub-core/src/bridge/host/gateway.rs:93-104`、`361-455`
- **问题：** `Gateway` 通过 `Arc<Mutex<GatewayRegistry>>` 同时管理 socket、runtime 和 primary port；`EdgeState` 又混合 URL、HTTP client、停止标志、并发、健康状态、账号选择、模型映射和 route index。
- **建议：** 分离 listener/runtime registry、edge 配置、请求级路由上下文和健康/选择状态；通过明确的生命周期对象管理启动、关闭和重载。
- **影响/风险：** 并发请求、关闭、重载和健康更新共享同一状态边界，后续修改容易互相影响。

#### O-30｜Protocol 解析器混入供应商策略

- **严重程度：中**
- **位置：** `crates/agenthub-core/src/bridge/protocol/responses.rs:1669-1789`
- **问题：** 同一组函数同时负责 Responses 输入校验、函数调用分组、能力拒绝、错误码选择和 Kimi 格式转换；`developer -> system` 等供应商策略与通用解析绑定在一起。
- **建议：** 分为外部协议 parser、内部 IR normalizer 和供应商 transport policy；供应商字段改写由具体 transport/策略对象负责。
- **影响/风险：** 修改某一供应商语义时容易改变通用协议解析，能力拒绝和转换测试边界不清。

#### O-31｜Bridge `Usage` 的协议映射分散在多组方法

- **严重程度：中**
- **位置：** `crates/agenthub-core/src/bridge/types.rs:142-219`、`660-719`
- **问题：** Chat、Responses、Anthropic 的 usage 解析和生成分散在多个方法，`reasoning_tokens` 又由独立兼容函数处理；默认值、字段映射和 total 计算没有单一行为 owner。
- **建议：** 先由一个 `UsageSummary`/normalizer 形成统一语义，再由各协议 serializer 输出；补充字段时只改一个归并 owner 和各协议薄映射。
- **影响/风险：** 新增 usage 字段时容易只更新部分协议，导致不同客户端看到不一致的统计。

#### O-32｜Usage 查询入口重复维护过滤和聚合规则

- **严重程度：中**
- **位置：** `crates/agenthub-core/src/services/usage_service.rs:130-160`、`256-308`、`311-435`
- **问题：** `query`、`trend`、`overview` 分别拼接时间、Agent、model、exclude 等 SQL 条件和参数，部分 helper 不能保证三条路径完全一致。
- **建议：** 抽出不可变的 `UsageQuery`/filter object，由 repository 负责统一过滤；Service 只组合 query、trend 和 overview 的不同聚合策略。
- **影响/风险：** 统计口径修改时可能漏改某个入口，尤其 overview 的 models 与 metrics 查询容易漂移。

#### O-33｜Usage 模型混合存储记录、日志解析中间态和展示计费语义

- **严重程度：中**
- **位置：** `crates/agenthub-core/src/models/usage.rs:11-29`、`132-158`
- **问题：** `UsageRecord` 同时承载数据库字段和计费语义，`ParsedUsageEvent` 又是日志解析中间结构，并包含 cache 拆分和原始 hash。
- **建议：** 分离 `StoredUsageRecord`、`ParsedUsageEvent`、`UsageSummary`；解析和计费归并通过明确的转换对象完成。
- **影响/风险：** 存储字段、解析兼容和展示统计互相牵制，数据与行为的变化边界不清。

#### O-34｜模型映射模块承担 Bridge 运行时切换决策

- **严重程度：中**
- **位置：** `crates/agenthub-core/src/models/adapter_model_mapping.rs:452-522`
- **问题：** `decide_model_switch` 不只是模型映射，还依据目标 Agent、surface、运行状态和候选 edge 决定切换；模型模块因此知道 Bridge runtime 概念。
- **建议：** models 只提供模型映射和候选值；由 Bridge route policy/selector 读取运行状态并决定是否切 edge。
- **影响/风险：** 领域模型层反向依赖运行时策略，模型映射难以独立复用和测试。

### 八、CLI、脚本和测试辅助层的职责漂移

#### O-35｜CLI `run` 命令重复承担运行策略

- **严重程度：中**
- **位置：** `crates/agenthub-cli/src/commands/run.rs:28` 的 `run`、`resolve_agents`
- **问题：** CLI 自己决定跳过缺失 Agent、全部跳过时的失败判定、危险运行参数和默认 Agent 筛选，而 core 只接收最终参数。
- **建议：** 将运行策略、跳过规则和结果判定收敛到 core 的运行用例/策略对象；CLI 只解析参数和呈现结果。
- **影响/风险：** GUI、CLI 或未来入口可能对同一运行行为产生不同判定。

#### O-36｜CLI 切换确认直接读取领域状态并吞掉错误

- **严重程度：中高**
- **位置：** `crates/agenthub-cli/src/commands/account.rs:118`；`provider.rs:171` 的 `switch_confirm_prompt`
- **问题：** 确认提示函数直接读取 Account/Provider 列表并用 `.ok()` 忽略数据库错误，同时自行拼接备份目录、当前项和进程影响说明。
- **建议：** 由 core 返回结构化切换预览（当前项、备份计划、进程影响、警告）；CLI 只渲染，读取失败必须显式返回。
- **影响/风险：** 数据库异常时可能显示不准确的确认信息，用户在错误认知下执行切换。

#### O-37｜CLI 多个命令重复维护 Agent 选择和确认行为

- **严重程度：低**
- **位置：** `crates/agenthub-cli/src/commands/account.rs:12`、`provider.rs:17`、`skill.rs:8`
- **问题：** account/provider/skill 各自实现 Agent 过滤、必选校验、确认文案和输出分支。
- **建议：** 抽出 CLI 级 `AgentSelection`、确认上下文和统一结果呈现对象；领域规则仍留在 core。
- **影响/风险：** 参数语义、错误信息和确认行为逐渐漂移。

#### O-38｜本地发布脚本与 CI 重复维护同一发布业务

- **严重程度：中**
- **位置：** `scripts/release-update.ps1:690`；`scripts/build-latest-json.mjs:198`；`.github/workflows/release.yml:730`
- **问题：** 本地 PowerShell 与 CI workflow 分别维护产物发现、平台映射、签名检查、清单生成和发布状态处理。
- **建议：** 将产物发现、平台映射、签名完整性和清单生成收敛到一个可测试脚本；本地入口和 workflow 只负责环境编排。
- **影响/风险：** 本地检查与 CI 发布行为可能不一致，新增平台时容易只更新一套流程。

#### O-39｜测试 Mock 与生产 `speaks` 规则已经漂移

- **严重程度：中高**
- **状态：已处理（本条漂移）**
- **位置：** `src/dev/mocks/ticket.ts`；`crates/agenthub-core/src/models/ticket.rs`
- **问题：** mock 对 `glm-coding-plan`/`deepseek-api` 只返回两种协议，生产还支持 `openai-responses`；mock 与生产对同一连接的可用协议判断不一致。
- **当前：** mock 已与 core 对齐，补上 `openai-responses`。共享 fixture / contract test 仍未做。
- **建议：** 由 core 生成可供测试使用的只读能力 fixture，或增加共享 contract test 强制校验 mock 与生产的 `speaks` 结果一致。
- **影响/风险：** 浏览器测试通过但真实后端不可用，或反向把不可用路线展示为可用。

#### O-40｜Mock 重复实现来源分类规则

- **严重程度：中高**
- **位置：** `src/dev/mocks/ticket.ts:121-245`；`src/dev/mocks/adapter/types.ts:59-117`；生产入口 `crates/agenthub-core/src/services/adapter_route_service/classify.rs:21-100`
- **问题：** mock 多处重复维护 Kimi、Anthropic、OpenAI、xAI、GLM、DeepSeek 的 preset、endpoint 和登录信息判断，生产侧另有一套实现。
- **建议：** mock 只读取共享分类 fixture/contract；规则判断由 core 的 plan/classify 结果提供，不在 mock 内复制业务分支。
- **影响/风险：** 新增来源或路线时测试可能继续通过，但与真实后端分类不一致。

#### O-41｜连接流程 fixture 手工构造完整绑定结果

- **严重程度：中**
- **位置：** `src/dev/mocks/connect-flow-fixtures.ts:85-182`
- **问题：** fixture 直接硬编码 Provider、AdapterProfile、ruleId、route、端口、endpoint 和运行状态，绕过 plan/apply 流程，实质上维护一套“绑定成功后的规则”。
- **建议：** fixture 只描述输入和最小观察结果，绑定结果由共享 builder 或 contract fixture 生成；避免用 `as Account` 补生产类型未声明的 `extra/credentials` 字段。
- **影响/风险：** 字段、规则版本或路由语义变化后，测试数据会静默失真。

#### O-42｜Mock Ticket resolver 依赖过宽的内部拼装接口

- **严重程度：中**
- **位置：** `src/dev/mocks/ticket.ts:32-44`
- **问题：** resolver 直接读取 accounts/providers/profiles/桥状态，并可调用 `planAdapter`、`applyAdapter`、`removeBinding`；测试辅助层穿透了 ticket/wallet port。
- **建议：** 让 resolver 只依赖稳定的 ticket/wallet 读写接口；Adapter 行为通过注入的窄测试 double 提供。
- **影响/风险：** 生产模型封装变化会迫使测试辅助层同步修改，mock 不再能代表稳定 contract。

#### O-43｜测试 fake 用 wildcard 掩盖能力枚举扩展

- **严重程度：低**
- **位置：** `crates/agenthub-core/src/services/adapter_apply_service/tests.rs:909-913`
- **问题：** fake 对 Capability 使用 `_ => unsupported`，生产新增能力时编译器会提示真实实现，但测试 fake 不会被迫更新。
- **建议：** 测试 fake 对能力枚举使用显式穷举；新增能力时让测试模型和生产实现同时更新。
- **影响/风险：** 测试覆盖模型可能静默落后，无法发现新增能力没有正确接入。

#### O-44｜OAuth 测试直接操作共享 store，清理不是结构化作用域

- **严重程度：中**
- **位置：** `crates/agenthub-core/src/oauth/device/tests.rs:8-35`、`121-167`
- **问题：** 测试通过唯一字符串和末尾手动 `remove` 操作模块级共享状态；断言 panic、早退或并行复用时，清理可能无法执行。
- **建议：** 让 store 支持测试作用域 guard/实例级注入，测试结束自动恢复；不要让用例直接持有全局 map 的锁和写权限。
- **影响/风险：** 用例之间可能互相污染，测试顺序和并行度影响结果。

## 归属建议

| 业务概念 | 当前散落位置 | 建议的主要 owner |
| --- | --- | --- |
| current/active 连接 | Account、Provider、Ticket、多个 runtime store | `ConnectionService`；其他对象只读投影 |
| 写入后刷新与批处理 | account/provider/ticket façade、页面回调、多个 store | runtime mutation coordinator |
| Provider 配置保存 | Provider 页面、Config port、core projector | Backend/domain save use case |
| 票夹与 surface 分组 | core read model、contract mapper、mock、页面 fallback | 一个 ticket read-model mapper |
| Agent 安装/登录/连接摘要 | `AgentStatus`、Account、连接池 | 分离的状态对象 + 页面 read model |
| Agent 集合与展示/安装元数据 | `src/config/agents.ts`、`src/styles/tokens.ts`、运行时 catalog | catalog 为唯一集合 owner；token 和 UI meta 只做只读映射 |
| Bridge 绑定事务 | Core bridge service、ProviderService、Tauri controller | Core application use case；Tauri 仅托管运行时生命周期 |
| Bridge 协议与 Usage | transport façade、protocol parser、Usage model/service | 协议 parser、供应商策略、Usage normalizer 和查询 filter 分层 |
| CLI 运行与切换预览 | CLI command 模块、core Service | core 返回结构化策略/预览；CLI 只解析和呈现 |
| Backup 文件与索引 | 一个 BackupService | Backup catalog、文件安全策略、snapshot/restore transaction 内部组件 |

## 低风险调整顺序

以下是建议的迁移顺序，不等同于已批准的实施任务：

1. 先收窄 `AgentHub`、`Database` 和 repository 的公开面，补调用方清单与契约测试，保持现有 façade 行为。Provider 生产读取已离开 `repo()`；`with_conn` 与 `AgentHub` 字段仍待单独清单。
2. ~~建立统一 runtime mutation coordinator，先接管 ticket bind/unbind 和 Provider 批量删除，解决静默刷新失败与重复刷新。~~ 已落地 `refreshRuntimeReadModels`；尚未做刷新失败的自动重试或页面提示。
3. ~~让 Chat 使用共享票夹；让 Adapter 页面只消费共享连接池。~~ 已落地。旧的 `loadAdapterPageResources` 仍作测试辅助。
4. 在不改变 current、锁和补偿语义的前提下，分别为 Provider、Account、Backup 和 Bridge 做内部组件拆分；保留兼容门面。
5. 最后收窄宽对象和重复派生：Agent/Account/Provider read model、route mapping、surface grouping、typed config value object，以及 mock 状态实例化。

补查范围中没有发现 CLI 自建的进程级可变全局状态；CLI 通常通过单个 `AgentHub` 实例调用 core。`src/main.tsx`、`src/App.tsx` 和 `src/lib/utils.ts` 主要承担启动、页面组合和通用工具职责，未发现比上述条目更明确的独立对象化缺陷。

## 已确认合理的边界

- `src/lib/backend/tauri/invoke.ts` 集中 Tauri 调用本身符合当前前后端边界，不属于本次问题。
- `app/runtime` 持有共享读模型是有意设计；本次问题是其模块级可变状态、多个 reset/失效入口和外部协调策略过多，不是要求移除共享 store。
- `ConnectionService` 作为 current/active 连接 owner 的方向是正确的；需要做的是减少其他对象对 current 状态的镜像和直接写入。
- `BackupService` 的路径校验、锁和补偿属于必要安全行为；建议是内部拆分，不能削弱现有 fail-closed、快照和回滚语义。
