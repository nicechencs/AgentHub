# Adapter 页面与本地协议桥接设计

> 状态：**可应用路径已接线（Claude 稳定直连 + Codex 实验性本地桥接）**。Pi/config_sync 等仍为预览-only；发布前仍需实机 dogfood。`local_bridge` 的目标宿主已决策为用户级 sidecar，但当前工作区仍由 Tauri `AppState` 进程内托管，尚未完成进程迁移。
> 调研日期：2026-08-12（进度同步：2026-08-12）
> 重点参考：`D:\demo_github\AgentHub_Ref\Cli-Proxy-API-Management-Center`
> 关联文档：[adapter-sidecar-design.md](adapter-sidecar-design.md)、[provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md)、[architecture.md](architecture.md)、[ui-design.md](ui-design.md)、[logging.md](logging.md)、[account-authorization-pool.md](account-authorization-pool.md)

## 0. 当前落地状态

| 范围 | 状态 | 当前边界 |
|---|---|---|
| 规则分析与预览 | ✅ | contracts、mock、`analyze`、`plan`、Adapter 页面和 profile 列表已接线；limitations 与 `canApply` 对齐真实能力 |
| 稳定规则应用 | ✅ | Kimi Code 会员 Provider → Claude Code `native_endpoint` 可 apply；finalize 失败会回滚 live/current；返回值脱敏 |
| 其它直连 / 配置同步规则 | 🟡 | Pi 等仍预览或 unsupported；未显式 `canApply=true` 一律不可写 |
| Bridge core | ✅ | `BridgeRuntimeHost`（per-profile gate、admission、超时与 cancellation-safe drain）、Responses ↔ Chat 协议与 fixtures |
| Bridge 产品接线 | ✅ | Codex `local_bridge` 的 `canApply`、Tauri apply/start/stop/status、健康检查、失败补偿、凭证轮转 stop→restart、端口 rebind、opt-in auto-start 恢复、退出 drain；UI 已拆分 wire/model/components |
| Bridge 进程边界 | 🎯 已决策 / 未迁移 | 目标为同包用户级 `agenthub-adapterd`；当前 `BridgeRuntimeHost` 仍由 Tauri `AppState` 持有，详细契约见 [Adapter Sidecar 目标架构](adapter-sidecar-design.md) |

这里的“已落地”描述当前工作区状态，不代表相关能力已经随 Release 发布；“已决策”只表示目标架构确定，不代表 sidecar 二进制、IPC 或进程监管已经实现。

## 1. 结论

Adapter 负责把 **Connections 中已有的授权或 API Key** 接入另一个 Agent。它不是第二套 Connections，也不是通用 API 网关控制台。

一次适配只产生以下四种结果之一：

| 结果 | 含义 | 用户看到的动作 |
|---|---|---|
| `config_sync` | 目标 Agent 原生支持该凭据及协议，仅转换配置结构 | 写入配置 |
| `native_endpoint` | 上游已经提供目标 Agent 所需协议，仅写 Base URL、模型和凭据引用 | 写入配置并测试 |
| `local_bridge` | 凭据可用，但上下游协议不一致；启动 loopback 本地服务进行请求、流式响应转换 | 启动服务、写入 Connections |
| `unsupported` | 授权范围、协议能力或服务条款不允许安全转换 | 解释原因，不提供“强制转换” |

核心产品决策：

1. **复用 Connections**：凭据仍在 Connections 管理；Adapter 只引用 `connection_id`，不复制一套账号池。
2. **优先直连**：能通过配置同步或上游原生兼容端点完成时，不启动本地服务。
3. **桥接是兜底**：只有明确需要协议转换时才启动本地服务，并把生成的端点登记回 Connections。
4. **不是 Token 格式互转**：OAuth access/refresh token 不能通过改字段名变成另一家授权。只有目标客户端明确支持同一授权和刷新语义时，才可做配置同步。
5. **能力要可验证**：兼容性由版本化规则和真实探测共同决定，不依赖页面硬编码的宣传矩阵。
6. **Provider 不是服务**：Provider/Connection 是持久化配置实体；需要后台运行的是 `BridgeRuntime`。当前由 AgentHub 托盘进程托管，目标迁移到用户级 `agenthub-adapterd`；无论部署形态如何，都不把页面组件、Connections 或 ProviderService 变成长驻 HTTP 服务。

## 2. 范围与非目标

### 2.1 MVP 范围

- 从 Connections 选择一个现有 OAuth 或 API Key 连接。
- 选择目标 Agent，并自动分析可用路径。
- 预览将写入的配置、本地服务影响和已知能力差异。
- 执行配置同步、原生兼容端点接入或本地协议桥接。
- 对上游和最终目标协议分别进行最小有效请求测试。
- 管理本地桥接的启动、停止、重启、最近状态和错误诊断。
- 关闭主窗口后，本地桥接继续由托盘进程运行；显式退出 AgentHub 前提示会停止的桥接数量。
- 将桥接结果创建为目标 Agent 可使用的 Provider/Connection，并复用现有切换、备份和恢复链路。

### 2.2 明确不做

- 不把 ChatGPT、Claude 等消费级订阅 OAuth 当作通用 API Key 导出或任意转售。
- 不承诺所有 OAuth 都能跨 Agent 使用；未经官方客户端或 SDK 明确支持的 OAuth 组合默认为 `unsupported`。
- 不建设公网网关、团队租户、计费、负载均衡、权重、冷却池或配额调度平台。
- 不在 Adapter 首屏建设完整协议矩阵、监控大盘、日志控制台或 Provider 多栏工作台。
- 不记录请求/响应正文，不展示或复制完整 Token。
- 不把凭据落盘加密列为本功能任务；按项目既有决策继续沿用当前存储方案。
- 不在 MVP 转换厂商专属原生工具、加密思考块、视频通道或无法无损表达的扩展字段。

## 3. 兼容性判定

厂商产品、API/OAuth 边界、官方依据和当前实现矩阵统一维护在 [模型厂商、API 与 OAuth 适配规则](provider-api-oauth-adaptation.md)。本文只定义 Adapter 如何消费这些规则，不再维护第二份厂商矩阵。

### 3.1 判定顺序

```text
选择 Connection + 目标 Agent
  → 校验凭据产品/区域/授权范围
  → 读取目标 Agent 能力与版本
  → 是否有目标原生配置映射？
       是 → config_sync
  → 上游是否原生提供目标协议端点？
       是 → native_endpoint
  → 是否存在已测试的协议转换器，且条款/授权允许？
       是 → local_bridge
  → unsupported（给出原因与可行替代）
```

### 3.2 规则契约

兼容规则至少包含：

```ts
type CompatibilityRule = {
  id: string;
  sourceProduct: string;
  credentialKinds: Array<'oauth' | 'api_key'>;
  sourceProtocols: ProtocolId[];
  targetAgent: AgentId;
  targetProtocol: ProtocolId;
  route: 'config_sync' | 'native_endpoint' | 'local_bridge' | 'unsupported';
  support: 'stable' | 'experimental' | 'unsupported';
  minTargetVersion?: string;
  maxTargetVersion?: string;
  requiredCapabilities: CapabilityId[];
  limitations: string[];
  sourceUrl?: string;
  verifiedAt: string;
};
```

规则必须来自独立适配文档中的已验证条目，并与分析、计划、执行和测试使用同一版本。页面只展示适用于当前选择的结论；“查看兼容性依据”再显示来源、验证日期和限制，不把完整规则表常驻首屏。

## 4. 页面设计

### 4.1 页面定位

- 路由：沿用 `/adapter`；旧 `/router` 继续重定向。
- 标题：`Adapter`。
- 简介：`复用已有连接；必要时启动本地协议转换。`
- `descriptionTip`：说明不会把一家 OAuth 凭据“转换”为另一家的授权，也不会在日志记录请求正文。
- PageHeader 右侧唯一主按钮：`新建适配`。

页面沿用 `pageRhythm.pageShell`、`PageHeader`、`ListRow`、`Card`、`Badge`、`Dialog`、`EmptyState`、`ErrorState` 和现有 Tailwind 语义 token。不引入参考项目的 SCSS token、Sheet primitive 或新的视觉系统。

### 4.2 首屏信息架构

保持单列纵向结构：

```text
PageHeader                                             [新建适配]

适配记录（N）                                  [仅显示异常 □]
  后台服务 ● 运行中 · 2 个本地桥接

  ● Kimi Code → Codex        本地桥接    127.0.0.1:43121
    运行中 · 上次请求 2 分钟前 · 1280 ms        [停止] [详情]

  ● Anthropic → Pi           配置同步
    已应用 · 上次验证 今天 10:24                  [重新验证] [详情]

（无记录时：说明 + 新建适配 CTA）
```

不默认展示：

- Provider 分类左栏；
- OAuth/AuthFiles 卡片网格；
- 全量源 × 目标协议矩阵；
- 请求成功率图表；
- 完整日志流。

### 4.3 新建适配流程

使用现有宽 Dialog 完成创建；只在最终应用前切换到简短确认视图，不增加多页 Wizard。若未来全站经设计评审引入共享 Sheet primitive，再统一迁移，Adapter 不单独创造交互原语。

#### 步骤 A：选择来源

- 下拉框按 Agent/Provider 分组列出 Connections 中可用连接。
- 每项展示：名称、凭据类型、所属产品、状态、脱敏尾号。
- 来源名称、产品与凭据类型按[独立适配规则](provider-api-oauth-adaptation.md)展示；已验证的预设可自动选择端点，不要求用户手工猜 Base URL。
- 没有连接时显示 `前往 Connections 添加`，完成后返回当前 Dialog 并刷新。
- OAuth 尚未完成时，在当前区块内展示 `连接 / 继续授权 / 等待回调 / 已连接 / 失败`；授权成功后的主动作是继续创建，而不是另开管理页。

#### 步骤 B：选择目标

- 目标 Agent 只列已安装或可配置的 Agent；不可用项置灰并说明原因。
- 用户选择后立即运行 `analyze`，局部显示 skeleton，不锁住已有适配列表。
- 结果使用一个清晰 Badge：`直接同步`、`原生端点`、`需要本地代理` 或 `不支持`。

#### 步骤 C：确认配置

默认只展示必要字段：

- 目标模型；
- 生成名称；
- 本地端口（仅桥接，默认自动分配）；
- 是否立即设为当前连接；
- 已知能力差异。

模型发现是辅助能力：可点击 `获取模型`，失败时允许手工填写，不阻断流程。Headers、模型别名和协议细节放入 `高级设置`，且仅在规则声明需要时出现。

字段附近提供两类局部测试：

- `测试上游`：按上游协议发送最小、非流式请求；
- `测试目标格式`：直连时测试目标端点，桥接时在服务启动后测试 loopback 端点。

任何 endpoint、模型、Header 或来源连接变更都将旧测试结果重置为 `idle`，避免展示过期的成功状态。

#### 步骤 D：应用预览

确认 Dialog 只包含：

- 适配路径和原因；
- 将创建/更新的 Connection/Provider；
- 将写入的目标配置摘要；
- 是否启动本地服务及 loopback 地址；
- 已知能力损失；
- 备份与失败回滚说明。

默认展示“新增 1、修改 1、启动服务 1”等摘要；`查看变更` 再展开结构化 diff。无需常驻 FloatingSaveBar，也不引入代码编辑器式全屏 diff。

#### 步骤 E：完成

- 成功：Dialog 内展示 `已创建并验证`，主按钮 `在 Connections 查看`，次按钮 `完成`。
- 部分失败且已回滚：说明失败阶段和“未改动现有配置”，允许重试。
- 补偿失败：进入 `needs_attention`，显示稳定错误码、已完成/未完成步骤和唯一推荐恢复动作。

### 4.4 适配列表与详情

每行只展示：

- 状态点；
- `来源连接 → 目标 Agent`；
- 路径 Badge；
- 本地 endpoint（仅桥接）；
- 当前状态、最近请求或最近错误；
- 与状态匹配的主操作：启动、停止、重新验证；
- `详情`。

列表区顶部仅在存在 `local_bridge` 时显示一行后台服务摘要：`运行中 / 启动中 / 已停止 / 异常`、桥接数量和 `重新启动`。它不是新的监控面板；`config_sync`、`native_endpoint` 不依赖后台服务，也不计入数量。

详情 Dialog 使用 `detail / edit` 三态：

- 详情：配置摘要、能力限制、最近 5 条事件、生成的 Connection 链接。
- 编辑：模型映射、端口/自动启动等必要字段；dirty 时才允许保存。
- 关闭或切换对象时若有未保存变更，使用二次确认 Dialog 拦截。
- 停止/删除走独立确认；只禁用当前行，不锁住整个列表。

### 4.5 页面状态与文案

| 状态 | 表现 |
|---|---|
| loading | 复用列表 skeleton，Dialog 分区局部 skeleton |
| empty | 说明“把现有连接接入其他 Agent”，提供 `新建适配` |
| disconnected | inline ErrorState；禁用新建和 mutation，已有信息可读 |
| unsupported | 中性说明，不使用红色故障态；给出可用替代路径 |
| starting/stopping | 当前行按钮 loading，其他行可操作 |
| error | 行内短错误 + `查看诊断`；toast 只用于操作结果，不承载完整原因 |
| needs_attention | warning 状态，显示恢复动作，不自动反复重试写配置 |

## 5. 服务设计

### 5.1 模块边界

建议新增 `adapter_service`，不要扩张现有 `AgentAdapter` 为协议代理实现。下图中的 `ConfigurationService` 是 core 实现；前端 `ConfigPort` 只是经 Backend adapter 暴露其能力的契约，core 不依赖 TypeScript contract：

```text
adapter_service
  ├─ compatibility_registry  # 版本化规则、限制、来源
  ├─ route_planner           # 分析 config/native/bridge/unsupported
  ├─ bridge_runtime          # 独立的 loopback 数据面与实例生命周期
  ├─ bridge_runtime_host     # 宿主端口：restore/start/stop/status/shutdown
  ├─ protocol/               # 纯请求/事件/错误映射
  │   ├─ openai_chat
  │   ├─ openai_responses
  │   └─ anthropic_messages
  └─ adapter_repo            # profile 与运行状态

复用：
  account_service / provider_service / connection_service
  backup_service / ConfigurationService / AgentAdapter
  logging::redact_* / utils::agent_lock
```

职责：

- `AgentAdapter`：仍负责每个 Agent 的 live 配置读写真相。
- `adapter_service`：规划跨连接路径，编排 Bridge Runtime 和目标配置；不自行监听端口。
- `protocol/*`：只做协议结构/流式事件转换，不读数据库、不写 Agent 配置。
- `bridge_runtime`：数据面，只处理 loopback HTTP、上游调用和协议转换；不写目标 Agent 配置。
- `bridge_runtime_host`：管理实例恢复、启动、停止和状态；core contract 不依赖 Tauri，当前由 GUI `AppState` 实现宿主接线。
- `provider_service`：继续负责 Provider CRUD、切换预览、备份和安全切换。

### 5.2 本地桥接形态

需要区分“模块独立”和“进程独立”。Bridge Runtime 已在 core 中完成模块隔离；下一目标是把 `local_bridge` 的完整运行时与生命周期迁入同包用户级 sidecar `agenthub-adapterd`。这项决策不等于把 Connections、Adapter 页面或所有 Adapter 规则搬进另一个进程。

| 形态 | 能否关窗口继续运行 | 能否退出 GUI 后运行 | 安装/升级成本 | 决策 |
|---|---:|---:|---:|---|
| AgentHub 托盘进程内托管 | 是 | 否 | 低 | **当前实现；迁移期回滚路径** |
| 同包 sidecar / `agenthub-adapterd` 用户级进程 | 是 | 是 | 中；需 IPC、单实例、版本握手与升级协调 | **目标架构** |
| Windows Service / macOS LaunchAgent / systemd 系统服务 | 是 | 是 | 高；涉及权限、安装器、跨平台运维 | 个人桌面产品当前不做 |

当前 `BridgeRuntimeHost` 由 Tauri `AppState` 持有：窗口隐藏或前端重载不影响监听，但显式退出 GUI 会停止 Bridge。目标态由 sidecar 持有 host 与 `local_bridge` 完整 saga；GUI 只通过本地 IPC 管理它，退出 GUI 默认不停止已运行的本地适配。

进程迁移必须遵守以下边界：

- Connections 继续管理 Account、Provider、ActiveBinding 与来源引用，不建设 `connectionsd`，也不复制账号池。
- `ConnectionService` / `ProviderService` / `AccountService` 仍是数据库与 live 配置事务的领域 owner；sidecar 只能调用这些 core service，禁止直接拼 SQL 或直接写 Agent 配置。
- `native_endpoint` / `config_sync` 不依赖 sidecar；只有 `local_bridge` 的监听、协议转换、运行状态、恢复和完整 apply/start/stop/remove saga 进入 sidecar。
- `local_bridge` profile 的变更在目标态只有 sidecar 一个进程 writer；GUI 只读持久化 profile，并通过 IPC 发起 mutation。
- GUI 与 sidecar 必须复用同一规则实现。plan 携带 `rule_version`，sidecar apply 时重新解析来源、校验 revision 与规则版本，不能信任陈旧的前端计划。
- sidecar 是当前用户权限下的同包进程，不默认提权为系统服务；`AdapterProfile.autoStart` 也不得静默开启 OS 开机自启。

- 仅监听 `127.0.0.1`/`::1`，禁止默认监听 `0.0.0.0`。
- 默认自动分配端口；用户指定端口时先检测占用。
- 每个 profile 一个稳定本地访问 Token，目标 Agent 只持有本地 Token，不直接获得上游 OAuth Token。
- 上游凭据按 `connection_id` 在 core 内解析和刷新；协议层只接收已授权的请求上下文。
- 当前进程内实现的可控退出仍统一经过 Tauri `ExitCoordinator`；目标态的普通 GUI 退出不 drain sidecar，只有显式“停止适配并退出”、应用更新、卸载或 sidecar 自身关停才走控制面 drain。
- `AdapterProfile.autoStart` 只表示“sidecar 启动后恢复此 profile”。若系统后台启动未开启，UI 只提示依赖条件，由用户在 Settings 明确选择。
- 恢复顺序为先启动并健康检查，再校验目标配置；失败时保持 `stopped/error`，不静默切到 mock 或其他上游。
- 每个 canonical data directory 只能有一个 sidecar/runtime owner；GUI 单实例不能替代 sidecar 自身的进程锁、instance epoch 和安全陈旧锁回收。

控制面、状态真相、进程启动/退出、版本与 schema 握手、SQLite shared/exclusive schema lease 与 migration 权威、锁序、崩溃恢复、更新前 running-set 恢复、卸载前 live binding 清理和三阶段迁移的完整契约见 [Adapter Sidecar 目标架构与迁移方案](adapter-sidecar-design.md)。

### 5.3 应用事务

延续现有 Provider 安全切换，不使用参考项目“多段写入后仅 refetch”的弱补偿方式。事务边界分为两层：`adapter_service` 负责完整操作的 saga；目标 Agent live 配置事务仍由 `ProviderService` 作为唯一 owner。当前 saga 接线位于 Tauri controller；迁移完成后由 sidecar 内的 Tauri-neutral application service 成为 `local_bridge` 唯一编排者，GUI 不得跨 IPC 继续执行后半段。`adapter_service` 不在外层持有 agent lock 后调用公开 `ProviderService::switch`，也不重复调用 `ConfigurationService.apply`：

```text
analyze
  → validate source connection / target capability / rule version
  → re-read source connection and target live revision
  → ConfigurationService.validate/plan（只读预览）
  → if local_bridge:
       reserve loopback port
       start candidate instance
       run upstream probe
       run target-protocol probe
  → create/update generated Provider（尚不设 current）
  → ProviderService.switch_generated_provider
       acquire target-agent lock（全流程唯一一次）
       backfill + re-read live revision
       create backup
       apply provider to live config
       read back and verify
       activate Provider in DB
       release lock
  → mark profile active and commit operation record
```

`switch_generated_provider` 是建议新增的 core 组合入口，可复用现有 `switch_inner` 的安全顺序，但必须由 ProviderService 内部持锁。若实现选择传递显式 lock guard，则 guard 只能由最外层取得一次，现有会自行加锁的公开 `switch` 不得在 guard 内调用。两种实现选其一，禁止双 owner、嵌套锁或同一次操作写 live 两次。

失败补偿按相反顺序：

1. 恢复目标 live 配置快照；
2. 恢复或删除本次生成的 Provider/Connection；
3. 停止本次新启动的桥接实例；
4. 释放端口和锁；
5. 记录原始错误和补偿结果。

进入 ProviderService 事务后再次读取目标配置，并按稳定身份和 revision 检测外部变更；若已变化，返回 `adapter.config_changed`，要求刷新预览，不能覆盖未知字段。core mapping 沿用 `ConfigurationService` 的 preserve-unknown/secret sentinel 语义，前端预览通过 `ConfigPort` 获取同一结果。

### 5.4 运行状态机

持久化 profile 生命周期、`auto_start` 恢复意图和进程内 observed runtime 必须分开。SQLite 可以保存最后错误与诊断信息，但 `running` 只能由当前 sidecar instance 的实时状态确认；sidecar 不可达时，页面派生 `host_unavailable`，不得把上次运行记录当成仍在监听。

```text
durable profile：draft → applying → active → removing → removed
                         └──────────────→ needs_attention

runtime observed：unknown/stopped → starting → running ↔ degraded
                                      └→ error      └→ stopping → stopped

client derived：宿主不可达 + 持久化 local_bridge profile → host_unavailable
```

- `degraded`：服务仍可监听，但上游探测失败或最近请求连续失败。
- `error`：实例未运行，且自动启动/显式启动失败。
- `host_unavailable`：目标配置仍指向 loopback，但当前 GUI 宿主或目标 sidecar 不可达；页面应给出启动、重连或修复动作。
- `needs_attention`：durable 配置、依赖或补偿无法自动恢复一致，必须由用户执行明确恢复动作。

禁止用单一 `is_running` 掩盖“监听成功但协议探测失败”。

## 6. 协议转换范围

### 6.1 MVP 协议

- OpenAI Chat Completions；
- OpenAI Responses；
- Anthropic Messages。

### 6.2 统一中间事件

不做“万能请求 JSON”，而是把响应流归一为少量语义事件：

```rust
enum BridgeEvent {
    MessageStart { id: String, model: String },
    TextDelta { text: String },
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, arguments_delta: String },
    ToolCallEnd { id: String },
    Usage { input: u64, output: u64, cached_input: Option<u64> },
    MessageEnd { stop_reason: StopReason },
    Error { code: String, message: String, retryable: bool },
}
```

请求转换至少覆盖：system/developer 指令、文本消息、工具定义、工具调用结果、模型映射、输出 token 上限和 reasoning effort 的有损映射。响应转换至少覆盖：SSE 顺序、文本增量、工具参数增量、停止原因、用量、上游 HTTP/协议错误以及客户端取消。

### 6.3 能力降级原则

- 无法表达的参数默认删除并在预览中列为 limitation；不能静默编造等价能力。
- 加密思考/签名块只允许原样透传到同协议，不解密、不伪造、不跨协议重建。
- 厂商原生 web search、computer use、视频等目标协议无对应能力时标记不支持。
- 工具 schema 转换失败要在请求发往上游前返回可操作错误。
- 不自动重试非幂等工具回合；429/5xx 的重试遵循上游 `Retry-After` 和严格上限。
- 客户端断开后立即取消上游流，避免继续消耗配额。

## 7. 数据与前端 Backend 契约

### 7.1 Core 模型

```ts
type AdapterProfile = {
  id: string;
  name: string;
  sourceKind: 'account' | 'provider';
  sourceId: string;
  targetAgentId: AgentId;
  route: 'config_sync' | 'native_endpoint' | 'local_bridge';
  status: 'applying' | 'active' | 'needs_attention';
  ruleId: string;
  ruleVersion: string;
  generatedProviderId?: string | null;
  localPort?: number | null;
  autoStart: boolean;
  lastErrorCode?: string | null;
  createdAt: string;
  updatedAt: string;
};

type AdapterRouteAnalysis = {
  route: 'config_sync' | 'native_endpoint' | 'local_bridge' | 'unsupported';
  support: 'stable' | 'experimental' | 'unsupported';
  reason: string;
  actions: AdapterAction[];
  limitations: string[];
  evidence: { label: string; url: string; verifiedAt: string }[];
};

type AdapterBridgeRuntimeStatus = {
  profileId: string;
  state: 'starting' | 'running' | 'stopping' | 'stopped' | 'error' | 'degraded';
  port?: number | null;
  endpoint?: string | null;
  startedAt?: string | null;
  upstreamStatus?: string | null;
};
```

数据库保存 profile、规则版本、生成 Provider 关联和最近运行摘要；不复制 Connection 凭据。运行时句柄、监听 socket、取消令牌只存在内存。

### 7.2 前端端口

在 `src/lib/backend/contracts/adapter.ts` 增加独立端口，并接入 `Backend.adapter`：

```ts
export interface AdapterPort {
  analyze(request: AdapterRouteRequest): Promise<AdapterRouteAnalysis>;
  plan(request: AdapterRouteRequest): Promise<AdapterApplyPlan>;
  listProfiles(filter?: AdapterProfileFilter): Promise<AdapterProfile[]>;
  apply(request: AdapterApplyRequest): Promise<AdapterApplyResult>;
  remove(profileId: string): Promise<void>;
  startBridge(profileId: string): Promise<AdapterBridgeRuntimeStatus>;
  stopBridge(profileId: string): Promise<AdapterBridgeRuntimeStatus>;
  getBridgeStatus(profileId: string): Promise<AdapterBridgeRuntimeStatus>;
  setBridgeAutoStart(profileId: string, autoStart: boolean): Promise<AdapterProfile>;
}
```

当前端口只包含已进入工作区的能力。upstream/target probe 与 recent events 仍是后续目标，不应作为现有 API 写入调用方。

约束：

- 页面不直接 `invoke`；Tauri 命令仅由 `src/lib/backend/tauri/` 调用。
- `dev:mock` 提供独立的 adapter mock 和 reset；生产 build 不包含 mock。
- 非 Tauri 生产环境明确显示 unavailable。
- 错误统一为稳定 `code + message + details? + retryable`，解析留在 adapter 层，页面不猜测不同后端 envelope。

## 8. 日志与诊断

### 8.1 延续现有日志体系

继续使用 agenthub-core 的 tracing、按日日志、`log_level`、`log_retention_days`、`redact_text` 和 `redact_json`。新增 target：

| target | 用途 |
|---|---|
| `core.adapter` | analyze、plan、apply、start/stop、补偿、目标配置验证 |
| `core.adapter.protocol` | 协议转换阶段、SSE/工具事件统计、映射失败 |

稳定字段：

| 字段 | 含义 |
|---|---|
| `module` | `core.adapter` / `core.adapter.protocol` |
| `code` | 稳定错误码，如 `adapter.unsupported`、`adapter.port_in_use`、`adapter.upstream_auth` |
| `op` | `analyze`、`apply`、`start`、`stop`、`probe`、`translate`、`rollback` |
| `profile_id` | Adapter profile id |
| `agent` | 目标 Agent id |
| `route` | config_sync/native_endpoint/local_bridge |
| `source_protocol` / `target_protocol` | 协议标识 |
| `request_id` | 本地生成的关联 id；向上游传递时遵守对方 Header 规则 |
| `upstream_status` | 上游 HTTP 状态，可选 |
| `elapsed_ms` | 操作或请求耗时 |
| `outcome` | success/error/cancelled/rolled_back |

### 8.2 级别

- `info`：profile 应用、启动、停止、恢复、探测结果与请求摘要。
- `warn`：能力降级、端口冲突后重新分配、上游限流、自动恢复失败但可重试。
- `error`：授权失败、协议转换失败、配置写入失败、补偿失败。
- `debug`：阶段与字段级映射结果，只记录字段名/数量/类型，不记录值。
- `trace`：SSE 事件类型与序号，仍禁止正文、工具参数和凭据。

禁止记录：Authorization、Cookie、完整 API Key/OAuth token、原始凭据 JSON、请求/响应正文、system prompt、用户消息、工具参数/结果、图片内容。模型名、token 计数、状态码和耗时可以记录。

### 8.3 页面日志体验

Adapter 详情只展示最近 5 条结构化事件：时间、阶段、结果、耗时、request id、短错误。提供：

- `复制 request id`；
- `查看诊断`（脱敏详情）；
- `打开日志目录`；
- `重新测试`。

MVP 不做全文搜索、自动滚动、错误文件下载、方法/路径筛选和清空日志。后续若建设全局 Logs 页面，再复用参考项目的 cursor 分页、8 秒轮询、`cursorReset` 恢复和“接近底部才自动跟随”等交互。

## 9. 对重点参考项目的取舍

`Cli-Proxy-API-Management-Center` 是 Management API WebUI，不是 CLIProxyAPI 或协议代理实现。其 `/api-call` 用于管理端代发最小测试请求和模型发现，不能证明它实现了 SSE 翻译、监听端口、协议路由或 OAuth 跨客户端复用。

重点阅读的参考源：

| 文件/模块 | 参考内容 |
|---|---|
| `src/pages/OAuthPage.tsx`、`features/authFiles/hooks/useAuthFilesOauth.tsx` | OAuth 卡内状态、轮询、手动回调 fallback、不支持与故障的区分 |
| `features/providers/components/ProviderSheet.tsx` | detail/create/edit 三态、dirty 保存和未保存离开保护 |
| `features/providers/sheets/forms/BaseProviderForm.tsx` | 字段附近的连接测试反馈 |
| `services/api/providers.ts` | 保存前读取最新配置、稳定身份冲突检测、保留未知字段 |
| `features/providers/sheets/forms/useConnectivityTest.ts` | 按 Chat/Responses/Messages 等目标协议发送最小探测请求 |
| `services/api/apiCall.ts` | 管理端测试代发的请求/响应 envelope；明确不是 proxy runtime |
| `services/api/logs.ts`、`pages/LogsPage.tsx` | cursor/after 增量日志、request id 下钻、轮询恢复 |
| `features/providers/useProviderWorkbench.ts` | mutation guard、完成后 refetch；其多 Provider 联动只作反例，不复制到 MVP |

| 参考能力 | 决策 | AgentHub 落法 |
|---|---|---|
| Provider 详情/创建/编辑 Sheet，dirty 才保存 | 采用其状态模型 | Adapter 使用现有 Dialog 承载详情/编辑，保留 dirty 与未保存离开确认 |
| OAuth 卡内 URL、等待、回调、成功/失败状态 | 简化采用 | 只在来源连接尚未完成时原位显示；成功后继续创建或回 Connections |
| 字段旁连接测试状态 | 采用 | 上游/目标格式分开测试，输入变化清除旧结果 |
| 保存前 DiffModal | 简化采用 | 先摘要，按需展开结构化 diff，不引入全屏编辑器 |
| 先取最新配置、保留未知字段、目标变更时报冲突 | 采用并加强 | 前端经 ConfigPort 展示 plan；core 由 ConfigurationService 规划、ProviderService 单次事务应用并负责备份/验证，外层 saga 负责补偿 |
| 协议化最小探测和模型发现 | 采用 | 非流式小请求；模型发现失败不阻断手工配置 |
| 日志 cursor、request id drilldown | 延后采用 | MVP 仅最近事件；全局 Logs 页面再做 cursor/过滤 |
| Providers 多栏工作台、240px 分类左栏 | 不采用 | Adapter 单列列表 + 现有 Dialog |
| AuthFiles 340px 卡片网格、批量操作 | 不采用 | 复用 Connections 紧凑 ListRow |
| Provider priority/weight/cooling/赞助商多协议联动 | 不采用 | 超出个人本地 Adapter MVP |
| 自定义 SCSS token 与成功率色谱 | 不采用 | 沿用 AgentHub Tailwind token、StatusDot、Badge |
| `/api-call` 视为代理服务 | 明确拒绝 | 新增真实 loopback BridgeRuntime 和 protocol 层 |

## 10. 实施顺序

### Phase 0：规则与预览（已落地）

- contracts、mock、`analyze`、`plan`、来源/目标选择、结果解释和应用预览已接线。
- 普通 Apply 只开放后端显式 `canApply=true` 的规则；当前写入范围见[实现矩阵](provider-api-oauth-adaptation.md#4-当前实现矩阵)。
- 其它结果仍可用于解释兼容路径，但未显式返回 `canApply=true` 时必须 fail-closed。
- 仅预览规则（如 Pi `config_sync`）不产生写入、不启动本地服务。

### Phase 1：首条真实桥接（工作区已接线，发布前待 dogfood）

- core 已有 `BridgeRuntimeHost`、loopback token、实例 start/status/stop/shutdown，以及 OpenAI Responses ↔ Chat Completions 的文本、SSE、工具、用量、停止原因和错误映射 fixtures。
- Tauri `AppState` 持有 host；controller 已实现 apply/start/stop/status、健康检查、失败补偿、凭证漂移 stop→restart、端口占用 rebind、opt-in auto-start restore；`ExitCoordinator` 负责退出 drain；Adapter 页面已有对应状态与操作控件。
- Codex `local_bridge` 已由 route plan 开放 Apply（实验性），规则状态见[实现矩阵](provider-api-oauth-adaptation.md#4-当前实现矩阵)。**发布前**仍需实机验收：密钥轮转、端口冲突、长流/工具闭环、托盘退出 drain。
- 默认 `auto_start=false`；用户可在成功启动后自行打开自动恢复。

### Phase 2：Claude Code 桥接

- 增加 Anthropic Messages ↔ OpenAI Chat Completions。
- 验收 OpenAI-compatible API Key → Claude Code。
- 补充工具 schema、缓存/思考能力降级提示。

### Phase 3：按证据扩展

- 只有新组合有官方文档、兼容规则、协议 fixtures 和端到端测试后才开放。
- 根据真实诊断需求决定是否建设全局 Logs 页面。
- WebSocket、厂商原生工具、复杂多模态均独立评审，不因“协议大致兼容”自动开启。
- 协议/厂商扩展与进程迁移分别推进；不得以“sidecar 已存在”为由绕过规则证据、fixtures 或 fail-closed 门禁。

### Runtime 进程迁移轨

sidecar 目标已经确认，但不进行 big-bang 重写：

1. **建立进程缝**：把 Tauri controller 中的 `local_bridge` 编排抽成 Tauri-neutral application/control contract，继续使用 in-process host，产品行为不变。
2. **可回滚 sidecar**：新增同包 `agenthub-adapterd`、本地 IPC、单实例、shared/exclusive schema lease 与版本/schema 握手；以内部 rollout mode 二选一启用 in-process 或 sidecar，禁止双 host。
3. **sidecar 成为唯一 owner**：GUI 只保留 control client，退出 GUI 不再停止 Bridge；完成更新前 running-set 恢复、卸载前 live loopback 解除、后台启动和 CLI 接线后，再删除进程内兼容路径。

每一阶段的验收门槛、回滚条件和故障注入矩阵以 [adapter-sidecar-design.md](adapter-sidecar-design.md) 为准。OS 系统服务、远程控制面与多机高可用继续单独评审。

## 11. 测试与验收

### 11.1 Core/协议测试

- 每个方向使用固定 JSON/SSE fixtures，生产代码与测试文件分离。
- 覆盖文本、多轮、工具定义、并行工具调用、工具结果、usage、stop reason、错误、取消、SSE 分片边界和 Unicode。
- 验证未知字段策略：允许透传的保留；不允许的产生 limitation 或 fail-closed。
- 验证任何日志中不存在 API Key、Authorization、prompt、工具参数和响应正文。

### 11.2 生命周期/事务测试

- 端口冲突、上游 401/429/5xx、健康检查失败、目标配置外部变更。
- 启动成功但配置写入失败时停止实例并恢复快照。
- 配置写入成功但 read-back 不一致时恢复快照。
- 补偿自身失败时进入 `needs_attention` 并给出稳定恢复动作。
- 应用重启后只恢复 `auto_start` profile；恢复失败不污染已有当前 Provider。
- 当前进程内模式：关闭窗口进入托盘后端点持续可用；显式退出执行 drain 并释放端口。
- 当前进程内模式：`close_to_tray=false` 且有活跃桥接时，窗口关闭必须出现“隐藏到托盘 / 停止桥接并退出 / 取消”，不能直接结束进程。
- sidecar 模式：普通 GUI 退出后端点持续可用；只有显式停止、更新、卸载或 sidecar 自身关停才 drain。
- sidecar 模式：GUI 崩溃不影响 listener；sidecar 崩溃后必须明确呈现 `host_unavailable`，重启与恢复不得产生第二个 listener。
- 第二实例、重复恢复、IPC 重试和并发 start 不能创建重复监听器；版本不兼容的客户端不得执行 mutation。
- 宿主未运行时生成的 loopback Provider 明确返回连接失败；不得静默绕过到其他上游。

### 11.3 前端测试

- loading/error/empty/disconnected/unsupported/active/degraded/needs_attention 全状态。
- 来源或配置变更后测试成功状态复位。
- 未保存变更关闭 Dialog 的确认。
- 应用预览只显示脱敏值；unsupported 没有强制应用入口。
- 单行 start/stop 不阻塞其他行。
- 非 Tauri 生产环境显示 unavailable，测试固定使用 mock backend。

### 11.4 MVP 验收标准

1. 用户可在 3 个主要选择内完成 `Connection → 目标 Agent → 适配路径`。
2. 直连路径不启动任何本地服务。
3. Kimi Code → Codex 能完成文本流和至少一次工具调用闭环；当前模式关闭主窗口进入托盘后继续请求，sidecar 目标态退出 GUI 后仍继续请求。
4. 应用前能看到写入对象、服务影响和能力损失。
5. 任一失败均可回到操作前状态；补偿失败有明确恢复动作。
6. Connections 可看到生成的目标 Provider，现有切换/备份体验不分叉。
7. 默认日志不含请求正文或凭据，能以 `profile_id + request_id + code` 完成排查。

## 12. 建议文件落点

```text
crates/agenthub-core/src/
├─ models/adapter.rs
├─ services/adapter_route_service.rs
├─ services/adapter_apply_service.rs
├─ services/adapter_secret_resolver.rs
├─ services/adapter_bridge_service.rs
├─ storage/adapter_profile_repo.rs
├─ storage/migrations/00012_adapter_profiles.sql
├─ storage/migrations/00013_adapter_bridge_profiles.sql
└─ bridge/
   ├─ mod.rs
   ├─ runtime.rs
   ├─ host.rs
   ├─ types.rs
   └─ protocol/
      ├─ mod.rs
      ├─ chat.rs
      ├─ responses.rs
      └─ fixtures/

src-tauri/src/commands/adapter.rs
src-tauri/src/commands/adapter/tests.rs

crates/agenthub-core/src/lib.rs             # service/repo wiring
src-tauri/src/state.rs                      # 当前：托盘进程持有 BridgeRuntimeHost
src-tauri/src/exit_coordinator.rs           # 当前：统一退出确认、drain 与 shutdown
src-tauri/src/tray.rs                       # 当前：退出动作委托 ExitCoordinator

src/lib/backend/
├─ contracts/adapter.ts
└─ tauri/adapter.ts

src/dev/mocks/adapter.ts
src/pages/adapter/
├─ index.tsx
└─ index.test.tsx
```

以上是当前工作区的实际落点。sidecar 目标态预计新增 `crates/agenthub-adapterd`、Tauri-neutral control/application contract 与 `src-tauri` IPC client，均尚未落地，具体边界见 [adapter-sidecar-design.md](adapter-sidecar-design.md)。后续拆分页面组件或扩展协议文件时继续保持 service、runtime、protocol 与 UI 边界，不要求机械照搬最初的建议文件名。
