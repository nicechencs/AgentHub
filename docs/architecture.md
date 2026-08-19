# AgentHub 目录结构与模块拆分

> **现行状态（dest `02358a3` / 0.2.2）**：Linux 一等公民；官方船经 `release` 三文件版本 bump 后发布。`agenthub-adapterd` sidecar 是目标、未迁。Chat 模型选择 / Projects `--resume` / MCP 注入 / 全站 i18n 未做。
> 对应《AgentHub 项目方案 v1.3》第 4 节的落地细化。  
> 目标 cargo workspace 为三 crate：`agenthub-core`（业务核心）/ `agenthub-gui`（Tauri 壳）/ `agenthub-cli`（命令行）；当前三者均已在仓库中（`src-tauri` = gui）。
> v1.1 同步：Adapter 接口加厚（skills/backup 路径）、Service 职责表、Usage/模型列表边界。  
> v1.2：CLI 命令与配置契约详见 [cli-and-config.md](cli-and-config.md)（本文 §5–§6 仅结构摘要）。  
> v1.3：`runtime/` + `env_service` —— 安装 Agent 前的共享运行时（Node/npm 等）检测与引导。  
> 2026-08-12 同步：Adapter 规则分析/稳定直连、Bridge core 与只读 MCP inventory 的当前工作区结构。
> 2026-08-12 决策同步：`local_bridge` 的目标宿主确定为用户级 `agenthub-adapterd` sidecar；当前实现仍由 Tauri `AppState` 进程内托管，迁移契约见 [adapter-sidecar-design.md](adapter-sidecar-design.md)。
> 日志：core 统一 tracing（文件 + 可选 stderr）→ [logging.md](logging.md)。  
> **前端 backend 分层（已落地）**：`lib/backend/{contracts,tauri,current}` + `dev/mocks` + `app/runtime`；命令与 adapter 选择见 **§4.1–§4.2**。
> 2026-08-14：Hub 重构 Phase 1 入口（ConnectFlow）已落地，详见 [hub-redesign-plan.md](hub-redesign-plan.md) / [ui-design.md](ui-design.md)。
> 2026-08-19：把已有登录接到另一个工具的领域对象仍是票 / 绑定 / 协议图（实现名），见 [connection-binding-model.md](connection-binding-model.md)。**产品方向**（① 直接改配置 / ② 写进对方认的登录 / ③ 本机转发）见 [product-decisions.md](product-decisions.md)。现行 UI 说「登录」不说「票/钱包」，芯片不再标圈号。当前实现：读模型 + 全局登录列表 + `plan_ticket` / `bind` / `unbind`；`canApply` 仍按「现在能不能写上去」打开。
> 2026-08-15：模块化审查结论与分阶段收口见 [modularity-improvement.md](modularity-improvement.md)。目录职责仍以本文为准；生产组合仍偏 Adapter-centric，改进按该文 P0/P1/P2，不另开微服务。
> 2026-08-16 文档回写：core 现树含 `integrations/` `platform/` `adapter_control/` `domain/`；生产写入走 Ticket；`adapter_control` 契约已落地但仍 in-process。

## 1. 顶层结构

```
AgentHub/
├── README.md                  # 入口：启动方式与结构摘要
├── AGENTS.md                  # 项目约定 + Agent 协作规则（真源）
├── agent.md                   # 兼容入口 → AGENTS.md
├── run.bat / run.ps1          # Windows 一键启动
├── run.sh                     # macOS / Linux 源码启动（`--check` 可只验依赖；正式安装包见 GitHub Releases）
├── Cargo.toml                 # workspace members = ["crates/agenthub-core", "crates/agenthub-cli", "src-tauri"]
├── package.json               # 前端根(pnpm)
├── pnpm-lock.yaml
├── vite.config.ts
├── tsconfig.json
├── tailwind.config.ts
├── index.html                 # Vite 入口
├── docs/                      # 方案与设计文档（索引见 docs/README.md）
├── scripts/                   # 运维脚本
├── src/                       # React 前端(见 §4)
├── src-tauri/                 # agenthub-gui,Tauri v2 壳(见 §3)；应用图标在 icons/
└── crates/
    ├── agenthub-core/         # 全部业务逻辑(见 §2)；presets 在 src/presets/
    └── agenthub-cli/          # CLI 二进制(见 §5)
```

静态资源约定（无顶层 `assets/`）：

- 桌面/安装图标：`src-tauri/icons/`（仅 Win/macOS/Linux；源图 `app-icon.svg`；生成请用 `pnpm icons`，会剔除移动端/商店产物）
- 前端供应商预设模板：`src/config/presets/`
- Core 侧预设：`crates/agenthub-core/src/presets/`

约定：`agenthub-core` **不依赖 tauri**；GUI 与 CLI 只是 core 之上的两种薄壳。所有路径解析、文件读写、SQLite 和脱敏逻辑都在 core 内，可单测。当前方案没有必要做凭据落盘加密。

当前实现状态、未实现清单和风险以 [agenthub-plan.md §8](agenthub-plan.md) 为唯一真源；本文只保留目录与模块职责说明。凭据落盘加密仍属于项目范围外。

## 2. `crates/agenthub-core` — 业务核心

```
crates/agenthub-core/
├── Cargo.toml                 # rusqlite(bundled)/serde/toml_edit/
│                              # tokio/axum/reqwest（OAuth + Bridge）/thiserror/dirs/tempfile/jsonc-parser
└── src/
    ├── lib.rs                 # 对外门面:pub use 各 service;AgentHub::open(data_dir) 统一构造
    ├── error.rs               # AppError(thiserror),含 i18n key + 中英文消息;Result<T> 别名
    ├── logging/               # 统一 tracing：按日文件 + 可选 stderr、targets、保留清理（见 logging.md）
    ├── catalog/               # 产品常量 façade（limits / market；install 转调 platform::install）
    ├── presets/
    │
    ├── models/                # 纯数据结构,serde 序列化；规划矩阵不在此树
    │   ├── agent.rs           # AgentId 枚举(当前八家，含 `dsh`)、DetectResult、AgentStatus
    │   ├── capability.rs      # Capability / CapabilityLevel / CapabilityState（产品能力枚举）
    │   ├── provider.rs / account.rs / skill.rs / usage.rs / backup.rs
    │   ├── chat.rs / run.rs / project.rs / install.rs / runtime.rs / settings.rs
    │   ├── adapter.rs         # 路由分析、apply plan、profile 与状态
    │   ├── ticket.rs          # Ticket / TicketBinding / TicketWallet
    │   ├── connection_trash.rs / agent_visibility.rs / update.rs
    │   └── adapter_state_model.rs / adapter_model_mapping.rs
    │
    ├── domain/                # 无 I/O 的规划图
    │   └── protocol_graph/    # 规划矩阵真源（不再在 models/ 下）
    │       ├── adapter_capability_matrix.rs
    │       └── agent_capability.rs
    │
    ├── storage/               # SQLite 层:agenthub.db,WAL,schema 版本迁移
    │   ├── mod.rs             # Database
    │   ├── migrations/
    │   ├── account_repo.rs / provider_repo.rs / backup_repo.rs / chat_repo.rs
    │   ├── adapter_profile_repo.rs / skill_repo.rs / operation_repo.rs
    │   ├── usage_repo.rs      # 用量入库（不是 usage_dao）
    │   ├── binding_repo.rs    # ActiveBinding 行
    │   └── connection_trash_repo.rs
    │
    ├── adapters/              # 生产 AgentAdapter 仍在此：trait / registry / 各家 impl
    │   ├── adapter_trait.rs / registry.rs / detect_binary.rs
    │   ├── claude.rs / codex.rs / kimi.rs / grok.rs / pi.rs
    │   ├── workbuddy.rs / cursor.rs / dsh.rs
    │   └── config_write.rs / auth_revision.rs / pi_auth.rs
    │                          # 兼容期未整段迁走；integrations/*/adapter_facade 才是空转 façade
    │
    ├── integrations/          # 稀疏端口贡献（并非每家都有全套文件）
    │   ├── agents/
    │   │   ├── {claude,codex,kimi,grok,pi,workbuddy,cursor,dsh}/
    │   │   │   # 按需：paths / install / config / usage / stream / project
    │   │   └── demo_agent/    # 仅 test-only，不进 register_production
    │   └── shared/
    │
    ├── platform/              # 平台能力（与具体 Agent 解耦的 registry / service）
    │   ├── agent_catalog/     # AgentCatalogService / AgentKey / 目录
    │   ├── config/            # ConfigurationService：schema / read / apply
    │   ├── detection/ / install/ / lifecycle/
    │   ├── paths/ / projects/ / skills/ / stream/ / usage/
    │
    ├── adapter_control/       # Tauri-neutral 控制契约（仍 in-process）
    │   ├── contract.rs        # AdapterControl / BindAction / UnbindAction
    │   ├── coordinator.rs     # AdapterSagaCoordinator
    │   └── status.rs
    │                          # 当前由 src-tauri adapter_control_host.rs
    │                          # 的 DesktopAdapterControl 实现；agenthub-adapterd 仍不存在
    │
    ├── bridge/                # loopback host + Responses/Chat 协议转换
    │   ├── host/              # 已拆：mod / lifecycle / http / dispatch
    │   ├── runtime.rs / types.rs / session.rs
    │   └── protocol/          # chat / responses / anthropic_messages
    │
    ├── runtime/               # 共享运行时(与具体 Agent 解耦;安装 Agent 的前置环境)
    │   ├── mod.rs / detect.rs / nodejs.rs / bootstrap.rs
    │
    ├── services/              # 业务层:组合 adapter + repo,GUI/CLI 只调这一层
    │   ├── env_service.rs / agent_service.rs / install_service.rs
    │   ├── provider_service.rs / account_service.rs
    │   ├── adapter_route_service/ / adapter_apply_service/
    │   ├── adapter_secret_resolver.rs / adapter_bridge_service.rs
    │   ├── ticket_read_service.rs / ticket_bind_service.rs
    │   ├── connection_service.rs / live_write_authority.rs
    │   ├── agent_visibility_service.rs / update_check_service.rs
    │   ├── skill_service.rs / skill_market.rs
    │   ├── skillssh_market.rs / skillhub_market.rs
    │   ├── switch_undo/ / account_quota.rs
    │   ├── mcp_inventory.rs / project_service.rs
    │   ├── chat_service.rs / run_service.rs
    │   ├── usage_service.rs / backup_service.rs / settings_service.rs
    │
    ├── usage/                 # 会话日志用量解析（零代理、只读）
    │   ├── session_jsonl.rs / pricing.rs / grok.rs
    │   ├── embedded-pricing.json / embedded-pricing.meta.json
    │
    ├── oauth/                 # PKCE + device / identity / catalog / pi_refresh
    │   ├── pkce.rs / providers.rs / server.rs / session.rs
    │   ├── device.rs / identity.rs / catalog.rs / pi_refresh.rs
    │                          # 勿在公开文档抄写端点/client
    │
    └── utils/
        ├── paths.rs / atomic.rs / process.rs / command_exec.rs
        ├── agent_lock.rs / redact.rs / expiry.rs / grok_toml.rs
        └── stream_parse/      # Chat 结构化流解析（claude/codex/kimi/grok/pi）
```

**关键依赖对照**：`rusqlite(bundled)`、`toml_edit`、`serde_json(preserve_order)`、`dirs`、`tempfile`、`jsonc-parser`、`thiserror`、`tokio`、`axum`、`reqwest`（OAuth / Bridge）、`tracing` / `tracing-subscriber` / `tracing-appender`（日志）。凭据加密依赖不属于当前版本。日志模块与契约见 [logging.md](logging.md)。

### 2.1 AgentAdapter 接口约定

```rust
trait AgentAdapter {
    fn id(&self) -> AgentId;
    fn detect(&self) -> DetectResult;
    fn install_channels(&self) -> Vec<InstallChannel>; // 默认 catalog 贡献
    fn read_config(&self) -> Result<AgentConfig>;
    fn write_config(&self, config: &AgentConfig) -> Result<()>; // 不是 apply_provider
    fn read_auth(&self) -> Result<AuthState>;
    fn read_account(&self) -> Result<LiveAccount>;
    fn apply_account(&self, account: &LiveAccount) -> Result<()>;
    fn build_api_key_account(&self, api_key: &str) -> Result<LiveAccount>;
    fn authorization_key(&self, kind: AccountKind, credentials: &Value) -> Option<String>;
    fn identity_label(&self, kind: AccountKind, credentials: &Value, label_hint: Option<&str>) -> Option<String>;
    fn skills_dir(&self) -> Option<PathBuf>;
    fn live_backup_paths(&self) -> Vec<PathBuf>;
    fn build_run_spec(&self, binary: &Path, prompt: &str, opts: &RunOptions) -> Result<RunSpec>;
    fn capability(&self, cap: Capability) -> CapabilityState;
}

// 没有 supports_skills / usage_source。
// skills 在 platform/skills，usage 在 platform/usage。

// 共享运行时 — 不在 Adapter 内实现检测
enum RuntimeId { NodeJs, Npm, PowerShell, Git }
struct InstallChannel {
    id: &'static str,           // "npm" | "native" | ...
    label: &'static str,
    requires: &'static [RuntimeId],
}
// host_runtimes(): Windows → ALL；macOS/Linux → NodeJs/Npm/Git（不含 PowerShell）
// native_install_requires(): Windows → [PowerShell]；其它 → []
```

| 方法 | 含义 |
|---|---|
| `detect` / `read_*` / `write_config` / `apply_account` | 该 Agent 安装态与 live 配置/凭据的读写真相 |
| `install_channels` | 可选安装渠道及**平台相关前置 Runtime**；生产默认走 catalog；native 在 Unix 上不得 require PowerShell |
| `read_account` / `build_api_key_account` | 读 live 快照、构造 API Key 票（不写盘） |
| `authorization_key` / `identity_label` | 账号池去重指纹与展示标签（身份 ≠ 授权） |
| `skills_dir` | 技能投影目标目录；无目标则 None。投影编排在 `platform/skills` |
| `live_backup_paths` | 写前快照文件清单，供 `backup_service` 统一拷贝 |
| `build_run_spec` | headless 运行命令 |
| `capability` | 该 Agent 对 `Capability` 的声明（穷尽 match） |

**演进预留**（非 MVP 必做）：当投影方式不再只是「复制到 dir」时，再增加 `install_skill` / `remove_skill` / `list_installed_skills`，把落盘差异继续留在 Adapter，**禁止**在 `skill_service` 写 `match agent_id`。

### 2.2 Service 职责一览

| Service | 做什么 | 不做什么 |
|---|---|---|
| `provider_service` | CRUD、切换编排（backfill→backup→写→锁） | 不解析 jsonl；不知 skills 真源 |
| `account_service` | 账号池、OAuth/导入、切换编排 | 不实现各家 OAuth 端点细节（oauth/ 模块） |
| `adapter_route_service` / `adapter_apply_service` | 只读分析、预览；应用后端显式 `can_apply` 边（现有多条，不再是「仅 Kimi→Claude」） | 不推断未知凭据，不把 preview 自动升级为可写 |
| `adapter_bridge_service` | 准备/恢复 Bridge profile 与 runtime material，记录 finalize/needs_attention | 不持有 listener，不直接写 live 配置；当前由 Tauri controller、目标由 sidecar application service 编排 host 与 ProviderService |
| `ticket_read_service` | 钱包只读聚合 + `plan_ticket` | 不写绑定、不启动 listener |
| `ticket_bind_service` | `bind` / `unbind` 写口 | LocalBridge bind 拒绝，必须走 desktop host saga |
| `connection_service` | 当前指针 `ActiveBinding`（account/provider `is_current`） | 不是产品 `TicketBinding`；不写 live 文件 |
| `live_write_authority` | 跨进程 live 写锁 | 不解释配置语义 |
| `agent_visibility_service` | dest store-stamp 隐藏：`{data_dir}/agent_visibility.json`（不是 release #22 软隐藏）。detect / install / 凭据 / 备份不变；Usage collect 与 parser_health 跳过 hidden | 不改 detect / install / 凭据 |
| `ConfigurationService`（platform） | 通用配置 schema / read / validate / apply | 不持有账号池 |
| `AgentCatalogService`（platform） | Agent 目录（key / 能力 / 安装渠道） | 不执行安装 |
| `skillhub_market` / `skillssh_market` | skillhub.cn / skills.sh 远程搜索与安装 | 不改投影矩阵真源 |
| `update_check_service` | 探测远程 latest（npm dist-tags / 官方 feed） | 不自动升级 |
| `switch_undo` | Provider / Account 切换撤销 | 不改 live 语义，只回滚最近一次切换 |
| `account_quota` | OAuth 账号用量窗（写入 `account.extra`） | 失败软降级，不阻断 list/refresh |
| `mcp_inventory` | 只读扫描已知 MCP 配置文件并归一化 server 条目 | 不创建、编辑、删除或注入 MCP server |
| `skill_service` | 真源扫描、投影矩阵、sync/enable/disable、install/uninstall/update/project、import_private | 不扫描会话日志；远程市场由 `skill_market`/`skillssh_market` 提供；插件体系仅只读协作 |
| `usage_service` | collect、入库、summary/trend、**list_models（用量去重）** | 不提供官方模型商店；不算 live 配置默认 model 源 |
| `backup_service` | live/db 快照、恢复（恢复前再备）、索引 | 不解释 TOML/JSON 语义（只拷文件） |
| `env_service` | Runtime detect/ensure/引导安装计划；doctor 的 runtimes 段（**仅 host_runtimes**） | 不装具体 Agent；不写 L2 live；非 Windows 不探测 PowerShell |
| `agent_service` / install 管线 | detect；**install/upgrade = ensure_env → 平台渠道**（Windows ps1 / Unix sh / npm） | 不直接改 providers 表；不在各 Adapter 内复制 `which node` |
| `run_service` | 多 Agent headless 执行（`run` / `run_each`）；流式 stdout 行推送 | 不维护多轮会话；不拼聊天上下文 |
| `chat_service` | 会话 CRUD；按 Agent **隔离**拼接历史；调用 `run_each`；取消令牌 | 不使用各 CLI 原生 `--resume`；core 内无 Tauri 类型 |

### 2.3 调用关系（示意）

```
切换 Provider:
  provider_service.switch
    → account/provider backfill
    → backup_service.snapshot(agent)      // adapter.live_backup_paths()
    → adapter.write_config(...)           // ProviderService 编排原子写
    → emit provider-switched

同步 Skill:
  skill_service.toggle(skill, agent)
    → adapter.skills_dir()
    → 复制/移除 + 更新 lock/矩阵
    → （可选）backup 将被覆盖的目标文件

安装 Agent（两阶段）:
  agent_service.install(agent, channel, opts)
    → channel = adapter.install_channels() 选中项
    → env_service.ensure(channel.requires)
         · missing 且 !opts.install_deps → Err(EnvNotReady { remediations })
         · missing 且 opts.install_deps  → runtime/bootstrap 流式执行 → 再 detect
    → process 跑官方 install 命令（白名单 + CREATE_NO_WINDOW）
    → adapter.detect() 刷新；runtime 缓存 invalidate

应用稳定 Adapter 规则:
  adapter_route_service.analyze / plan
    → 后端显式 can_apply 门禁（多条已登记边）
    → adapter_apply_service.apply
    → 创建受管 profile / Provider
    → provider_service.switch（复用备份与原子写）

产品绑定:
  bind_ticket
    → AdapterControl（in-process DesktopAdapterControl）
    → reshape：TicketBindService
    → local_bridge：desktop host saga（TicketBindService 拒绝 LocalBridge）

应用 local_bridge（当前 / 目标）:
  当前：Tauri adapter_bridge_controller → BridgeRuntimeHost + core services
  目标：Tauri/CLI control client → local IPC → agenthub-adapterd
      → AdapterRuntimeApplication（local_bridge 完整 saga 的唯一进程 owner）
      → BridgeRuntimeHost + AdapterBridgeService + ProviderService
  // Connections/ProviderService 仍是领域 owner；sidecar 不直接写表或 live 文件
  // agenthub-adapterd 仍不存在

MCP 清单:
  mcp_inventory.list
    → 只读解析各 Agent 已知配置文件
    → Tauri command → MCP 页面

采集 Usage:
  usage_service.collect
    → platform/usage 源 parse_incremental(...)
    → usage_repo 增量插入
    → pricing 填 cost
    → emit usage-updated

list_models（Usage 筛选）:
  usage_service.list_models
    → DISTINCT model FROM usage_records
    // 不是 Adapter 能力，也不是官方 API

Chat 发送（GUI）:
  commands/chat::chat_send(Channel<ChatEvent>)
    → spawn_blocking
    → chat_service.send
         · build_agent_prompt（仅 user + 该 agent 的 ok 回复）
         · run_service.run_each（per-agent prompt + StreamingProcessRunner）
         · on_event → Channel.send（行级 stdout/stderr / 完成态）
    → SQLite conversations + chat_messages（migration 0002_chat）
    // 过程 UI（命令/状态/stderr/后续步骤）见 docs/chat-process-streaming.md
```

## 3. `src-tauri` — agenthub-gui（Tauri v2 壳）

```
src-tauri/
├── Cargo.toml                 # 依赖 agenthub-core;tauri 2.x;
│                              # 插件：autostart / dialog / single-instance；
│                              # updater 条件编译（macos/windows/linux）。
│                              # 没有 window-state / opener / process
├── tauri.conf.json
├── build.rs
├── icons/
└── src/
    ├── lib.rs                 # 启动:初始化 AgentHub(core 门面)→ .manage(AppState);
    │                          # tauri::generate_handler! 注册 commands;托盘
    ├── state.rs               # AppState 持有 hub + BridgeRuntimeHost
    │                          # + AdapterBridgeSagaCoordinator + ExitCoordinator
    ├── adapter_bridge_controller.rs
    ├── adapter_control_host.rs  # DesktopAdapterControl（in-process AdapterControl）
    ├── exit_coordinator.rs
    ├── tray.rs
    ├── skill_watch.rs
    ├── window_policy.rs
    └── commands/              # 薄层:参数校验 + 调 core service + 序列化;每模块一文件
        ├── account.rs / adapter.rs / agent_catalog.rs / agent_visibility.rs
        ├── backup.rs / chat.rs / configuration.rs / doctor.rs
        ├── install.rs / lifecycle.rs / mcp.rs / oauth.rs
        ├── project.rs / provider.rs / settings.rs / skill.rs
        ├── trash.rs / usage.rs
```

事件约定（**目标**）：切换/采集完成后可 `app.emit("provider-switched" | "account-switched" | "usage-updated", payload)`。**当前实现**以前端主动 refetch 为主，尚未统一 emit 事件桥。Chat 流式走 Tauri v2 `ipc::Channel<ChatEvent>`（非 SSE），阻塞 IO 经 `spawn_blocking` / hub blocking 封装。

### 3.1 Adapter Runtime 进程边界（目标）

当前工作区中，`src-tauri::AppState` 同时持有 `BridgeRuntimeHost`、process-local saga coordinator 与退出 barrier。这是可工作的进程内实现，不是最终部署边界。Phase 1 控制契约已在 core `adapter_control` + in-process host；sidecar 二进制 / IPC / schema lease 未开工。

目标结构：

```text
React Routes UI（本机路由页）
  → lib/backend/tauri/adapter.ts
  → Tauri adapter command（薄映射）
  → AdapterControlClient
  → 用户级本地 IPC
  → agenthub-adapterd（每个 canonical data dir 单实例）
       ├─ AdapterRuntimeApplication   # local_bridge 完整 saga
       ├─ BridgeRuntimeHost           # listener / drain / observed status
       └─ agenthub-core services      # profile、Connection 引用、Provider 安全切换
```

进程边界不改变领域边界：

- Connections 不拆进程；Account、Provider、ActiveBinding 仍由 `AccountService`、`ProviderService`、`ConnectionService` 管理。
- sidecar 是 `local_bridge` runtime 与其 lifecycle mutation 的唯一进程 owner，但数据库/live 配置写入仍必须通过上述 core service 和跨进程 `LiveWriteAuthority`。
- `native_endpoint` / `config_sync` 不依赖 sidecar。sidecar 不可用时它们仍应正常工作。
- GUI 不直接持有第二个 host，也不跨 IPC 执行 saga 的后半段；否则会形成双主和跨进程半事务。
- SQLite WAL 是共享持久化真源，但 `running` 是 sidecar 内存中的 observed truth。控制面不可达时由页面派生 `host_unavailable`，不得信任上次运行记录。
- 当前与目标态、IPC、版本/schema 握手、SQLite shared/exclusive schema lease 与 migration 权威、锁序、更新/卸载 saga 及三阶段迁移详见 [Adapter Sidecar 目标架构与迁移方案](adapter-sidecar-design.md)。

## 4. `src` — React 前端

技术：React 18 + TypeScript + Vite + Tailwind + shadcn/Radix（只此一套 UI）+ CodeMirror 6 + recharts + react-router。页面内本地 state + `lib/api` 拉取（**未**引入 TanStack Query / react-hook-form / zod / i18next）。GUI 语言为轻量自研字典 + `LanguageProvider`（`src/lib/i18n/`），见 `ui-design.md`。详细设计见 `ui-design.md`。

### 4.1 目录结构（已落地）

> 已把 **invoke 边界**、**contracts**、**mock**、**测试** 与 **页面** 拆开；页面可继续经 `lib/api` 兼容 façade 渐进迁移，无需一次全改。

```
src/
├── app/
│   └── runtime/                 # 应用组合入口 + catalog / agent-status /
│                                # connection-pool / bridge-presence / app-update stores
├── lib/
│   ├── backend/
│   │   ├── contracts/           # DTO、接口、纯映射（不碰 Tauri）
│   │   ├── tauri/               # 唯一允许调用 invoke 的地方
│   │   └── current.ts           # 默认生产实现切换点
│   ├── api/                     # 兼容 façade，页面无需一次全改
│   ├── i18n/                    # 轻量字典 + t()；zh/en；不引入 i18next
│   ├── connect-flow/            # 统一连接流程逻辑层
│   ├── hooks/                   # useSkills / useInstalledAgents
│   ├── ticket-wallet.ts
│   └── bridges-path.ts
├── dev/
│   └── mocks/
│       ├── backend.ts / account.ts / chat.ts / adapter.ts / ticket.ts
│       ├── catalog.ts / config.ts / trash.ts / update.ts
│       ├── OAuthFlowDialog.tsx
│       └── fixtures/
├── test/
│   └── setup.ts                 # 无 factories/
├── pages/                       # 页面层；不直接 invoke
├── main.tsx / App.tsx           # 入口;侧边导航 + 路由(react-router)
├── config/
│   ├── agents.ts                # agent 元数据:名称、品牌色、logo、能力位
│   └── presets/                 # 各 agent 供应商预设模板
├── components/
│   ├── ui/                      # shadcn 生成组件
│   ├── layout/                  # Sidebar / TopBar / PageHeader / AgentTabStrip
│   ├── connect/                 # ConnectFlowDialog（统一连接流程 UI + 状态机）
│   └── shared/                  # SecretInput / EmptyState / ...
└── styles/
```

| 位置 | 职责 |
|---|---|
| `app/runtime/` | 应用组合 / 依赖装配入口；按命令与环境选择 backend adapter |
| `lib/backend/contracts/` | DTO、接口、纯映射；**不**调用 `invoke` |
| `lib/backend/tauri/` | **唯一**允许调用 Tauri `invoke` 的边界 |
| `lib/backend/current.ts` | 默认生产实现（指向 Tauri adapter） |
| `dev/mocks/` | 浏览器开发态 mock（backend / 领域 / fixtures） |
| `test/` | vitest `setup.ts`（约定见 [testing.md](testing.md)；无 factories/） |
| `lib/api/` | 兼容 façade：现有页面可继续 import，内部逐步委托到 `lib/backend` |
| `lib/connect-flow/` | 统一连接流程逻辑层（契约/可行性/fan-out/用途反查） |
| `components/connect/` | ConnectFlowDialog（统一连接流程 UI + 状态机） |
| `pages/` | 页面与 UI 状态；不直接碰 `invoke` |

### 4.2 命令与 Backend Adapter 选择

| 命令 | 选用的 adapter | 说明 |
|---|---|---|
| `pnpm dev` / `tauri dev`（及 `pnpm tauri:dev`） | **Tauri adapter** | 桌面壳 + 真实 invoke |
| `pnpm dev:mock` | **browser mock adapter** | 纯浏览器 Vite，走 `dev/mocks` |
| `pnpm build` | **强制 Tauri adapter** | 生产构建不得打进 mock 实现 |
| `pnpm test` / vitest | **mock adapter** | `vitest.config.ts` 的 `#backend` alias |

**实现机制（Vite alias，非 per-function `isTauriApp` fallback）**：

1. `vite.config.ts` 将 `#backend` 解析到  
   - `src/lib/backend/tauri/create-backend.ts`（`serve` 且非 mock / **任意 build**）  
   - `src/dev/mocks/create-backend.ts`（`vite --mode mock`）
2. `src/lib/backend/current.ts` → `export { createBackend } from '#backend'`
3. `src/app/runtime` 持有单例 `getBackend()`；`src/lib/api/*.ts` 仅为 façade 委托。
4. **生产 module graph 护栏**：`vite build` 的 `generateBundle` 扫描 chunk.modules，若路径匹配 `src/dev`、`src/test`、`*.test.*`、`*.spec.*` 则 **立即失败**（不是对 dist 字符串 grep）。

**生产边界（硬约束）**：

- 非 Tauri 运行时调用 Tauri port：`assertTauriRuntime` → 明确 **unavailable**，禁止静默 mock。
- Usage：生产已接线 `UsageService`（session JSONL 增量采集 + 文件游标）；解析策略全面借鉴 **ccusage**；成本优先日志 `costUSD`，否则查内嵌 `embedded-pricing.json`（**离线快照**，LiteLLM 子集 + 日期别名 + `scripts/pricing/overrides.json`）；估算结果与价表同单位（**USD / 1M tokens**，字段 `costUsd`，**不做汇率换算**）；价表日更靠 `pnpm pricing:update` / `.github/workflows/update-embedded-pricing.yml` 开 PR，**运行时不拉价**；`doctor` 附带 `usageHealth` 分区（CLI ④）；`getAvailability()` → `available`；演示曲线仍仅 `dev:mock`。GUI：`UsageSyncProvider` 按 `usageCollectIntervalMin` 在**前台**定时 `collect`（`0` 仅手动），Dashboard 展示上次/下次同步；后台守护与文件监听见 [ui-design.md §4.6](ui-design.md)。
- OAuth：Claude / Codex / Grok 已有 loopback PKCE（`oauth_start` / `wait` / `complete`）；另有 device flow 命令（`oauth_device_start` / `oauth_device_poll` / `oauth_device_complete`）。未配置的 Agent 对话框展示 unavailable。演示 OAuth 仍走 `#oauth-flow-dialog` mock。不要抄 OAuth 端点。
- Dashboard alerts：生产从 doctor 派生 auth/env/update 告警（本地 dismiss）；mock 另有演示样例。
- 可选产品能力由 `Backend.features` 声明；生产：`providerUndoSwitch` / `providerTestLatency` / `accountUndoSwitch` 为 true，`backupExport` 仍为 false。UI 必须按 features 隐藏入口，不得依赖用户点了再失败。
- 页面与 `lib/api` façade **不得**直接 `import` `@/dev/*`；测试可直接实例化 mock backend。
- 页面 **不得**用 `isTauriApp()` 在真实/ mock transport 之间分支；能力由 `Backend.features` / typed result / unsupported 表达。

### 4.3 UI 本地偏好 vs mock

| 类别 | 位置 | 说明 |
|---|---|---|
| UI 偏好 | `src/lib/ui-preferences.ts`（`storage.ts` re-export） | theme / onboarding / sidebar 等 **合法 localStorage**，不是 mock |
| 业务 mock | `src/dev/mocks/**` | 仅 dev:mock / vitest |
| 能力模型 | `src/lib/capability.ts` | 仅类型 + `isCapabilityUsable/Blocked` |
| 演示能力矩阵 | `src/dev/mocks/capabilities.ts` | 原 `MOCK_CAPABILITIES` |

### 4.4 旧 `lib/api` 迁移对照

| 原文件 | 生产 | mock | façade |
|---|---|---|---|
| `account.ts` | `lib/backend/tauri/account.ts` | `dev/mocks/account.ts` | `lib/api/account.ts` |
| `adapter.ts` | `lib/backend/tauri/adapter.ts` | `dev/mocks/adapter.ts` | `lib/api/adapter.ts`（`apply` 已 deprecated，页面不得再调用；产品写入走 TicketPort） |
| `tickets` | `lib/backend/tauri/ticket.ts` | `dev/mocks/ticket.ts` | `lib/api/tickets.ts`（plan/bind/unbind） |
| `catalog` | `lib/backend/tauri/catalog.ts` | `dev/mocks/catalog.ts` | 经 `app/runtime` catalog store；无独立 `lib/api/catalog.ts` |
| `config` | `lib/backend/tauri/config.ts` | `dev/mocks/config.ts` | `lib/api/config.ts` |
| `trash` | `lib/backend/tauri/trash.ts` | `dev/mocks/trash.ts` | `lib/api/trash.ts` |
| `update` | `lib/backend/tauri/update.ts` | `dev/mocks/update.ts` | `lib/api/update.ts` |
| `agent.ts` | `lib/backend/tauri/agent.ts` | `dev/mocks/agent.ts` | `lib/api/agent.ts` |
| `backup.ts` | `lib/backend/tauri/backup.ts` | `dev/mocks/backup.ts` | `lib/api/backup.ts` |
| `chat.ts` | `lib/backend/tauri/chat.ts` | `dev/mocks/chat.ts` | `lib/api/chat.ts` |
| `dashboard.ts` | `lib/backend/tauri/dashboard.ts`（doctor 派生告警） | `dev/mocks/dashboard.ts` | `lib/api/dashboard.ts` |
| `doctor.ts` | `lib/backend/tauri/doctor.ts` | `dev/mocks/doctor.ts` | `lib/api/doctor.ts` |
| `doctor-map.ts` | 保留纯映射（无 MOCK fill） | — | 同左 |
| `env.ts` | `lib/backend/tauri/env.ts` + `lib/env-plan.ts` | `dev/mocks/env.ts` | `lib/api/env.ts` |
| `install.ts` | `lib/backend/tauri/install.ts` | `dev/mocks/install.ts` | `lib/api/install.ts` |
| `mcp.ts` | `lib/backend/tauri/mcp.ts` | `dev/mocks/mcp.ts` | `lib/api/mcp.ts` |
| `project.ts` | `lib/backend/tauri/project.ts` | `dev/mocks/project.ts` + fixtures | `lib/api/project.ts` |
| `provider.ts` | `lib/backend/tauri/provider.ts` | `dev/mocks/provider.ts` | `lib/api/provider.ts` |
| `settings.ts` | `lib/backend/tauri/settings.ts` | `dev/mocks/settings.ts` | `lib/api/settings.ts` |
| `skill.ts` | `lib/backend/tauri/skill.ts` | `dev/mocks/skill.ts` | `lib/api/skill.ts` |
| `usage.ts` | `lib/backend/tauri/usage.ts`（UsageService 真实接线） | `dev/mocks/usage.ts` | `lib/api/usage.ts` |
| `agent-connection.ts` | 纯聚合，保留生产 | — | 同左 |

DTO / mapper：`lib/backend/contracts/*-map.ts`。错误类型：`contracts/errors.ts`、`contracts/agent-errors.ts`。

### 4.5 Mock 审计表（生产依赖图清理）

| 原位置 | 分类 | 迁移 / 处理 |
|---|---|---|
| `lib/api/*` 内 `isTauriApp` + mock 分支 | 混合生产/mock | 拆到 tauri / dev/mocks；façade 仅委托 |
| `chat.ts` / `project.ts` `__reset*MockForTests` | 测试 hook 泄漏生产 API | 删除 façade 导出；改为 `dev/mocks` 的 `reset*` |
| `usage.ts` 演示数据 | 开发演示 | `dev/mocks/usage.ts`；**生产**已接线 `UsageService`（`getAvailability` → available） |
| `dashboard.ts` 模拟告警 | 开发演示 | `dev/mocks/dashboard.ts`；生产由 doctor 派生 |
| `skill.ts` `C:\mock\...` 路径 | 开发演示 | `dev/mocks/skill.ts` |
| `backup.ts` seed 备份 | 开发演示 | `dev/mocks/backup.ts` |
| `capability.ts` `MOCK_CAPABILITIES` | 浏览器演示矩阵 | `dev/mocks/capabilities.ts`；生产只留模型/判断 |
| `doctor-map` 静默 MOCK fill | 生产误用 mock | 删除；matrix 缺失 → `undefined` |
| `storage.ts` theme 等 | 合法 UI 本地状态 | 重命名语义为 `ui-preferences`（兼容 re-export） |
| `env` `simulateBrokenPath` / `resetRuntimesDemo` | 演示工具 | 仅 `dev/mocks/env.ts` 导出，不在 EnvPort |
| `fakeInstallScript` / `fakeEnv*Script` | 演示脚本 | 已移出生产 port；UI 统一 `*Detailed` + feature-local `install-preview` |
| `resetForTests` on Chat/Project ports | 测试 hook | 仅 `dev/mocks` 的 `resetChatMock` / `resetProjectMock` |
| 测速 / 切换撤销 / 导出备份 | 测速与切换撤销已实现；导出仍关 | 生产 `Backend.features`：undo/latency true，`backupExport` false。mock：测速/撤销可演示，导出仍关闭。**OAuth / token refresh 已接线**（Claude/Codex/Grok） |
| 注释中的 mock 说明 | 文档 | 更新为「dev:mock / tauri 边界」 |

### 4.6 页面与其它约定

Connections 收拢凭据生命周期。目标领域是 **票（Ticket）+ 绑定（Binding）+ 协议图**，不是「account/provider 出身 × 商品白名单」；完整模型与可重做的 UI 见 [connection-binding-model.md](connection-binding-model.md)。

当前实现：读模型 + 全局登录列表 + `plan_ticket` / `bind` / `unbind`。日常入口仍是 Dashboard「连接/切换」与 Connections「接到…」，走 `ConnectFlowDialog`（`plan.canApply` 表示**现在能写入**）。预览按三种做法说明（模型 ①②③）；界面芯片是「直连 / 用这份登录 / 本机路由 / 当前不支持」，见 [product-decisions.md](product-decisions.md)。生成投影不进登录列表。`/routes` 只管理 ③ 的本机转发运行时（旧 `/adapter`、`/router`、`/bridges` 永久跳过来）。各家接口与现在能不能写上去仍以 [provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md) 为规则真源。MCP 当前只读展示 inventory。页面仍可 import `@/lib/api/*`（渐进迁移）。`isTauriApp()` **仅**供 `lib/backend/tauri/invoke.ts` fail-closed 使用，页面不得据此选择 mock。

**未迁移 / 有意保留**：

- 页面目录结构（不做大爆炸重构）
- Rust command 名与 IPC 参数未改
- 凭据落盘加密：项目范围外
## 5. `crates/agenthub-cli` — 命令行

**完整命令树、全局 flags、退出码、GUI↔CLI 矩阵、验收清单 → [cli-and-config.md](cli-and-config.md)。**

```
crates/agenthub-cli/
├── Cargo.toml                 # clap(derive)、agenthub-core、comfy-table、dialoguer
└── src/
    ├── main.rs                # 全局 --data-dir/--agent/-o/--yes；构造 AgentHub
    └── commands/
        ├── doctor.rs          # 排障：runtimes + detect + paths + usage health
        ├── run.rs             # 多 Agent headless 并行/串行
        ├── env.rs             # list / install <runtime>
        ├── agent.rs           # list / capabilities / install(--install-deps) / upgrade / uninstall
        ├── provider.rs        # list/show/switch/import-live/presets
        ├── account.rs         # list/switch/import/add-apikey/delete/oauth-url/refresh
        ├── skill.rs           # list/list-installed/sync/enable/disable/install/uninstall/update/project/market/import-private
        ├── usage.rs           # collect/stats/models/health
        ├── backup.rs          # list/create/restore/delete
        └── config.rs          # path/get/set（L1 settings 白名单）
```

CLI 与 GUI 共用数据目录与 per-agent 写锁（core 内文件锁，跨进程）。危险操作默认确认；非 TTY 无 `-y` 不写盘（退出码 4）。

## 6. 数据目录与配置分层（运行时）

**分层契约与 `agenthub.toml` 允许键 → [cli-and-config.md §7](cli-and-config.md)。摘要：**

| 层 | 是什么 | 典型位置 |
|---|---|---|
| L0 启动 | `AGENTHUB_HOME` / 可选 `agenthub.toml`（仅 data_dir、log_level） | 默认 `~/.agenthub` |
| L1 业务真源 | SQLite + 备份索引 | `{data_dir}/agenthub.db` |
| L2 live | 各 Agent 官方配置/凭据 | 由各 Adapter 声明（路径不在文档展开） |
| L3 内置 | presets / pricing / 元数据 | core + assets（只读） |

```
{data_dir}/                    # 默认 ~/.agenthub ；可用 AGENTHUB_HOME 覆盖
├── agenthub.toml              # 可选 L0（禁止存密钥/provider 列表）
├── agenthub.db                # L1 SQLite(WAL)
├── backups/
│   ├── db/
│   └── live/<agent>/<ts>/     # L2 写前快照
├── exports/                   # 导出能力预留，按现有存储方案处理
└── logs/                      # 应用日志（CLI/GUI 共用）
    └── agenthub.YYYY-MM-DD    # tracing-appender 按日滚动；log_retention_days 启动清理
```

`logs/`：core `logging` 写入；默认级别 `info`，保留默认 14 天；完整规范 → [logging.md](logging.md)。

`backups` 表与前端 `BackupMeta` 对齐：`id, agent_id, kind(auto-switch|manual|pre-uninstall), path, files[], size, created_at, note`。

## 7. 拆分原则（为什么这样切）

1. **core 无 Tauri 依赖** —— 双端共享 + 可单测，这是「GUI + CLI」形态的地基。
2. **adapter 与 service 分离** —— adapter 只管「某 agent 的文件在哪、怎么读怎么写、备份哪些路径、挂哪个 parser、**安装渠道依赖哪些 Runtime**」；backfill、锁、备份拷贝、技能投影、用量聚合、**ensure_env** 在 service 层，各家复用同一套安全流程。
3. **UsageParser 独立目录** —— 日志格式漂移是最高频维护点，隔离后改解析器不碰业务；**模型筛选列表来自 usage 表去重，不是 parser 的额外 API**。
4. **backup 独立 service** —— 所有写 live 的路径（切换/卸载/恢复）共用快照与索引，避免 Adapter 漏备。
5. **skills 真源在 service** —— 矩阵与 lock 是跨 Agent 视图；Adapter 只提供目标目录（及未来落盘策略）。
6. **runtime 与 agent 解耦** —— Node/npm 等是共享前置环境，装一次多渠道受益；卸载 Agent **不**卸载 Runtime。禁止在 Adapter 内各自 `Command::new("node")` 散落检测。
7. **平台分流** —— `runtime::host_runtimes()` 决定 doctor/环境条探测集（PowerShell **仅 Windows**）；`runtime::native_install_requires()` 与 install catalog 决定 native 前置与展示命令（Windows `irm|iex` / macOS·Linux `curl|bash`）；Runtime 一键修复默认渠道 Windows=`winget`、macOS=`brew`、Linux=`manual`（也可传 `apt`/`dnf`/`pacman`/`zypper`/`apk`，只给可复制命令 / 官网，不自动 sudo；未知发行版不猜测 apt-get）。细节真源：[agenthub-plan.md §5.7.5](agenthub-plan.md)。
8. **commands 一文件一模块、薄到只做校验** —— 不要把大量 command 堆进同一个 `lib.rs`。
9. **models 纯数据、credentials 脱敏边界清晰** —— 当前版本沿用现有存储方案，DTO 出 core 前集中脱敏，避免 API、CLI、日志泄漏完整凭据。
10. **前端 invoke 单点 + mock 外置** —— 仅 `lib/backend/tauri/` 可 `invoke`；mock 只在 `dev/mocks/` 且仅由 `dev:mock` 注入；`build` 强制 Tauri；非 Tauri 生产页明确报错/unavailable。
11. **Bridge 数据面独立进程、Connections 领域不拆** —— `agenthub-adapterd` 只承接 `local_bridge` 的长驻 listener、协议转换和完整 saga；票、绑定、Account/Provider/live 事务继续由 core service 单点负责，避免按页面边界制造 `connectionsd` 或双写。生成 Provider 是绑定的私有 runtime，不是第二套钱包。
12. **跨工具复用按协议图规划、按绑定写入** —— `plan(ticket, agent)` 在 native / reshape / bridge / 不可行 中择一；用户层解释为 ① 直接改配置 / ② 写进对方认的登录 / ③ 本机转发，不新增领域枚举。`bind` / `unbind` 是唯一写入。扩大靠登记登录表面、Agent `accepts`（听哪种接口 **和** 认哪套订阅登录）/`writer` 和已测过的做法。见 [product-decisions.md](product-decisions.md)、[connection-binding-model.md](connection-binding-model.md)。
