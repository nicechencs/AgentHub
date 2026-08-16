# Adapter sidecar：稳定目标架构与迁移契约

> 状态：目标架构（已决定；Phase 1 控制契约已落地，sidecar 本体尚未实现）  
> 日期：2026-08-12（进度回写：2026-08-16）  
> 范围：跨 Agent Adapter 的 `local_bridge` 长驻运行时、协议数据面与生命周期控制面。  
>
> **进度摘要（区分契约与二进制；不要倒读为 sidecar 已迁移）：**  
> - **Phase 1 控制契约：已落地** — `crates/agenthub-core/src/adapter_control/{mod,contract,coordinator,status}.rs` + `src-tauri/src/adapter_control_host.rs`（`DesktopAdapterControl`，仍 in-process）。  
> - **Phase 2 sidecar：未开工** — 无 `crates/agenthub-adapterd/`、无 IPC client、无 `DataStoreBootstrap` / `SchemaGenerationLease` 实现（仅本文出现）。  
> - 协议数据面内核：仍在 `agenthub-core`（纯函数 Messages ↔ IR ↔ Responses 与 `RetryGate` 单测）。listener 生命周期仍随 GUI 宿主进程。  
> - Codex OAuth → Claude：现为 **experimental bind**（Responses）。对照 `domain/protocol_graph/adapter_capability_matrix.rs` 的 `codex-subscription-to-claude-responses-v1`（`can_apply=true`，gates 全开）。**不要**再写文首旧门禁「仍 unsupported / canApply=false」。App Server 边（`codex-subscription-to-claude-app-server-v0`）仍关闭。  
> - 票、绑定、Connections / Account / Provider / ActiveBinding：**始终由 core services owner**，不迁入 sidecar。生成 Provider 是绑定的私有投影，不是第二套钱包。  
> - 凭据落盘加密范围外。国产 OAuth 适配 / 转 API 产品不做。

## 1. 决策摘要与现状边界

本项目采用同包、当前用户级的 `agenthub-adapterd` sidecar，承载 `local_bridge` 的长驻 Runtime。它让 GUI 退出、崩溃、更新或重启不再天然等同于桥接 listener 停止；GUI 与 CLI 都成为管理客户端，而不是 Runtime owner。

这是一份**目标设计**，不能倒读为当前实现。当前 MVP 中：

- `adapter_control` 契约已落地；`DesktopAdapterControl` 仍是 in-process host，`BridgeRuntimeHost` 仍由 Tauri `AppState` 持有；
- `src-tauri` 的 controller 仍编排 host、`AdapterBridgeService` 与 `ProviderService`（经 control contract）；
- Runtime 的协议转换实现在 `agenthub-core`，但 listener 的生命周期仍随 GUI 宿主进程；
- `local_bridge` 已是独立路由，`config_sync` 和 `native_endpoint` 不依赖这个 host；
- **不要**把 sidecar 写成已迁移：无 `agenthub-adapterd`、无 IPC client、无 schema lease 实现。

目标实现中，只有下列职责移入 `agenthub-adapterd`：跨 Agent Adapter 的 `local_bridge` 长驻 Runtime、loopback 协议数据面、完整 `local_bridge` saga，以及其恢复、健康检查和进程生命周期。不要以“sidecar”之名把全部 Adapter、Connections 或账户领域拆成另一个进程。

`agenthub-adapterd` 是随桌面应用发布、由当前用户运行的同包进程；它**不是** Windows Service、macOS LaunchAgent、systemd service 或需要提权的系统守护进程。`auto_start` 仅表达 sidecar 每次启动后是否恢复某 profile，绝不静默开启操作系统开机自启。

凭据落盘加密不属于本项目范围，既不是本设计的风险，也不是迁移任务。

## 2. 术语

| 术语 | 含义 |
|---|---|
| `AgentAdapter` | 每个 Agent 的配置路径、格式与能力差异实现；不是一个常驻进程。 |
| Adapter 产品功能 | 复用已有 Connection，为目标 Agent 规划并应用 `config_sync`、`native_endpoint` 或 `local_bridge`。 |
| `local_bridge` | 上下游协议不一致时，以 loopback listener 转换请求/流响应的 Adapter route。 |
| profile | 一条 `bridge` 绑定的运行时材料：loopback、本地 bearer、与目标 live 的关联；以 `profile_id` 标识。目标态不是钱包里的新票，见 [connection-binding-model.md](connection-binding-model.md)。 |
| sidecar | `agenthub-adapterd` 用户级进程，唯一持有目标架构中的 listener 与观测运行状态。 |
| 控制面 | GUI/CLI 到 sidecar 的管理 IPC，例如 apply、start、stop、status。 |
| 数据面 | 每个 profile 的 loopback HTTP listener 与目标 Agent 之间的真实请求/流式响应。 |
| durable state | SQLite 中可恢复的 profile、saga 和关系记录。 |
| observed state | sidecar 当前进程实际观察到的 listener、健康、epoch 与在途请求状态。 |
| LiveWriteAuthority | 对目标 Agent live 配置写入的跨进程协调权；仅由 core 服务经受控接口使用。 |

## 3. 动机与非目标

### 3.1 动机

- `local_bridge` 的可用性不应依赖某个 GUI 窗口、WebView 或 Tauri 进程仍存活。
- GUI/CLI 需要相同的规则、错误和恢复行为，不能复制一套桥接编排。
- 生成的 Provider 指向本地 listener；listener、Provider live 配置与 profile 必须以一个可恢复的 saga 保持一致。
- 将协议流量与 GUI 生命周期隔离，同时仍保持个人桌面产品的无提权安装和简单运维。

### 3.2 非目标

- 不把 `config_sync`、`native_endpoint` 迁入 sidecar；这两类 route 不要求常驻 Runtime。
- 不把 `Connection`、`Account`、`Provider` 或 `ActiveBinding` 拆到 sidecar 私有存储。它们仍由 `ConnectionService`、`ProviderService`、`AccountService` 作为领域 owner。
- 不在 sidecar 直接写 SQLite 表、目标 Agent 的 live 文件或凭据文件。sidecar 必须通过 Tauri-neutral 的 core application/control contract 调用服务。
- 不将 sidecar 发展为远程 API、LAN listener 或多用户服务；数据面仅监听 `127.0.0.1` / `::1`。
- 不自动注册 OS 开机自启、不实现系统服务安装，也不承诺跨用户共享。
- 不改变既有凭据存储决策；凭据落盘加密范围外。

## 4. 当前与目标架构

### 4.1 当前实现（MVP）

```text
GUI / Tauri commands
  └─ Tauri AppState
      ├─ Hub（core services）
      └─ BridgeRuntimeHost（当前 runtime owner）
          └─ per-profile loopback listener / protocol conversion

SQLite + Agent live files
  └─ ProviderService / ConnectionService / AccountService
```

当前 `BridgeRuntimeHost` 由 Tauri `AppState` 持有这一事实必须保留在任何现状说明、故障提示和迁移代码中。普通 CLI 也不能假装存在后台 daemon；当 GUI host 不在时，当前 `local_bridge` 的持久化 profile 可存在，但 host 不可用。

### 4.2 目标架构（稳定形态）

```text
                    control plane (local IPC)
GUI client  ──────────────────────────────────┐
CLI client  ──────────────────────────────────┼──> agenthub-adapterd
                                                │     ├─ control/application contract
                                                │     ├─ local_bridge saga + recovery
                                                │     ├─ BridgeRuntimeHost / profile lifecycle
                                                │     └─ per-profile data-plane listeners
                                                │              └─ loopback HTTP + protocol conversion
                                                │
SQLite WAL <── shared source of durable truth ──┘
   │
   └─ core services: ConnectionService / ProviderService / AccountService
         └─ LiveWriteAuthority → generated Provider / target Agent live config

config_sync / native_endpoint: GUI or CLI → same core rules/services; no sidecar dependency
```

sidecar 内部可复用 `BridgeRuntimeHost` 的运行时语义，但它的 control/application contract 必须是 Tauri-neutral。`src-tauri` 只实现 IPC client、GUI UX 与进程管理；不能让 Tauri command 成为 sidecar 的业务真相。

## 5. 目标态所有权矩阵

| 资源或决策 | 唯一 owner / writer | sidecar 的权限 | GUI / CLI 的权限 |
|---|---|---|---|
| `local_bridge` profile durable mutation | 进程 owner：sidecar；领域 API：`AdapterBridgeService` | 唯一进程 writer；必须通过 core contract 写入 | 仅查询或经 IPC 请求 mutation |
| profile runtime observed state | sidecar | 唯一 observed truth | 读取 snapshot / 订阅事件，不从 DB 推断运行中 |
| listener、端口 reservation、drain | sidecar | 唯一 owner | 请求操作、显示结果 |
| 连接、账户、Provider、ActiveBinding | `ConnectionService` / `ProviderService` / `AccountService` | 通过服务读取、受控组合调用 | 通过各自 core API/IPC 使用 |
| generated Provider 与 live config 切换 | `ProviderService` + `LiveWriteAuthority` | 调用受控 application service；不得直接写表或文件 | 调用服务或请求 sidecar 组合操作 |
| Adapter route/rule 决策 | core route/application rules | 使用同一规则版本 | 使用同一规则版本 |
| `config_sync` / `native_endpoint` | 既有 core 服务 | 无需介入 | 直接调用 core 服务 |
| SQLite schema generation | 前台 `DataStoreBootstrap` + canonical data-dir schema lease | sidecar 持 shared lease、只做 compatibility check，永不自行 migrate | GUI/CLI 同样持 shared lease；migration coordinator 取得 exclusive lease 后才迁移 |

SQLite 以 WAL 模式作为共享的**持久化真源**，不是运行状态真源。它可记录最近一次观察、恢复意图或失败摘要，不能让任何客户端据此把“上次 running”展示为当前 listener 仍在运行。

## 6. 进程、模块与落点

`adapter_control` **已存在于 core**（Phase 1 in-process host）；`adapterd` / IPC client **仍是目标**，不要把下列目录整表读成「现已迁到 sidecar」：

```text
crates/
  agenthub-core/
    src/adapter_control/       # 已落地：Tauri-neutral DTO、application contract、进程内 coordinator
    src/services/              # Connection/Provider/Account/LiveWriteAuthority 等领域 owner
    src/bridge/                # runtime 与协议转换可复用实现
  agenthub-adapterd/           # 目标：sidecar binary、IPC server、instance lock、recovery supervisor（未开工）
src-tauri/
  src/adapter_control_host.rs  # 已落地：DesktopAdapterControl（in-process）
  src/adapter_sidecar_client.rs # 目标：spawn/connect/handshake/control IPC client（未开工）
  src/...                       # 目标：UI command 只映射到 client，不再持有 runtime host
```

`agenthub-adapterd` 用 canonical data directory 推导 SQLite、日志、IPC 名称和 instance lock；不得接受含糊的相对目录启动。GUI 或 CLI 先 canonicalize data directory，再连接或启动同一实例。sidecar 的 argv 只允许定位数据目录、日志级别和受控 bootstrap 参数，不能携带原始凭据、OAuth token 或 profile secret。

## 7. 控制面 IPC 契约

### 7.1 Transport 与边界

优先 transport：Windows named pipe；Unix domain socket。socket/pipe ACL 必须限制为当前用户。loopback control plane 只是无法提供前两者时的备选，须随机控制 token、仅 bind loopback、禁止写入 URL/日志/argv，并与每 profile 数据面端口严格分离。

控制面不是数据代理：所有真实模型请求走 profile listener；控制 IPC 不承载流式模型内容或原始凭据。sidecar 按 `connection_id` 在 core 内部解析与刷新凭据，调用者只传稳定 ID 和必要的非敏感预期 revision。

### 7.2 Envelope、幂等与错误

所有请求使用结构化 envelope：

```json
{
  "protocol_version": 1,
  "client_version": "x.y.z",
  "request_id": "UUID",
  "client_instance_id": "UUID",
  "expected_sidecar_instance_id": "UUID or null",
  "expected_epoch": 42,
  "operation": "handshake|status|list_statuses|apply|start|stop|remove|set_auto_start|reconcile|prepare_update|prepare_uninstall|shutdown",
  "payload": { "...": "non-secret fields only" }
}
```

首次 handshake 不携带 `expected_sidecar_instance_id` / `expected_epoch`；后续 mutation 使用握手结果，防止把延迟请求提交给已经重启的新实例。响应回显 `request_id`，含 sidecar `instance_id`、`epoch`、兼容窗口、`operation_state`、结构化 result 或 error。sidecar 在 durable operation journal 中保留 mutation 的 `request_id`、canonical payload hash 与最终结果，直到超过明确的保留窗口；相同 request ID + 相同 payload 必须返回同一结果，不得重复写 profile、重复切换 Provider 或重复启动 listener。相同 ID 而 payload 不同返回 `request_id_reused`。

错误至少区分：`version_incompatible`、`host_unavailable`、`not_found`、`config_changed`、`rule_changed`、`invalid_state`、`dependency_conflict`、`port_unavailable`、`operation_in_progress`、`needs_attention`、`shutdown_in_progress`、`internal`。错误应有稳定 code、可展示 message、retryability 和安全诊断 ID；不得序列化 secret、token、完整上游 URL query 或原始配置内容。

### 7.3 最小 API 与握手

| API | 关键输入 | 结果 / 语义 |
|---|---|---|
| `handshake` | client/protocol versions、canonical data-dir identity、当前 schema version | server version、协议兼容范围、可读 schema min/max、`instance_id`、`epoch`、capabilities；不兼容时拒绝 mutation |
| `status` / `list_statuses` | optional profile ID | 当前 observed snapshot；没有 sidecar 时报告 `host_unavailable`，不得伪造 running |
| `apply` | profile intent、`connection_id`、target Agent、`revision`、`rule_version` | 完整 apply saga；创建/更新后使 generated Provider 安全生效 |
| `start` | profile ID、expected revision/rule version | 幂等地确保 running listener，返回 endpoint health |
| `stop` | profile ID、drain policy | 幂等地 drain 并停止 listener，保留 profile |
| `remove` | profile ID、dependency disposition | 完整反向 saga；先安全解除 generated Provider / 依赖关系，再删除 profile |
| `set_auto_start` | profile ID、boolean、revision | 只修改 restore intent，不登记 OS 自启 |
| `reconcile` | profile ID / all | 处理恢复、credential rotation 或 `needs_attention` 的受控收敛 |
| `prepare_update` | target versions/schema、deadline | 关闭 admission，等待 mutation 收敛，持久化本次 running set，drain 后退出；供版本化替换与失败回滚 |
| `prepare_uninstall` | profile disposition、deadline | 先经 ProviderService 安全解除 active generated Provider/live binding，再 drain；无法安全解除时阻止卸载 |
| `shutdown` | reason: explicit-exit/system | admission close、drain、停止所有 listener、退出进程；不得冒充 update/uninstall saga |

每个 mutation（包括 `set_auto_start`、`reconcile`）必须带 `request_id`，并在执行前及进入 live-write 临界区前重新读取/校验 profile `revision` 与 route `rule_version`。计划时的规则或配置已变，返回 `rule_changed` / `config_changed`，要求调用方刷新后重新提交，禁止覆盖未知字段。`start`、`stop`、`remove` 均必须是幂等操作：已在目标状态时返回成功的终态；已删除 profile 的重复 `remove` 返回成功 tombstone 或明确同等成功结果。

durable profile 列表仍可由 GUI/CLI 通过 core 只读 query 获取；运行中的 sidecar 只补充 observed status。即使 sidecar 不可达，UI 也必须继续显示持久化 profile，并为所有 `local_bridge` 行派生 `host_unavailable`，不能把列表误显示为空。

握手必须先于 mutation。协议或核心规则兼容窗口不重叠时，sidecar 允许只读状态（若 payload 可安全解析），但拒绝 mutation；GUI/CLI 显示“需升级到兼容版本”，绝不降级到 GUI 内 host 或静默 mock。

## 8. 状态模型与实例身份

将状态分为四类，API 不得混合它们：

| 类别 | 示例 | 真源与规则 |
|---|---|---|
| profile durable | route、target、connection ID、generated provider、revision、saga/tombstone | SQLite；只由 sidecar 对 `local_bridge` 写入 |
| restore intent | `auto_start` | SQLite；sidecar 启动后尝试恢复，不代表 OS 启动项 |
| runtime observed | starting/running/draining/stopped/degraded/error、health、bound address、in-flight | sidecar 内存和 IPC snapshot；DB 仅可存 last observation 供诊断 |
| host derived | `host_unavailable` | client 无法连接匹配 data-dir 的健康 sidecar 时推导；不是可被 DB 设置为 running 的状态 |

durable profile 生命周期与 runtime observed 状态使用两套状态机：

```text
durable profile:
draft → applying → active → removing → removed
          │          │
          └──────────┴→ needs_attention（补偿不完整或依赖无法自动收敛）

runtime observed（每个 sidecar instance 重新建立）:
unknown/stopped → starting → running ↔ degraded
                       │         │
                       └→ error  └→ draining/stopping → stopped

client derived:
control plane 不可连接 + durable local_bridge profile → host_unavailable
```

`instance_id` 在 sidecar 新进程启动时生成；`epoch` 在该 instance 接管或恢复 runtime generation 时单调递增。所有 observed snapshot 和 mutation response 回显二者。client 收到不同 instance/更高 epoch 时丢弃旧事件并重新拉取状态；server 收到错误 epoch 的延续/取消请求时返回 stale-instance。sidecar 每个 canonical data dir 只能有一个实例。

## 9. 单实例、并发与锁

sidecar 使用 canonical data dir 派生的 instance lock。启动流程是：尝试独占 lock → 若失败连接控制 IPC 并 handshake → 健康实例存在则作为 client 返回；若 IPC 不通，按锁记录的 PID、启动时间、实例 ID 与 OS liveness 做 stale 判定 → 在确认旧进程已死且锁不可再被其持有后安全回收 → 创建新 instance/epoch。不得仅因“socket 连不上”就删除 lock 或杀死未知 PID；竞态回收必须重新取得 lock 并再次验证。Windows/Unix 的 lock 实现应利用 OS 文件锁/命名对象语义，锁记录仅作诊断，不是唯一裁决。

并发 mutation 不靠 SQLite 的全库写锁序列化。sidecar 要有 lifecycle admission gate：`shutdown` 关闭 admission 后，新 mutation 快速返回 `shutdown_in_progress`；已有操作可完成或进入补偿。各操作必须遵守固定锁序：

```text
lifecycle admission
  → profile gate
    → target-agent gate
      → cross-process LiveWriteAuthority
        → short SQLite transaction
```

- 同 profile 的操作经 profile gate 串行；不同 profile 可并发，除非目标 Agent 相同。
- `target-agent gate` 使同一 Agent 的 live 切换和补偿有序。
- `LiveWriteAuthority` 是跨进程的最终 live-write 协调，必须由 core 的 `ProviderService`/相关服务获取或传入受控 guard；sidecar 不实现另一套绕过锁的写入。
- SQLite transaction 只包短数据库读写、revision 比较、operation journal 或状态提交。**不得跨网络 I/O、listener 启动、health probe、drain 或 Agent 文件 I/O 持有 DB transaction。**
- 不允许反向加锁；若操作需要重新计划，应释放到安全边界后 retry，而不是尝试升级锁。

### 9.1 SQLite schema bootstrap、租约与迁移权威

WAL 和 `busy_timeout` 只能缓解普通读写竞争，不能充当 schema migration 协调。仅在启动瞬间持有 migration lock 也不够：早已打开数据库的旧 GUI/CLI 仍可能跨越 migration 继续操作。目标态必须把当前 `Database::open` 的“打开即迁移”拆成两个入口，并引入覆盖整个 Database handle 生命周期的 schema-generation lease：

- `DataStoreBootstrap::open_and_migrate`：只供前台 bootstrap coordinator 使用。桌面正常启动/更新时由 GUI 或 updater 担任；无 GUI 的显式 CLI 启动可以担任。
- `Database::open_checked`：供 sidecar 以及 migration 已完成后的 GUI/CLI 使用；读取 schema version 并检查兼容范围，**永不自行执行 migration**。
- `SchemaGenerationLease`：每个持有打开 Database handle 的 GUI、CLI、sidecar 进程都必须同时持有 shared lease，直到关闭连接。migration coordinator 必须取得 exclusive lease；exclusive lease 与任何已有/新 shared lease 互斥。

schema lease 由 canonical data dir 派生，独立于 sidecar instance lock 和 `LiveWriteAuthority`，实现应使用 OS shared/exclusive lock 或等价命名对象。正常进程先取得 shared lease，再读取 schema 并以 `open_checked` 打开数据库；兼容检查失败时不创建业务 service。migration coordinator 先关闭自身 Database handle并释放自身 shared lease，再请求其它本产品进程 quiesce；sidecar 通过 `prepare_update` 退出，GUI/长期 CLI 必须结束数据库操作、关闭连接并释放 shared lease。随后 coordinator 才竞争 exclusive lease。仍有旧进程持 lease 时在有界等待后提示用户关闭/重试，禁止强删 lease、杀死未知 PID 或带着活跃旧连接强行迁移。

exclusive lease 持有期间，新 GUI/CLI/sidecar 只能等待，不能打开 DB。迁移提交并释放 exclusive lease 后，所有进程必须重新取得 shared lease、重新读取 schema generation 和兼容范围，再新建连接；不得复用迁移前的连接、statement、repo 或缓存。旧 binary 若不支持新 schema，在重新取得 shared lease后必须 fail-closed。

只要目标版本需要改变 schema，bootstrap coordinator 必须先通过控制面关闭旧 sidecar admission，等待/补偿所有 mutation，并执行 `prepare_update` 停止旧进程，再等待所有其它 shared leases 释放；禁止让任何旧二进制一边访问数据库一边升级。随后 migration coordinator 在 exclusive lease 下、在事务内应用迁移，验证目标 schema/foreign keys，写入 schema generation，再释放 lease 并启动新进程。当前项目尚无自身 DB 备份能力；在引入任何不能完整包含于 SQLite 事务、或超出旧 binary `max_readable_schema` 的迁移前，必须先新增并实测可恢复的 DB 快照能力（可归入 BackupService），并在失败时恢复或保持 fail-closed；不得发布“部分迁移已可用”的 ready 状态。

控制面 handshake 同时交换当前 schema version 与双方支持的 `min_readable_schema` / `max_readable_schema`。范围不相交时，旧进程不得执行 mutation；sidecar 在 schema 过旧、过新或 migration generation 未 ready 时拒绝启动 listener。并发 GUI/CLI/sidecar 首启、迁移中崩溃、**已持 shared lease 的旧 GUI/CLI 遇到更新**、旧 sidecar + 新 GUI、旧 GUI + 新 sidecar都必须有故障注入测试。

## 10. `local_bridge` saga 与崩溃恢复

### 10.1 通用规则

`local_bridge` 的 apply/start/stop/remove 均是可重入、持久化 journal 的 saga。每个 phase 写入 operation record、资源身份、补偿意图和安全错误摘要；journal 不记录原始凭据。sidecar 崩溃后，新实例从 durable operation record 重建并执行确定的 resume/compensate，而不是根据 DB 中单个 `is_running` 字段猜测。

sidecar 只调用 core service 的受控组合入口完成 generated Provider 的创建、切换、恢复和删除。`ProviderService` 与 `LiveWriteAuthority` 始终是唯一 live 写 owner。尤其禁止：sidecar SQL 直写 `providers` / binding 表、直接改目标 Agent live 文件、或绕开 `ProviderService` 删除 generated Provider。

### 10.2 Apply / update

```text
admit → profile gate → validate request_id/revision/rule_version
  → core re-read connection + target capability + live revision
  → plan (preserve unknown fields; no secret in IPC)
  → reserve endpoint / start candidate listener
  → upstream probe + target-protocol probe
  → core creates or updates generated Provider (not current yet)
  → ProviderService safely switches generated Provider under LiveWriteAuthority
  → mark profile active + finalize journal
```

任一失败按相反顺序补偿：恢复 live 配置快照/Provider binding → 恢复或删除本次 generated Provider → stop candidate listener/release port → 提交失败与补偿结果。补偿不完整时进入 `needs_attention`，不伪装为 active。对更新，旧健康 listener 应尽可能持续服务，候选 listener 探测成功后再进行受控切换；不能做到时必须明确短暂不可用窗口。

### 10.3 Start / restore

`start` 重新 revalidate revision/rule version、source connection、target live config 与 generated Provider 关系，随后绑定 listener、解析 `connection_id` 对应凭据并执行健康检查。健康后 observed runtime 才报告 `running`。如果 generated Provider 已不再指向本 profile 或 source 被撤销，停止恢复并把 observed runtime 标记为 `error`；只有依赖无法安全自动收敛或补偿不完整时，durable profile 才进入 `needs_attention`，不能私自创建替代连接。

sidecar 启动时只恢复 `auto_start=true` 且依赖完整的 profiles；恢复操作同样通过 profile gate。手动 start 与启动恢复共享相同 admission/锁规则，杜绝重复绑定。`auto_start=false` 的 profile 保持 durable profile 不变，其 observed runtime 为 `stopped`。

### 10.4 Stop

`stop` 先关闭该 profile 接收新数据面请求，再等待有限 drain 窗口；到期后取消可取消的在途请求，关闭 listener，写入 stopped observed snapshot。它不删除 profile、connection 或 generated Provider，除非调用的是另一个显式 remove/cascade 流程。重复 stop 对已 stopped/stopping profile 都应安全返回一致结果。

### 10.5 Remove、依赖与 credential rotation

删除 source connection 时，若仍有 `local_bridge` profile 依赖，必须阻止删除并列出 profile，或要求调用方明确选择受控 cascade。cascade 顺序为停止/移除 profile → 经 `ProviderService` 安全回退或删除 generated Provider → 再删除 connection；不得留下指向无效 loopback endpoint 的 active Provider。

generated Provider 同样不得绕开 Adapter lifecycle 从通用删除入口直接删除。通用删除应阻止并引导到 `remove`，或调用相同的受控 saga。

credential rotation 以 source `connection_id` 变更事件或 revision 变化触发：profile gate 下受控 stop → 重新解析凭据 → start/probe → reconcile，并记录结果。不得热交换未经验证的上游 credential；失败时保留清晰的 degraded/error 语义及原可恢复配置。

### 10.6 崩溃恢复决策表

| 崩溃时刻 | 恢复动作 |
|---|---|
| listener 未启动、未写 Provider | 清理 reservation/journal，保留 draft 或报失败 |
| listener 已健康、尚未切 live | 停止候选 listener，清理未 current generated Provider |
| live 已切换、profile 未 final | 重新验证 listener；能恢复则 final，不能恢复则由 ProviderService 回滚 live 并标记 needs_attention |
| stop / remove 中 | 依据 journal 重新检查 listener 与 Provider；完成幂等 stop/remove 或进入 needs_attention |
| process 被强杀 / 系统断电 | 不保证 drain；下一次冷启动按 journal 与 `auto_start` 恢复，输出诊断关联 ID；受控更新另按一次性 update-resume manifest 恢复更新前 running set |

## 11. GUI、CLI、更新与系统生命周期

- GUI 启动：连接并 handshake 已有 sidecar；没有时启动同包 binary，等待 ready/handshake。launcher 必须赋予 sidecar 独立于 GUI 的用户级进程生命周期，不能放入“父进程退出即杀死全部子进程”的 job/process group。sidecar 自己在 recovery 后恢复 `auto_start` profiles，GUI 只观察结果，不负责触发恢复。失败显示 `host_unavailable` 和明确修复动作，不静默 fallback。
- GUI 退出、窗口关闭、WebView reload 或 GUI crash：**不停止 sidecar**；现有数据面持续运行。GUI 只丢失管理能力。
- 显式“停止适配并退出”：GUI 发 `shutdown(reason=explicit-exit)`，sidecar 关闭 admission、drain、停止 listener 后退出；GUI 再退出。普通“退出 GUI”不能隐式等价于此动作。
- 更新：updater 与 sidecar 先 handshake，并调用 `prepare_update`，不能把更新当普通 shutdown。sidecar 关闭 admission、等待所有 mutation 结束或完成补偿，然后把当时 observed 为 `running/degraded` 的 profile ID、revision、schema generation 和当前 instance 写入一次性 update-resume manifest；该集合独立于 `auto_start`，因此手动启动且 `auto_start=false` 的桥接也会在本次更新后恢复。manifest 必须存于数据库外的受控数据目录，使用版本化、校验和与原子替换；在新/回滚 runtime 完成全部接管前不得消费。

  binary 回滚受 schema 兼容性约束：若迁移后的 schema 仍不高于旧 binary 的 `max_readable_schema`，可直接回滚 binary；若超出范围，更新前必须已经在 exclusive schema lease 下建立并验证 pre-migration DB snapshot，失败时先恢复该 snapshot、验证 schema generation，再重启旧 binary。没有可验证 snapshot 时，禁止开始这种不兼容迁移。旧 binary 和 snapshot 保留到新 sidecar 完成 handshake、journal recovery 并恢复 update-resume set；成功后才消费 manifest，再按正常策略处理其它 `auto_start` profiles并按保留策略清理 snapshot。新版本失败、snapshot 恢复失败或新旧 runtime 都无法恢复某个仍为 current 的 generated Provider时，manifest 保留供诊断/重试，并由仍与当前 schema 兼容的 recovery binary 调用 ProviderService unavailable-profile reconcile，安全恢复已记录的前一 binding/live snapshot或解除 Adapter-owned loopback 投影，进入 `needs_attention`；若没有兼容 recovery binary 则保持 fail-closed 并阻止结束更新，禁止尝试用不兼容旧 binary 打开 DB，也禁止把死 endpoint 留作无提示的 current 配置。
- 卸载：安装器必须先调用 `prepare_uninstall`，不能只 drain listener。该 saga 先枚举所有仍为 current/live 的 generated Providers，在 `LiveWriteAuthority` 下通过 ProviderService 重新读取目标 live stable identity/revision，并与最后一次成功 apply/reconcile 记录的 expected revision 比较；只有仍确认是 Adapter-owned projection 时，才以 preserve-unknown 语义恢复 pre-adapter binding/live snapshot，或执行用户明确选择且可 read-back 验证的安全解除，不得自动猜选另一个 Connection。若检测到外部修改，返回 `config_changed` 并进入 `needs_attention`，不得覆盖用户改动。只有目标 live 配置与 ActiveBinding 均不再指向将被删除的 loopback listener 后，才能停止 sidecar、移除 binary 与 IPC/instance resources。缺少安全补偿材料、验证失败或补偿失败时阻止正常卸载；保留或删除 durable profile/用户数据仍由卸载 UX 单独选择，不能由 runtime shutdown 推断。
- 系统关机/注销：尽力走同一 shutdown 路径并给短 drain 预算；OS 强杀、崩溃和断电无法保证，依赖后续恢复。

当 sidecar unavailable 时，所有 `local_bridge` profile 显示派生的 `host_unavailable`；`config_sync` 与 `native_endpoint` 继续正常直连，不受 sidecar 影响。禁止 GUI 将 local_bridge 偷偷改为 direct、mock 或其他上游。sidecar crash 会终止其 listener，直至 sidecar 被重新启动；GUI crash 不应影响 listener。

## 12. 安全、隐私、日志与诊断

- 控制面采用当前用户 ACL；数据面仅 loopback，并继续使用每 profile 稳定本地访问 token。目标 Agent 只获得本地 token，不能获得上游 OAuth token。
- IPC payload、命令行参数、结构化日志、崩溃报告、operation journal 均不得包含原始 credential、refresh token、Authorization header、完整敏感 live config 或控制 token。
- sidecar 根据 `connection_id` 通过 core `AdapterSecretResolver` 等受控服务在内存中解析/刷新凭据；日志仅记录 connection/profile 的安全标识或 hash。
- 每条日志至少带 timestamp、instance ID、epoch、request ID、profile ID、target Agent、operation phase、诊断 ID 和脱敏错误 code。数据面请求日志默认不记录 prompt/body；stream 只记录计数、时长、终止原因与安全采样指标。
- `status --diagnose` / GUI diagnostics 应输出版本兼容性、IPC 可达性、instance/epoch、profile durable 与 observed 的差异、最近 saga phase、listener health、端口占用分类和已脱敏日志关联；不得导出 secret。

## 13. 三阶段迁移、验收与回滚

### Phase 1：抽离契约，不改变当前宿主（**已落地**）

工作（已完成）：将 Tauri controller 中的 `local_bridge` 编排抽为 Tauri-neutral core application/control contract（`agenthub_core::adapter_control` + `DesktopAdapterControl`）。当前 `BridgeRuntimeHost` **仍由 Tauri `AppState` 持有**，仅作为该 contract 的 in-process host adapter。sidecar IPC envelope / handshake 仍是目标契约，尚未实现。

**与协议内核的关系（2026-08-16 回写）：** Codex→Claude Responses 是 **③ 本机路由** 的 experimental bind 边（`codex-subscription-to-claude-responses-v1`，见 [product-decisions.md](product-decisions.md) 与 `domain/protocol_graph/adapter_capability_matrix.rs`）。①② 不依赖 sidecar。**纯协议内核**（Messages/IR/Responses、fixtures、`RetryGate`）已在 `agenthub-core::bridge::protocol`。该内核**不**等于 sidecar 控制面、不改变 `BridgeRuntimeHost` 宿主归属。控制面 envelope / handshake / mutation API 仍属 Phase 2 目标契约，实现前 GUI 不得假装 sidecar 已存在。

验收：现有 GUI 行为不变；所有 `local_bridge` mutation 经相同 application contract（当前为 in-process `DesktopAdapterControl`）；`config_sync`/`native_endpoint` 无 sidecar 依赖；request-id 幂等、revision/rule conflict、saga 补偿有测试；协议内核单测通过。Codex→Claude Responses 的产品门禁以协议图为准（experimental bind），不要把过时的 `canApply=false` 写回验收。

回滚：保留 Tauri in-process host adapter 和既有 command 映射，撤回尚未启用的 contract/DTO 调用；不迁移或重写用户 profile 数据。

### Phase 2：引入 sidecar，双通路可控切换（**未开工**）

工作（尚未开始）：实现 `DataStoreBootstrap`/migration lock 与 `open_checked`，再实现 `crates/agenthub-adapterd`、独立进程生命周期、单实例锁、local IPC、handshake、observed status、恢复 supervisor；`src-tauri` 实现 client 与 spawn/connect。通过显式 feature/发布开关把 `local_bridge` 的管理请求路由到 sidecar；所有 live 写仍由同一 core `ProviderService` 路径完成。当前工作区没有这些类型或 crate 的实现，只有本文描述。

验收：同 canonical data dir 只起一个 sidecar；GUI 退出/crash 后 listener 存活；sidecar cold restart 恢复 auto-start；受控更新恢复更新前完整 running set；不兼容协议/schema 拒绝 mutation；migration 并发/中断 fail-closed 且可恢复；stale lock 安全回收；sidecar unavailable 仅影响 `local_bridge`；不出现数据库直写或 live 文件直写。

回滚顺序固定为：关闭 sidecar admission → 让当前 sidecar owner 完成、补偿并按 journal reconcile 所有 in-flight saga → 持久化 handoff manifest（running set、profile revision、端口和恢复所需的非 secret 身份）并验证 Phase 1 binary/schema 兼容 → 对无法由 Phase 1 接管的 current generated Provider，先经 ProviderService 安全解除 loopback 投影 → 停止 sidecar并确认端口释放 → **先以 provisional 模式启动 Phase 1 host、恢复 handoff set 并逐项 health-check** → 全部必须接管的 listener 健康后才提交路由开关和消费 manifest。由于同一端口不能由两个 host 同时占有，handoff 存在一个受控短暂中断窗口；若 Phase 1 绑定或健康检查失败，立即停止 provisional host并用未消费的 manifest 重启兼容 sidecar、恢复并验证原 running set。若 sidecar 也无法恢复，则用 manifest/journal 触发 ProviderService 恢复已记录的 binding/live snapshot 或安全解除 Adapter-owned loopback 投影，标记 `needs_attention`，继续阻止模式切换。禁止先杀 sidecar、再依赖已不存在的 owner 做 reconcile，也禁止在 listener 未健康时永久切换开关。

### Phase 3：sidecar 成为唯一 `local_bridge` runtime owner

工作：移除 Tauri 对 `BridgeRuntimeHost` 的 runtime ownership，只保留 IPC client；新安装/升级默认按需启动 sidecar。完成 update-resume manifest、schema migration coordination、显式 shutdown、卸载准备、系统关机和故障诊断的产品流程。

验收：Tauri `AppState` 不再持有 runtime host；GUI/CLI 相同 IPC contract；恢复、更新前 running set、卸载前 active generated Provider 解除、remove/cascade、credential rotation、schema migration 与故障恢复端到端验证；兼容性和回滚版本受发布策略覆盖。

回滚：仅在发布包仍包含兼容 Phase 1 host、operation journal 已安全 reconcile、schema 仍落在旧版本 `max_readable_schema` 内的受控版本中允许。不可逆 schema 迁移须另有批准、DB 快照和恢复方案，不能借本 sidecar 迁移隐含执行。

## 14. 测试矩阵

| 层级 | 必测场景 |
|---|---|
| core unit | route/rule revalidate、request-id 重放、revision conflict、状态映射、secret redaction、saga phase 与反向补偿 |
| service/integration | ProviderService + LiveWriteAuthority 的生成 Provider 安全切换、connection 删除阻止/cascade、generated Provider 删除门禁、credential rotation reconcile、SQLite WAL 多进程交错、卸载前 revision/stable-identity 冲突不覆盖、live/binding 安全解除 |
| datastore bootstrap | GUI/CLI/sidecar 并发启动、shared/exclusive schema lease、已打开的旧 GUI/CLI 阻止 migration、等待与 reopen、迁移中崩溃/DB snapshot 恢复、新旧 binary/schema min/max 兼容门禁 |
| IPC contract | named pipe/Unix socket ACL、handshake 协议与 schema 兼容/不兼容、envelope schema、超时/断线、instance/epoch stale event、不得在 payload/日志出现 secret |
| sidecar lifecycle | canonical-data-dir 单实例、健康连接复用、stale lock 安全回收、并发 profile/同 target-agent 锁序、shutdown admission 与 drain |
| data plane | 各支持协议的文本、SSE、工具、用量、取消、上游失败；每 profile token；listener crash/restart；端口冲突与 IPv4/IPv6 loopback |
| recovery | apply/start/stop/remove 每个 journal phase 强制崩溃后恢复；更新中断、running-set manifest 重放、兼容/不兼容 schema 下 binary+snapshot 回滚、系统强杀、DB 有 durable profile 但无 listener、listener 存活但 live 不匹配 |
| GUI/CLI E2E | GUI 关闭/崩溃不影响 sidecar；显式停止适配并退出会 drain；`host_unavailable` 不 fallback；`config_sync`/`native_endpoint` 在 sidecar 不可用时仍可用 |
| install/update | sidecar binary 同包可发现、版本/schema 拒绝 mutation、更新 drain/manifest/restart/restore/回滚、卸载先解除 live loopback 再清理 IPC 且不擅自删除用户数据 |

## 15. 延后项

- Windows Service、LaunchAgent、systemd、开机常驻和跨用户控制。
- 远程控制、LAN 数据面、多机 profile 调度或服务端部署。
- sidecar 承接 `config_sync`、`native_endpoint`、非 bridge Adapter 或完整 Connection/Account/Provider 领域。
- 多 sidecar 分片、高可用/自动重启策略、独立资源限额；在单用户 single-instance 语义稳定后再评估。
- 凭据落盘加密：无必要，项目范围外。

本设计的成功标准不是“多了一个 daemon”，而是：只有 `local_bridge` 获得独立、可诊断、可恢复的 runtime；领域数据和 live 配置仍通过既有 core owner 安全管理；任何 GUI、CLI、更新或异常路径都不把持久化配置误报为真实 listener。
