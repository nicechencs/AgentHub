---
title: AgentHub 命名规范审查报告
type: status
status: current
owner: maintainers
updated: 2026-09-02
---

# AgentHub 命名规范审查报告

本文记录 2026-09-02 对 AgentHub 当前代码库进行的命名规范审查，覆盖功能、页面、组件、控件、代码符号、架构模块、API、数据模型、配置、事件、状态、文件和目录。本文只记录当前事实与候选整改建议，不表示这些建议已经实施。

## 结论

项目整体命名规范成熟，没有系统性的大小写或格式混乱。确认 16 组值得处理的问题：

- 高严重度：4 组。
- 中严重度：10 组。
- 低严重度：2 组。

主要问题不是代码格式，而是 Routes 架构演进后遗留的历史名称，以及少数名称与实际删除粒度、运行对象、数据来源或用户概念不一致。

优先处理以下四项：

1. `deleteAgentProject` 实际删除会话。
2. 用户界面仍出现“凭据”等非项目规范词。
3. `connection-pool-store` 与真正的 `RoutePool` 共用“连接池”名称。
4. Routes 产品面仍被泛称为 `bridges`。

## 审查方法与范围

本次审查采用多视角独立检查：

1. 代码规范。
2. 业务语义。
3. 架构模块。
4. UI 页面与组件。
5. API、数据模型和配置。
6. 跨模块、文件和目录一致性。
7. 全仓词汇与历史名称盘点。

首轮由 7 个 Terra Agent 独立检查；随后由另一位 Terra Agent 对候选项进行反向审查，按“是否是真实命名问题、是否符合项目既有约定、收益是否值得迁移风险、是否与其他问题重复”四项裁决。主审再结合调用关系和源码证据汇总。

审查范围包括 `src/`、`src-tauri/`、`crates/` 及相关测试、配置和现行文档。依赖目录、构建产物、生成文件和归档文档不作为当前规范问题来源，除非它们能证明兼容关系。

审查依据以项目自己的 [术语表](../reference/terminology.md) 和 [架构总览](../architecture/overview.md) 为主，不机械套用通用命名规则。

本次审查没有修改生产代码。

## 当前项目命名规范

### TypeScript 与 TSX

- 组件、接口和类型使用 `PascalCase`。
- 变量、函数和属性使用 `camelCase`。
- Hook 使用 `useXxx`。
- 常量使用 `SCREAMING_SNAKE_CASE`。
- 后端能力边界使用 `*Port`，跨层数据使用 `*Wire`、`*Dto`，映射函数使用 `mapXxx`。
- 异步函数以业务动作命名，例如 `list…`、`start…`、`refresh…`，不机械增加 `Async`。
- 公共组件文件多使用 PascalCase；页面内部模块多使用 kebab-case；Hook 文件保留 `useXxx` 命名。

### Rust

- 类型、Trait 和枚举使用 `PascalCase`。
- 函数、字段和模块使用 `snake_case`。
- 常量使用 `SCREAMING_SNAKE_CASE`。
- `*Service` 表示业务编排，`*Repo` 表示 SQLite 存取，`*Adapter` 表示针对 Agent 的集成实现。

### 跨层契约

- Rust command 使用 `verb_object`，TypeScript 包装使用 `verbObject`。
- Rust DTO 通过序列化层转换为 TypeScript 的 `camelCase` 字段。
- 生命周期事件普遍使用 kebab-case，例如 `install-progress`、`skills-fs-changed`、`tray-navigate`。
- Tauri 与 mock 层使用成对文件和同名 Port，属于有意的接口对称。

### 用户词汇与内部词汇

- 用户界面使用“登录、官方登录、API Key、供应商、Routes、路由、本机转发、连接池、共享库、会话”。
- `Account`、`Provider`、`Ticket`、`Binding`、`AdapterProfile`、`local_bridge` 可以作为内部实现名。
- `Ticket`、`TicketWallet` 和 `Binding` 不应直接展示给用户，但不需要仅为统一表面用词而全仓重命名。

## 严重程度与优先级

严重程度表示名称可能造成的误解；优先级表示建议安排整改的先后顺序。两者不完全相同：涉及数据库或 IPC 的中严重度问题，迁移风险可能高于仅修改文案的高严重度问题。

| 等级 | 含义 |
| --- | --- |
| 高 | 名称与真实对象或操作明显不符，或者直接违反项目用户术语 |
| 中 | 跨层语义漂移、历史名称或兼容字段会持续增加理解成本 |
| 低 | 局部一致性或可读性问题，不影响当前业务含义 |

| 优先级 | 建议 |
| --- | --- |
| P0 | 立即安排，通常风险低或误导后果明显 |
| P1 | 下一轮相关功能整改 |
| P2 | 在契约或兼容迁移窗口处理 |
| P3 | 随相关模块修改顺带清理 |

## 问题清单

### N-01：删除 API 把会话称为项目

- 严重程度：高。
- 优先级：P0。
- 当前名称：`delete_agent_project`、`deleteAgentProject`。
- 推荐名称：`delete_agent_session`、`deleteAgentSession`。

[Tauri command](../../src-tauri/src/commands/project.rs) 的实现和注释都表明按 ID 删除的是 `AgentSession`，而 [领域模型](../../crates/agenthub-core/src/models/project.rs) 中 `AgentProject` 与 `AgentSession` 是不同实体。由于这是破坏性操作，名称可能让调用者误判删除粒度。

影响范围包括 Rust command、TypeScript Port、API façade、mock、项目页面调用方以及可能存在的外部 Invoke 自动化。建议新增规范 IPC 名，并至少保留一个版本的旧名兼容入口；不涉及数据迁移。

### N-02：用户界面仍使用“凭据”，面板名称也与职责不符

- 严重程度：高。
- 优先级：P0。
- 当前名称：“凭据展示”“凭据保存在……”“版本与凭据说明”、`SecurityPanel`、`settings.security.*`。
- 推荐名称：“API Key 与登录信息展示”“API Key 与登录信息保存在……”“版本与登录信息说明”、`LoginInformationPanel`、`settings.loginInfo.*`。

[SecurityPanel](../../src/pages/settings/LoginInformationPanel.tsx) 实际只展示登录信息和 API Key 的遮罩、保存位置说明，并被放在 About 页面中，不是完整的安全设置面板。[中文文案](../../src/lib/i18n/locales/zh.ts) 又直接使用了术语表不建议面向用户展示的“凭据”。

影响范围是 About/设置页、中英文翻译和相关 UI 测试。旧的 `?tab=security` 可以继续作为 URL 兼容映射保留。

### N-03：前端全量连接缓存与真正的连接池同名

- 严重程度：高。
- 优先级：P1。
- 当前名称：`connection-pool-store.ts`、`ConnectionPoolSnapshot`、`loadConnectionPool` 等。
- 推荐名称：`connection-inventory-store.ts`、`ConnectionInventorySnapshot`、`loadConnectionInventory` 等。

[connection-pool-store.ts](../../src/app/runtime/connection-inventory-store.ts) 实际缓存的是全部 `accounts`、`accountViews` 和 `providers`，并不持有 `RoutePool`。项目中真正的产品“连接池”已有 [RoutePool 模型](../../crates/agenthub-core/src/models/route_pool.rs) 和 [RoutePoolService](../../crates/agenthub-core/src/services/route_pool_service.rs)。

同一名称指向两类对象，会让调用者把前端读取缓存误解为 Routes 的连接池控制面。首轮扫描发现约 12 个直接文件、89 处相关标识符。建议先增加兼容导出，再分批迁移调用方。

### N-04：Routes 产品面仍被泛称为 bridges

- 严重程度：高。
- 优先级：P1。
- 当前名称（历史命名，当前仅应作为兼容范围）：`bridges-path.ts`、`BRIDGES_PATH`、`BRIDGES_NAV_LABEL`、`bridgesHrefForProfile`、泛用 `pages/bridges/`。
- 推荐名称：`routes-path.ts`、`ROUTES_PATH`、`ROUTES_NAV_LABEL`、`routesHrefForProfile`；泛用页面模块迁入 `pages/routes/shared/`。

[bridges-path.ts](../../src/lib/bridges-path.ts) 中常量的实际值是 `/routes`，导航文本也是 Routes；[App.tsx](../../src/App.tsx) 已把 `/bridges` 作为旧地址重定向。`bridge` 只是 `local_bridge` 这一种路由方式，不等同于整个 Routes 产品面。

影响导航、深链、Routes 页面、页面 façade、导入路径和测试。`local_bridge` 生命周期、`useBridgeRuntimeActions` 及旧地址兼容跳转仍可保留 `bridge`；不应机械重命名所有包含 bridge 的符号。

### N-05：同一本机运行入口存在 LocalEntry、Gateway 和 relay 三套称呼

- 严重程度：中。
- 优先级：P1。
- 当前名称：`LocalEntryStatus`、`startLocalEntry`、`stopLocalEntry`、`getLocalEntryStatus`。
- 推荐名称：`LocalGatewayStatus`、`startLocalGateway`、`stopLocalGateway`、`getLocalGatewayStatus`。

[前端契约](../../src/lib/backend/contracts/adapter.ts) 和 Rust 注释将它描述为共享本机转发入口；核心运行时、端口和接入动作则已经稳定使用 `Gateway`。

影响 Rust 状态 DTO、Tauri command、TypeScript Port、API façade、mock、Routes 页面和测试。旧 IPC command 应保留兼容别名。`LocalEntryStatus.statuses` 如继续存在，可同步明确为 `bridgeStatuses`。

### N-06：v2 被当作长期领域名称

- 严重程度：中。
- 优先级：P2。
- 当前名称：`v2_enrolled`、`v2Enrolled`、`enroll_v2_and_refresh_index`、`resolve_v2_pool_members`。
- 推荐名称：`unified_gateway_enrolled`、`unifiedGatewayEnrolled`、`enroll_unified_gateway_and_refresh_index`、`resolve_unified_gateway_pool_members`。

[RoutePool 模型](../../crates/agenthub-core/src/models/route_pool.rs) 的注释已说明真实含义是“是否接入统一 Gateway”。`v2` 只是历史实现版本，不能向长期调用者表达领域含义。

该名称涉及数据库列、核心服务、Tauri 控制器、TypeScript contract、wire map、mock、页面和测试，必须作为数据库与 IPC 兼容迁移处理，不能只做全文替换。

### N-07：来源 ID 无法表达 account/provider 多态身份

- 严重程度：中。
- 优先级：P1。
- 当前名称：`source_connection_id`、`sourceConnectionId`。
- 推荐名称：如果需要保留，拆为 `sourceKind + sourceId`；如果状态消费端不需要，则直接删除。

Adapter Profile 的来源可以是 account 或 provider，但单一 `sourceConnectionId` 无法完整表达来源身份。[TypeScript wire mapper](../../src/lib/backend/contracts/adapter-wire.ts) 没有把该字段保留到最终状态对象，部分 Rust 路径又把来源硬编码为 account。

这不是单纯改名问题。应先确认状态 DTO 是否需要暴露来源；需要时采用带类型的复合身份，不需要时删除死字段，避免继续形成伪契约。

### N-08：currentProvider 实际是当前有效连接标签

- 严重程度：中。
- 优先级：P1。
- 当前名称：`AgentStatus.currentProvider`。
- 推荐名称：迁移读取方到现有 `effectiveLabel` 和 `effectiveKind`，然后移除 `currentProvider`。

[AgentStatus](../../src/lib/types.ts) 已将 `currentProvider` 标为兼容字段。它可能表示账号登录，也可能表示供应商连接，因此不一定是 Provider。

影响 Dashboard、聊天、告警和状态展示等约 10 个读取方。新字段已经存在，整改重点是停止新增旧字段读取，而不是再创造第三套名称。

### N-09：公共 Tauri command 暴露实现后缀

- 严重程度：中。
- 优先级：P2。
- 当前名称：`list_mcp_inventory_cmd`、`list_plugin_inventory_cmd`、`enable_plugin_cmd`、`disable_plugin_cmd`、`list_install_catalog_cmd`。
- 推荐名称：公开 IPC 使用 `list_mcp_inventory`、`list_plugin_inventory`、`enable_plugin`、`disable_plugin`、`list_install_catalog`。

[MCP command](../../src-tauri/src/commands/mcp.rs)、[插件 command](../../src-tauri/src/commands/plugins.rs) 和 [安装 command](../../src-tauri/src/commands/install.rs) 的注释都使用无 `_cmd` 的 Invoke 名，但实际导出名带实现后缀。

Rust 内部函数可继续使用后缀消歧，公开 IPC 不应泄露该细节。建议注册无后缀规范名，并保留旧命令别名一个兼容周期。

### N-10：安装进度的 line 实际是原始输出块

- 严重程度：中。
- 优先级：P2。
- 当前名称：`InstallProgressPayload.line`。
- 推荐名称：`chunk` 或 `textChunk`。

[安装进度契约](../../crates/agenthub-core/src/services/install_progress.rs) 实际传输受大小限制的原始 UTF-8 块，可能包含半行、空串或多行；前端也会再次拆行。[TypeScript 类型](../../src/lib/backend/contracts/install-types.ts) 把它描述为 log line，容易诱导新消费者按完整行处理。

影响 Rust payload、TypeScript 类型、Tauri 事件监听和日志消费端。兼容期可以同时发送 `chunk` 与 `line`，前端优先读取 `chunk`。

### N-11：projectSkill 中 project 同时是动词和工作区名词

- 严重程度：中。
- 优先级：P2。
- 当前名称：`projectSkill`、`project_skill`、`SkillProjectResult`。
- 推荐名称：`applySkillProjection`、`apply_skill_projection`、`SkillProjectionResult`。

[技能 API](../../src/lib/api/skill.ts) 中 `projectSkill` 表示把共享技能同步到某个 Agent；同一文件的 `listProjectSkills` 中 `Project` 又表示工作区项目。两种含义在同一 API 中直接碰撞。

影响 Core Service、Tauri command、前端 API、contract、mock 和测试。工作区中的 `listProjectSkills`、`ProjectSkill` 可以保留。

### N-12：开放 Agent 注册表仍大量使用兼容名称 AgentId

- 严重程度：中。
- 优先级：P2。
- 当前名称：新 TypeScript 代码和公共契约中的 `AgentId`。
- 推荐名称：`AgentKey`。

[类型定义](../../src/lib/types.ts) 已将 `AgentId` 标为兼容别名，开放 Agent 注册表以 `AgentKey` 为准。

存量使用较多，应采用“新代码禁止增加 `AgentId`、公共契约优先迁移、兼容层最后删除”的方式处理。Rust 内置 Agent 的闭合枚举仍有独立语义，不能全仓机械替换。

### N-13：Routes 与 API Key 页面使用了另一套用户词汇

- 严重程度：中。
- 优先级：P1。
- 当前名称：“账号 / API 配置”“OAuth 接入 / API 接入”“接入 API”“账号显示名”“API Key 账号已添加/更新”。
- 推荐名称：“登录 / 供应商”“官方登录 / 添加 API Key”“添加 API Key”“登录显示名”“API Key 登录已添加/更新”。

[Routes 来源选择](../../src/pages/routes/shared/adapter-create-flow.ts) 当前仍位于历史命名目录；它与 [连接池按钮](../../src/pages/routes/pool/PoolAddButtons.tsx) 和 [中文文案](../../src/lib/i18n/locales/zh.ts) 使用的部分名称，都和项目其他页面已经采用的“登录、官方登录、API Key、供应商”不一致。

影响创建路由、连接池新增入口、API Key 对话框、中英文翻译和相关测试。后端枚举 `account/provider` 不需要因用户文案改变而重命名。

### N-14：share 同时表示直接接入和分享至连接池

- 严重程度：中。
- 优先级：P3。
- 当前名称：`ConnectBindPurpose = 'share' | 'route'`、`resolveTicketShareAction`。
- 推荐名称：`ConnectBindPurpose = 'direct' | 'route'`、`resolveTicketDirectAction`。

[connect-flow 类型](../../src/lib/connect-flow/types.ts) 中 `share` 实际表示直接写入目标 Agent；产品词汇中的“分享”已经专指“分享至连接池”。

该流程当前生产接线较少，适合在重新启用或清理这条历史流程时处理，不必单独插队重构。

### N-15：持久化键存在两套格式

- 严重程度：低。
- 优先级：P3。
- 当前名称：`agenthub:` 加 kebab-case，以及 `agenthub.` 加 camelCase。
- 推荐名称：集中定义 `StorageKey`，新键统一采用 `agenthub:` 加 kebab-case。

[ui-preferences.ts](../../src/lib/ui-preferences.ts) 已形成集中式前缀规范，但插件、备份、技能矩阵和 Routes 页面仍存在自建点号键。

这是持久化格式，不应直接替换。迁移必须读取旧键、写入新键，并在确认升级覆盖后再清理旧键，避免重置用户布局偏好。

### N-16：两个机械式词边界异常

- 严重程度：低。
- 优先级：P3。
- 当前名称：`USAGE_SYNC_SETTINGS_CHANGED`、`skillssh_market.rs`。
- 推荐名称：`USAGE_SYNC_SETTINGS_CHANGED_EVENT`、`skills_sh_market.rs`。

前者与同模块其他 DOM 事件常量的 `_EVENT` 后缀不一致；后者对应 `SkillsShMarket` 类型，但文件名缺少 `skills`、`sh` 的词边界。[Rust 文件](../../crates/agenthub-core/src/services/skills_sh_market.rs)

两项都适合作为相关模块修改时的机械清理，不值得单独安排高风险变更。

## 同一概念的名称冲突

| 概念 | 当前冲突 | 建议边界 |
| --- | --- | --- |
| 连接池 | `connection-pool-store`、`RoutePool` | 产品连接池固定为 `RoutePool`；全量登录/供应商缓存使用 `ConnectionInventory` |
| 路由产品面 | Routes、bridges、adapter | 页面、导航和用户操作使用 Routes；`bridge` 仅表示 `local_bridge`；`AdapterProfile` 保留在后端契约 |
| 本机运行入口 | LocalEntry、Gateway、relay | 内部统一 `LocalGateway`；界面使用“本机转发” |
| 当前有效连接 | currentProvider、effectiveLabel | 统一读取 `effectiveLabel/effectiveKind` |
| 来源身份 | sourceConnectionId、account/provider | 使用 `sourceKind + sourceId` |
| 项目与会话 | AgentProject、AgentSession、deleteAgentProject | 项目和会话使用各自实体名；删除操作必须写明 session |
| 技能同步 | projectSkill、listProjectSkills | 同步到 Agent 使用 `applySkillProjection`；项目目录技能保留 `ProjectSkills` |
| Agent 标识 | AgentId、AgentKey | 开放注册表和新 TypeScript API 使用 `AgentKey`；兼容层暂留 `AgentId` |
| share | 直接接入、分享至连接池 | `share` 只表示分享至连接池；直接接入使用 `direct` |
| 用户授权方式 | 凭据、账号、OAuth/API 接入 | 界面固定使用“登录、官方登录、API Key、供应商” |

## 暂缓项

以下候选存在异味，但当前不应直接确定新名称：

- `AdapterControl`：职责确实同时包含登录接入与本机转发生命周期，但 `RouteBindingControl` 又覆盖不了删除、状态查询和自动恢复。应先确定模块是否拆分，再命名。
- `UpstreamChannel` 的枚举变体：品牌与协议名称的抽象层级不完全一致，但 Codex、Grok 通道确有不同登录和转换行为。
- `OAuth` / `Oauth`：Rust 枚举变体和领域类型存在不同写法，但当前边界基本稳定。可以补充 acronym 规则，不值得全仓迁移。
- React/TSX 文件的 PascalCase、kebab-case、camelCase：目前大致对应公共组件、页面内部模块和 Hook，应先明确目录级规则。
- `OAuthFlowDialog.ts`、`ProviderEditDialog.tsx`、`ApiKeyAccountDialog.tsx` 等兼容代理：确认没有外部引用后直接删除，比再改一次名字更合理。

## 明确排除的误报

- 内部 `Ticket`、`TicketWallet`、`Binding`：术语表允许的内部领域名，只要求不直接展示给用户。
- `Account` 与 `Provider`：它们是不同的数据来源和存储模型，不应强行合并。
- Rust `snake_case` 与 TypeScript `camelCase`：由 serde 和 wire 映射层转换，属于正确分层。
- `Mcp`、`Api` 的 PascalCase：符合项目现行类型命名习惯。
- `apikey`、`api_key`、`api-key`：分别用于内部值、wire 值和历史输入兼容；应在转换边界规范化，不应全局替换。
- 应用 `autoStart` 与路由 `autoStart`：所属类型和作用域清楚，分别表示应用随系统启动和本机路由恢复。
- `mutation-coordinator.ts`、`runtime-context.ts`、`contracts/ports.ts`：存在更精确的备选名称，但当前名称尚未造成足够的语义错误。
- `opts`、`ro`、`tokenTick`、`collectKey` 以及普通短作用域变量：可以局部提高可读性，但不足以成为项目级整改项。
- mock 与 Tauri 层的同名文件：属于刻意的接口对称，不是重复或冲突。

## 整改顺序

### P0：立即处理

1. 为删除会话提供 `deleteAgentSession` 规范名称和旧 IPC 兼容入口。
2. 修正用户界面的“凭据”“账号”“OAuth/API 接入”等词，并重命名 `SecurityPanel`。

### P1：消除核心概念冲突

1. 将前端全量连接缓存从 `ConnectionPool` 改为 `ConnectionInventory`。
2. 将 Routes 页面、路径和页面层 façade 从泛用 `bridges` 迁出。
3. 统一 `LocalEntry` 为 `LocalGateway`。
4. 明确来源身份为 `sourceKind + sourceId`。
5. 迁移 `currentProvider` 的剩余读取方。

### P2：在兼容迁移窗口处理

1. 迁移 `v2_enrolled` 数据库和 IPC 字段。
2. 将安装输出 `line` 迁移为 `chunk`。
3. 为带 `_cmd` 的公开命令增加无后缀规范名。
4. 区分技能同步动作与项目目录技能。
5. 渐进迁移 TypeScript 公共契约中的 `AgentId`。

### P3：随模块修改清理

1. 清理未启用流程中的 `share` 歧义。
2. 统一持久化键格式。
3. 修正事件常量后缀和 `skills_sh_market` 文件词边界。
4. 删除确认无外部引用的兼容代理文件。

## 核对后的分批整改计划（2026-09-02）

以当前 `dev` 源码核对：审计 16 项均仍成立，推荐新名尚未落地。按 AGENTS.md 风险分级分批实施；每批改完跑过滤测试，最后同步文档并跑 `pnpm check:docs`。

### 实施状态（本分支）

| 批次 | 状态 | 备注 |
| --- | --- | --- |
| A | 已完成 | `deleteAgentSession` + LoginInformation + Routes 文案 |
| B | 已完成 | `ConnectionInventory`；`currentProvider` 读取方迁 `effectiveLabel` |
| C | 已完成 | `routes-path` + `pages/routes/shared`；`bridges-path` 兼容层 |
| D | 已完成 | `LocalGateway` IPC；TS wire 去掉死字段 `sourceConnectionId`（Rust 审计字段暂留） |
| E | 已完成 | 无 `_cmd` 规范名；install `chunk`/`line` 双发；`applySkillProjection` |
| F | 已完成 | 领域名 `unifiedGatewayEnrolled`；DB 列仍为 `v2_enrolled`；wire 双读 |
| G | 已完成 | `ConnectBindPurpose=direct`；事件 `_EVENT`；`skills_sh_market`；`AgentKey` 渐进（project port） |
| H | 已完成 | N-15 约定写入 `ui-preferences`；文档断链修复；`pnpm check:docs` 通过 |

### 核对调整

| 项 | 调整 |
| --- | --- |
| N-03 | 影响约 20 个 TS 文件 / ~149 标识符（审计原估偏小） |
| N-07 | 前端最终状态对象未消费该字段；优先删除死字段，除非 Rust 审计路径需要保留 |
| N-08 | 勿误改 chat 页本地 `Provider` 变量名 `currentProvider` |
| N-04 | 已有部分 `ROUTES_*_PATH` 与 `BRIDGES_*` 并存，迁移时合并而非重造 |
| N-12 | 全仓机械替换不做；本轮只迁公共契约与本轮触及文件，Rust 闭合枚举不动 |

### 批次划分

| 批次 | 覆盖 | 风险 | 文件范围（主） | 兼容策略 | 验收 |
| --- | --- | --- | --- | --- | --- |
| **A** | N-01、N-02、N-13 | 局部 / 跨层 IPC 轻量 | `commands/project.rs`、`project` Port/API/mock、Settings 面板、`zh.ts`/`en.ts`、Routes 连接池文案、相关测试 / e2e | 新 IPC `delete_agent_session(s)`；旧名作别名；`?tab=security` 仍映到 about；后端 `account`/`provider` 枚举不改 | 调用方走 session 名；用户文案无「凭据 / OAuth 接入 / API 接入」；Settings 面板名为 LoginInformation；定向 Vitest + i18n parity |
| **B** | N-03、N-08 | 模块 | `connection-pool-store*`、`ConnectionPoolProvider`、读取方；`AgentStatus.currentProvider` 读取方 | 先加 `ConnectionInventory*` 与兼容导出，再迁调用方；读 `effectiveLabel`/`effectiveKind`，compat 字段暂留 | 产品「连接池」仅指 RoutePool；新代码不读 `currentProvider`；相关 Vitest |
| **C** | N-04 | 模块 | `bridges-path.ts` → `routes-path.ts`；`pages/bridges/` → `pages/routes/shared/`；导航与导入 | `/bridges` 重定向保留；`local_bridge` / `useBridgeRuntimeActions` 不改 | 页面路径与导航用 Routes；无泛用 `BRIDGES_*` 生产入口；定向测试 |
| **D** | N-05、N-07 | 跨层 | LocalEntry DTO/command/Port/页面；`source_connection_id` | 新 IPC `*_local_gateway`，旧 `*_local_entry` 别名；N-07 删除死字段或改为 `sourceKind+sourceId` | 内部名 LocalGateway；界面仍「本机转发」；wire 无伪契约；contract + Rust filter |
| **E** | N-09、N-10、N-11 | 跨层 | MCP/plugins/install commands；InstallProgress；`project_skill` 动词 API | 无 `_cmd` 规范名 + 旧别名；payload 双发 `chunk`/`line`；`apply_skill_projection` + 旧别名；工作区 `listProjectSkills` 保留 | 新 invoke 无 `_cmd`；前端优先 `chunk`；技能同步与项目技能 API 分离；过滤测试 |
| **F** | N-06 | 高风险 | route_pools 映射、`v2_enrolled` 全栈 | **DB 列名暂留 `v2_enrolled`**；Rust/TS 领域名 `unified_gateway_enrolled` / `unifiedGatewayEnrolled`；wire 双读旧 `v2Enrolled` | 页面与 wire 用新名；SQL 仍读写旧列；过滤测试 |
| **G** | N-12（渐进）、N-14、N-16 | 局部 | 本轮触及的 TS 契约用 `AgentKey`；`ConnectBindPurpose`；事件常量与 `skillssh_market` | `AgentId` 别名暂留；`share`→`direct` 仅 connect-flow；文件/常量机械改名 | 本轮新代码不新增 `AgentId`；connect-flow 无 share 歧义；常量后缀一致 |
| **H** | N-15 + 文档同步 | 局部 / 持久化 | `ui-preferences` 与各页自建键；本审计文档、术语表交叉引用 | 读旧键写新键；确认覆盖后再清旧键 | 新键统一 `agenthub:`+kebab；`pnpm check:docs` |

### N-15 说明

本轮只落地「新键走集中 `StorageKey`」与 `ui-preferences.ts` 注释约定，不做存量 `agenthub.` camelCase 键批量迁移（避免重置用户布局）。存量迁移可单独跟进。

本文结论与问题清单仍保留审查时事实；实施以「实施状态」表为准。后续删除兼容别名前须再过一个版本窗口。

### 每批验证命令（默认）

- 前端局部：`pnpm test -- --run <touched.test.ts…>`；改 i18n 时加 locale parity。
- 跨层 IPC：`pnpm test -- --run` 对应 contract/wire + 必要时 `cargo test -p agenthub --locked <filter>`。
- 文档收尾：`pnpm check:docs`。
- 不做默认全量 `pnpm test` / 生产 `pnpm build`（留给提交前或 CI）。

## 迁移原则

跨层改名统一采用以下顺序：

1. 新增规范名称。
2. 保留旧名称作为明确标记的兼容别名。
3. 迁移仓库内部调用方。
4. 为旧数据库值、持久化键和 IPC 参数提供兼容读取。
5. 至少经过一个兼容版本后再删除旧名称。

不得直接全文替换数据库字段、IPC command、持久化键或外部配置值。涉及用户文案的修改不应反向触发内部 `Account`、`Provider`、`Ticket` 模型重命名。

## 验收标准

- `deleteAgentProject` 不再作为新代码调用入口，删除粒度在名称中明确为 session。
- 用户可见文案不再把“凭据”“OAuth 接入”“API 接入”作为普通产品概念。
- `ConnectionPool` 只表示 `RoutePool` 领域对象，不再表示全量连接缓存。
- Routes 的页面、路径和导航名称不再泛用 `bridges`；真实 `local_bridge` 和旧地址兼容除外。
- 新代码不再增加 `currentProvider`、`v2Enrolled`、`AgentId` 等兼容名称的使用。
- 所有数据库、IPC、持久化键改名均有旧值兼容测试。
- Markdown 改动通过 `pnpm check:docs`。
