# AgentHub 项目方案 v1.5

> 多 Agent 管理桌面工具：统一管理 Claude Code、Codex、Grok、Kimi 等 AI Agent 的安装、技能、API 配置、Token 统计与 OAuth 账号。  
> 技术栈：Tauri v2 + React + Rust，GUI + CLI 双端。  
> v1.1：补齐 Adapter / Service 职责边界、Skills 投影模型、备份流程、Token 统计与「模型列表」语义。  
> v1.2：CLI 命令树与配置三层契约见专文 [cli-and-config.md](cli-and-config.md)。  
> v1.3：**安装前置环境（Runtime）**：用户机可能无法直接装 Agent（缺 Node/npm 等），安装管线改为「检测环境 → 引导装环境 → 再装 Agent」。  
> v1.4：**平台环境差异**：PowerShell 仅 Windows 共享 Runtime；macOS/Linux native 安装/升级走官方 sh + bash，不得检测或要求 PowerShell；包管理引导 Windows=`winget`、macOS=`brew`。
> v1.5：**Adapter sidecar 目标架构**：`local_bridge` 的长驻 Runtime 与完整 saga 迁入用户级 `agenthub-adapterd`；Connections 与 live 配置事务继续由 core service 管理。当前实现仍为 Tauri 进程内宿主，按三阶段迁移。

系列文档：[产品决策（跨 Agent 复用三路）](product-decisions.md) · [目录结构与模块拆分](architecture.md) · [票 / 绑定 / 协议图](connection-binding-model.md) · [Adapter Sidecar 目标架构](adapter-sidecar-design.md) · [前端 UI 设计](ui-design.md) · [CLI 与配置契约](cli-and-config.md) · [Hub 重构 Phase 1 记录](hub-redesign-plan.md) · [DeepSeek Harness 接入](deepseek-harness-integration.md)

## 1. 已确认决策

| 决策点 | 结论 |
|---|---|
| 平台范围 | Windows 为主交付；macOS 已支持源码运行与本机构建；Linux 仅路径/命令抽象预留。**共享 Runtime 与 native 安装命令按宿主平台分流**（见 §5.7.2 / §5.7.5） |
| 复用策略 | 配置切换按「路径 + 读取 + 校验 + 原子写」；跨 Agent 复用分三路（① API 直连 ② 原生订阅 ③ 本机桥），见 [product-decisions.md](product-decisions.md)。实现从零自研 |
| MVP 范围 | Agent 安装/卸载（含**前置运行时检测与引导**）、API 配置管理、技能/插件管理、Token 统计、**票接到其他 Agent（直连 / 原生订阅 / 本机桥）** |
| 产品形态 | GUI + CLI 双端，核心逻辑抽成 `agenthub-core` crate 共享 |
| OAuth 账号管理 | 支持多账号池 + 一键切换；订阅先走目标原生槽（②），对不上再本机桥（③） |
| 跨 Agent 复用 | **核心产品**，三路都要做。能直连就直连，不默认常驻代理。实现未开 ≠ 产品不做。不做公网入口、多账号拼车、转售 |
| Token 统计来源 | **零侵入**：解析各 agent 本地日志/会话文件。这只约束 Usage，**不禁止** ③ 的本机桥 |
| Agent 范围 | **当前八家**：Claude / Codex / Kimi / Grok / Pi / WorkBuddy / **Cursor Agent**（半套 CLI）/ **DeepSeek Harness（`dsh`）**；不支持 Cursor IDE 私有库账号池。`dsh` 专项约束见 [deepseek-harness-integration.md](deepseek-harness-integration.md) |
| 分层原则 | **Service 管编排**（备份/锁/backfill/投影/聚合）；**Adapter 管差异**（路径、读写格式、解析器挂接） |
| Adapter 进程边界 | `local_bridge` 目标由同包用户级 `agenthub-adapterd` 托管；GUI/CLI 是控制客户端。Connections 不拆进程，OS 系统服务不在当前范围 |

## 2. 同类工具结论

### 配置与状态管理类工具 — 最直接参照系
- **借鉴**：按应用封装「路径 + 读取 + 校验 + 原子写」；供应商配置用灵活 JSON/`Value`，由前端预设模板决定；SQLite 存自身状态（带 schema 迁移）；backfill 机制（切换前把用户手改的 live 配置回存）；原子写（tempfile+rename）、TOML 编辑保留注释格式；前端 API 封装层。AgentHub **实际**为 `lib/backend` 分层 + 页面本地 state。
- **避坑**：Windows 下不要用 `HOME` 环境变量取 home dir（Git Bash 会注入错误值），用 `dirs::home_dir()`；巨型 `match` 分支散布多处（我们用 trait + 注册表替代）；官方 OAuth 登录态不能被配置切换覆盖，需识别保护。
- **差异化空间**：多 Agent 钱包 + 三路复用（直连 / 原生订阅 / 本机桥），而不是只做单一 CLI 的供应商预设，也不默认常驻代理。不做公网网关或多账号拼车。产品取舍见 [product-decisions.md](product-decisions.md)。

### 账号管理与工程实践类工具
- **借鉴**：后端分层（commands 薄层 → models → modules → utils）；索引 + 分文件 + SQLite 统计库的混合存储；OAuth loopback 回调（双栈监听、ephemeral 端口、state 校验）；token 提前刷新 + 每账号互斥锁防并发重复刷新；写第三方配置前必备份 + 原子写 + 路径白名单校验。
- **安全立场**：沿用现有凭据存储方案并保证输出脱敏；keyring、AES、主密码和密文迁移没有必要，不属于当前项目需求。

### OAuth 协议与凭据语义类工具
- **借鉴**：各平台 OAuth 的 PKCE 流程与 TokenProvider 模式（缓存 → 过期偏移检查 → 锁内单飞刷新）；账号模型 `platform + type + credentials + extra`；敏感字段集中脱敏，合并配置时不抹掉前端看不见的密钥。
- **不做**：多租户服务端机制（用户/支付/调度/Redis/Postgres）对单机桌面工具全是负担。无公开 OAuth 先例的平台走 API Key 或自研接入。

## 3. 各 Agent 适配矩阵（概要）

| Agent | 安装方式 | 渠道前置环境 | 配置 | 认证 | 技能 | Token 统计 |
|---|---|---|---|---|---|---|
| Claude Code | npm 或 native | npm 渠道需 Node.js + npm；native 走官方安装脚本 | 官方 settings / MCP 配置 | 文件型凭据 + 官方登录态（需保护） | 支持投影 | 会话/项目日志解析 |
| Codex | npm | Node.js + npm | TOML 供应商配置 | OAuth / 官方 auth 文件 | 支持投影 | 会话日志解析 |
| Kimi Code | native 官方脚本/二进制 | 通常不依赖 Node | TOML providers | OAuth 外置 + config 内 api_key | 暂不支持独立技能目录 | 会话目录解析 |
| Grok | native 官方脚本/二进制 | 通常不依赖 Node | TOML model 配置 | api_key / OAuth | 支持投影 | 会话目录解析 |
| Pi | npm only | Node.js + npm | settings / models JSON | provider 键控 auth | 支持投影 | 能力矩阵见实现 |
| WorkBuddy | native Setup only | 无 Node 依赖 | settings / models / MCP JSON | 官方 OAuth 落点；**P0 不切换账号** | 支持投影 | 能力矩阵见实现 |
| Cursor Agent | native 官方脚本 | Windows：PowerShell；Unix：bash/curl | 无稳定供应商模板（write fail-closed） | API Key / `agent login`；**禁止**私有 IDE 库账号池 | 产品 skill 目录可投影 | 非标准会话格式；usage 不支持 |
| DeepSeek Harness（`dsh`） | npm `@deepseek-ai/dsh` only | Node.js + npm | home 级 Cordis patch（只合并官方 LLM 插件行） | DeepSeek API Key 引用；无 OAuth | 投影到 `$DSH_HOME/skills`；对方也会读 `~/.agents/skills` | 解析会话日志 provider usage；启发式 Token Meter 不计费 |

> **Cursor Agent 边界**  
> - 卡片名 **Cursor Agent**；管公开 Agent CLI，不是把 IDE 可执行文件当 headless。  
> - 检测以校验过的 Agent CLI 为准，不把其他产品同名二进制误判为 Cursor。  
> - 不支持基于私有数据库的账号池切换。半套 CLI 管理见 `adapters/cursor.rs` 与 [adding-an-agent.md](adding-an-agent.md)。

> **Pi / WorkBuddy 边界（摘要）**  
> - Pi：npm 渠道；Provider 写回 `~/.pi/agent/models.json`，账号切换与 usage 以能力矩阵为准。
> - WorkBuddy：Electron 桌面 + bundled CLI；Provider 写回 `~/.workbuddy/models.json`；accountSwitch / usage 以能力矩阵为准。
> - 禁止改动安装包内部产品文件。
>
> **DeepSeek Harness 边界（已接入）**  
> - Agent id 用 `dsh`，不要和 DeepSeek API 票面混名。  
> - 只登记 npm 全局 `dsh`；`npx … web`、源码、Python SDK 不是安装渠道。  
> - 配置只改 home 级用户 patch；凭据只写引用，Key 进 `.credentials.yaml`。  
> - DeepSeek API → `dsh` 走现有 `AdapterCapabilityMatrix` + `config_sync`；DeepSeek → Claude 已开 experimental `native_endpoint`。专项方案见 [deepseek-harness-integration.md](deepseek-harness-integration.md)。

共享技能源：`~/.agents/skills/`（带 lock 清单），各 Agent 的 `skills` 目录是其投影目标，不是第二真源。

更细的路径与能力以 [capability-matrix.md](capability-matrix.md)、[adding-an-agent.md](adding-an-agent.md) 与代码中的 Adapter 为准。

## 4. 总体架构

```
┌─────────────────────────────────────────────────┐
│  Tauri GUI (React)          agenthub CLI        │
│  invoke commands            clap subcommands    │
├─────────────────────────────────────────────────┤
│              agenthub-core (Rust crate)          │
│  ┌───────────┐ ┌──────────┐ ┌────────────────┐ │
│  │ services  │ │ storage  │ │ AgentAdapter   │ │
│  │ (业务编排) │ │ (SQLite) │ │ trait + 注册表  │ │
│  └───────────┘ └──────────┘ └────────────────┘ │
│       adapters/: claude / codex / kimi / grok / pi / workbuddy │
│                  （每 agent 一个模块）            │
└─────────────────────────────────────────────────┘
```

**核心 crate 拆分**（`cargo workspace`）：
- `agenthub-core` — 全部业务逻辑，不依赖 Tauri 运行时，GUI 和 CLI 共用。这是「GUI + CLI 双端」的关键。
- `agenthub-gui` — Tauri v2 壳，commands 薄层只做参数校验/序列化。
- `agenthub-cli` — clap 命令行；**资源型子命令**（`agenthub provider switch` 等），完整命令树 / 退出码 / 配置分层见 [cli-and-config.md](cli-and-config.md)。

**后端分层**：`commands → services → adapters → storage`。`AppError`（thiserror）统一错误。

### 4.1 什么是「每个 Agent 一个 Adapter」

Adapter = 该 Agent 的**翻译官**：把「本机路径 / 文件格式 / 认证落点 / 日志位置」翻译成 AgentHub 统一接口。  
上层（GUI/CLI/Service）只说人话（检测、切换供应商、投影技能、采集用量），**不写** `if agent == claude { ... }` 巨型分支。

```rust
trait AgentAdapter {
    fn id(&self) -> AgentId;
    fn detect(&self) -> DetectResult;                 // 是否安装/版本/二进制路径
    fn install_channels(&self) -> Vec<InstallChannel>; // 渠道 id/标签 + requires: [RuntimeId]
    fn read_config(&self) -> AgentConfig;             // 当前 API 配置（live）
    fn apply_provider(&self, p: &Provider) -> Result<()>; // 只负责写 live；备份由 service 先做
    fn read_auth(&self) -> AuthState;                 // 当前认证状态（脱敏）
    fn apply_account(&self, a: &Account) -> Result<()>;   // 账号凭据写入
    fn supports_skills(&self) -> bool;
    fn skills_dir(&self) -> Option<PathBuf>;          // 技能投影目标；不支持则 None
    fn live_backup_paths(&self) -> Vec<PathBuf>;      // 写前应快照的 live 文件清单
    fn usage_source(&self) -> Option<Box<dyn UsageParser>>; // 日志解析器；无则 None
}
```

**新增一个 agent = 新增一个 adapter 模块 + 注册表注册一行。**  
**共享运行时（Node 等）不属于某个 Adapter**，由 `env_service` / `runtime/` 统一检测与引导安装；Adapter 只声明「某渠道依赖哪些 RuntimeId」。

### 4.2 Service vs Adapter 职责边界（强制）

| 能力 | Service（编排，跨 Agent 统一） | Adapter（差异，单 Agent） |
|---|---|---|
| 供应商/账号切换 | 校验 → backfill → **调用 backup** → 锁 → 调 adapter 写盘 | 读/写该 Agent 的 config/auth 文件格式 |
| 技能管理 | 扫真源、矩阵状态、lock、批量同步、冲突策略 | `supports_skills` / `skills_dir`；演进期可下沉 install/remove 落盘方式 |
| 备份 | 时间戳目录、索引表、恢复前再备份、导出包 | 提供 `live_backup_paths()` |
| Token 统计 | 调度采集、去重入库、聚合、定价估算 | 挂接 `UsageParser`；路径/格式在 parser 内 |
| 安装/卸载 | **先 ensure_env** → 再封装官方渠道命令、二次确认、卸载前备份 | `detect`；`install_channels()`（含 `requires`） |
| 前置环境 | 检测/引导安装共享 Runtime（Node/npm…）；doctor 汇总 | 不实现 Runtime 检测（只声明依赖） |

**禁止**在 `*_service` 内堆叠 `match agent_id` 写文件细节；差异一律进 Adapter / UsageParser。  
**禁止**在 Adapter 内各自实现一套备份策略或技能真源扫描（会复制多套安全流程）。

**MVP 取舍**：Skills 的落盘可先由 `skill_service` 统一「复制到 `skills_dir()`」；若出现第二种方式（链接 / manifest），再把 `install_skill` / `remove_skill` 下沉到 Adapter，避免 service 长成第二个巨型 match。

## 5. 核心模块设计

### 5.1 存储
- 自身数据目录 `~/.agenthub/`：`agenthub.db`（SQLite，rusqlite bundled，WAL 模式，schema 版本迁移）+ `backups/`。
- 表：`providers`（API 配置）、`accounts`（账号池）、`skills`（技能记录）、`usage_records`（token 统计）、`backups`（备份索引）、`settings`。
- **凭据安全**：
  - 当前方案没有必要做凭据落盘加密，沿用现有存储方案；所有 API、CLI、Tauri 返回值和日志必须脱敏；
  - keyring、AES、主密码和密文迁移不属于项目需求；
  - 备份导出能力尚未实现，按现有存储方案导出，不规划额外加密层。

### 5.2 API 配置管理（Providers）
- 数据模型：`Provider { id, agent_id, name, settings_config: Value, meta }` —— `settings_config` 不建模具体字段，前端预设模板决定（灵活模式，新 agent 后端零改动）。
- 切换流程：校验 → **backfill**（live 文件可能被用户手改，先回存当前 provider）→ **`backup_service` 快照 live** → 原子写（adapter）→ per-agent 切换锁防并发。
- 写 TOML 用保留注释的编辑器；写后注意重投影 MCP 等关联配置，避免重写清掉无关段落。
- 识别并保护官方 OAuth 登录态，不被 API 配置切换覆盖。
- **backfill ≠ backup**：backfill 是把 live 内容写回 AgentHub 的 provider/account 池；backup 是把 live 文件拷到 `~/.agenthub/backups/live/...` 供回滚。

### 5.3 账号管理（Accounts）+ 一键切换
- 模型：`Account { id, agent_id, type: oauth|apikey, credentials(按当前方案存储), extra(邮箱/计划/配额快照), status }`。
- 一键切换流程：检测 agent 进程在运行 → 提示关闭（或自动重启）→ backfill 当前凭据存回账号池 → **备份 live 凭据文件** → 写入目标账号凭据（原子写）→ 需要时刷新 token → 验证生效。
- 添加账号：
  - **API Key 型**：直接录入。
  - **OAuth 型**：内置 loopback 回调服务器完成 PKCE 流程，授权页自关闭，token 自动入库。
  - **导入现有**：从 live 凭据文件抓取当前登录态存为账号。
- Token 刷新：TokenProvider 模式（内存缓存 → 过期偏移检查 → 每账号互斥锁单飞刷新 → 失效标记）。

### 5.4 技能/插件管理

**模型**：共享真源 + 按 Agent 投影（不是每家各自维护一套技能库）。

```
真源（一份）                    投影目标（多份）
~/.agents/skills/     →   各 Agent 的 skills 目录
  <skill-id>/         →   （Adapter 声明 skills_dir）
                      →   无独立技能目录的 Agent：unsupported
```

- **`skill_service`（主导）**：扫描真源与 lock、维护「技能 × Agent」同步矩阵、启用/禁用、批量同步、冲突策略（目标已存在且内容不同 → 覆盖/跳过，默认不静默覆盖）。
- **Adapter（辅助）**：`supports_skills()` / `skills_dir()`；不负责真源扫描与矩阵状态。
- Windows 下投影**优先复制**，少用符号链接（权限问题）。
- 产品规则：真源优先；Agent 目录是派生视图。若用户只在 Agent 侧改 skill，P2+ 可补「从 Agent 目录导入/回收到真源」，MVP 以单向投影 + 冲突提示为主。
- **插件 ≠ 技能**：各 Agent 的 plugins 目录做**只读展示 + 移除**，不进入技能同步矩阵，避免用户以为插件也能「一键同步」。

### 5.5 Token 统计（零侵入）与模型列表

#### Token 统计
- **Usage 不做**本地代理、**不**劫持请求；只读解析各 Agent 已有会话/日志。③ 本机桥是另一条产品线，见 [product-decisions.md](product-decisions.md)。
- 每 agent 一个 `UsageParser`（独立 `usage/` 目录，经 Adapter `usage_source()` 挂接）→ 统一  
  `UsageRecord { agent, account?, model, input/output/cache tokens, cost?, ts, session_id }`  
  → 增量入库（文件 offset / 哈希去重）。
- 解析容错：格式失配跳过并计数，不中断整次采集。大文件只读打开、增量游标。
- `usage_service`：调度采集（手动「立即采集」+ 可选定时）、聚合查询、`pricing.rs` 估价。
- 前端：recharts 趋势图；按 agent / 账号 / **模型** / 天聚合；底部 ParserHealth（条数、失败率，失败率高提示日志格式可能变更）。

#### 「模型列表」语义（重要：不是模型商店）

| 来源 | 用途 | 是否独立产品页 |
|---|---|---|
| **A. 用量去重** | `SELECT DISTINCT model FROM usage_records` → Dashboard 用量段筛选下拉、明细列 | 否，附属 Dashboard 用量 |
| **B. Provider 预设/配置** | 模板里的 `model` / `default_model` 字段，编辑 API 配置时使用 | 否，附属 Providers |
| **C. pricing 价目表** | 模型名 → 单价(USD/1M) → 估算 `costUsd`（与价表同单位，无汇率） | 离线内嵌；`pnpm pricing:update` + 每日 CI 从公开价目源刷新开 PR；overrides 补本地模型 |

**明确不做（当前路线图）**：联网拉取官方「可购/可用全量模型目录」、模型市场、一键切换官方模型清单。若未来要做「账号可用模型探测」，单独立项评估（P4+），不与 Usage 混为一谈。

### 5.6 备份（Agent live + 自身数据）

备份由 **`backup_service` 统一编排**，不在各 Adapter 内各自实现策略。

#### 两套备份

| 类型 | 对象 | 路径 | 目的 |
|---|---|---|---|
| **A. Agent live** | 各 Agent 磁盘上的配置/凭据等 | `~/.agenthub/backups/live/<agent>/<ts>/` | 切换/卸载写坏后可回滚 |
| **B. 自身数据** | `agenthub.db` 等 | `backups/db/`、`exports/`（导出能力预留） | 应用数据与换机 |

#### 触发与类型（`BackupKind`）

| kind | 何时 |
|---|---|
| `auto-switch` | 切换 Provider / Account **写 live 之前** |
| `manual` | Backups 页 / Dashboard「立即备份」 |
| `pre-uninstall` | 卸载（尤其删配置）之前 |
| （恢复路径） | 恢复前**先对当前 live 再备份一次** |

#### 流程（以切换供应商为例）

```
校验 → backfill → backup_service.snapshot(agent)  // 问 Adapter.live_backup_paths()
     → adapter.apply_provider(...)                // 原子写
     → 索引写入 backups 表
```

典型 live 文件由各 Adapter 声明（配置、凭据等随版本可调）。  
**默认不备份**：会话日志 / 大体积 projects 树 / 大体积本地统计库（Usage 只读解析即可）。  
技能投影若需可回滚，由 skill 流程按需备份**将被覆盖的目标文件**，不并入每次 API 切换的 live 包。

路径白名单 + 拒绝危险字符（`is_safe_path`），防止任意路径读写。

### 5.7 安装/卸载管理

#### 5.7.1 两阶段安装（强制）

真实用户机常见情况：**没有 Node/npm、PATH 未刷新、仅有 winget、公司机禁装**——此时「直接点安装 Agent」必然失败，且错误信息难懂。

```text
选择 Agent + 渠道
    → Phase A  ensure_env(channel.requires)
         · 已满足 → 继续
         · 缺失   → 返回 EnvNotReady（结构化：缺什么、怎么装、原生命令）
                    或用户勾选「同时安装依赖」后 env_service 引导安装 Runtime
    → Phase B  install_agent(channel)
         · 封装官方命令，流式输出
         · 成功后 re-detect
```

**原则**：

1. **先环境，后 Agent**。`agent install` 在前置缺失时**不得**盲跑官方命令后甩一长串 npm 报错。
2. **共享 Runtime 与 Agent 解耦**。Node 装一次，Claude/Codex 的 npm 渠道共用；禁止在多个 Adapter 里各写一套 `which node`。
3. **克制**：AgentHub 不是通用包管理器。MVP 只覆盖安装 Agent **硬依赖**的 Runtime；不装 IDE、不装 Docker、不替用户改系统策略。
4. **可降级**：引导安装失败或无权限时，始终给出**可复制的原生命令 / 官方下载页**，用户可自行装完后点「重新检测」。

#### 5.7.2 Runtime 清单（MVP）

| RuntimeId | 典型检测 | 谁需要 | Windows | macOS / Linux |
|---|---|---|---|---|
| `nodejs` | `node -v` + 路径 | Claude/Codex/Pi 等 **npm** 渠道 | ① `winget install OpenJS.NodeJS.LTS` ② 官网 LTS ③ 可复制命令 | ① `brew install node` ② 官网 ③ 包管理器提示 |
| `npm` | `npm -v`（通常随 Node） | 同上 | 随 Node；node 在 npm 不在 → 修 PATH / 重装 Node | 同左 |
| `powershell` | 5.1（`powershell`）与 7（`pwsh`）双探针，任一可用即可 | **仅 Windows** native `.ps1` 渠道 | **只检测、不一键安装**；ExecutionPolicy 提示 | **不检测、不展示、不作为渠道前置**；native 走官方 sh |
| `git` | `git --version` + 路径 | Skills 市场 / git URL 安装 | ① `winget install --id Git.Git` ② 官网 | ① `brew install git` ② 官网 |
| `curl` / 系统下载器 | 可选（执行 native sh 时用） | 部分官方一键脚本 | 系统自带 / 浏览器降级 | 系统 `curl`；缺失时提示手动下载 |

版本门槛：在 Adapter/渠道元数据中声明 `min_version`（如 Node ≥ 18）；detect 返回 `ok | outdated | missing`。

**检测范围（硬约束）**：

- `env_service.detect_all()` / doctor 的 `runtimes[]` **只返回宿主相关 Runtime**（实现：`runtime::host_runtimes()`）。
- Windows：`nodejs` / `npm` / `powershell` / `git`。
- macOS / Linux：`nodejs` / `npm` / `git`（**不含** `powershell`）。
- 对 PowerShell 的显式 `detect_one` 在非 Windows 上必须 fail-soft（标记 not applicable / not required），**禁止** spawn `pwsh` 或把缺失当成环境故障。

#### 5.7.3 Agent 检测与安装命令

- **Agent 检测**：扫描 npm/pnpm 全局、常见用户 bin 目录、PATH；Windows 下执行命令加 `CREATE_NO_WINDOW`。
- **Runtime 检测**：`env_service.detect_all()` / `detect(RuntimeId)`；结果缓存短 TTL，安装后强制失效；**范围见 §5.7.2**。
- **安装/升级 Agent**：仅在 Phase A 通过后，封装官方渠道命令，捕获输出展示。
  - **npm**：各平台均为 `npm i -g <pkg>`（升级同路径 / latest）。
  - **native Windows**：allowlist 的 `install.ps1`，经 PowerShell `irm … | iex`（需 PowerShell）。
  - **native macOS/Linux**：allowlist 的 `install.sh`，经 `curl … | bash`（**不**要求 PowerShell）。
  - **CLI 入口统一**：`agenthub agent install|upgrade <id>`；底层命令由 core 按平台选择。
- **卸载**：官方卸载 + 可选清理配置目录（二次确认 + **pre-uninstall 备份**）。**不**因卸载 Agent 而卸载 Node/Git（共享运行时）。
- 路径白名单校验，拒绝危险字符防注入。

#### 5.7.4 结构化结果（GUI/CLI 共用）

```text
DetectResult（Agent）     : installed | not_found | path/version/channel
EnvStatus（Runtime）      : ok | missing | outdated | broken_path
InstallPlan               : channel + required_runtimes[] + agent_command
EnvNotReady               : missing[] + remediations[]（winget|brew|命令|url）+ can_auto_fix
```

`doctor` 必须同时报告 **Agent 安装态** 与 **宿主相关 Runtime 健康**（见 [cli-and-config.md](cli-and-config.md)）。macOS doctor 的 `runtimes[]` **不得**出现 PowerShell 行。

#### 5.7.5 平台环境差异（硬约束）

| 主题 | Windows | macOS | Linux（预留） |
|---|---|---|---|
| 共享 Runtime 探测 | Node / npm / **PowerShell** / Git | Node / npm / Git | 同 macOS |
| Runtime 一键修复默认渠道 | `winget` | `brew` | 无自动包管理器时仅 URL/命令 |
| native 渠道前置 | `requires: [powershell]` | `requires: []` | `requires: []` |
| native 安装/升级命令 | `irm <allowlisted-ps1> \| iex` | `curl -fsS <allowlisted-sh> \| bash` | 同 macOS |
| 仅 Windows 有 ps1 的 Agent（如 Codex native） | 展示 native 渠道 | **不**暴露 Windows-only ps1 为 native；优先 npm 或官网 | 同 macOS |
| 打开官网 Setup | `cmd /C start` | `open` | `xdg-open` |
| GUI 环境条 | 可显示 PS 5.1/7 双版本芯片 | **不**显示 PowerShell 芯片 | 同 macOS |
| 适配器 `install_channels().requires` | 与 catalog 一致，可用 `runtime::native_install_requires()` | 同左；**禁止**在 Unix 上硬编码 PowerShell | 同左 |

**禁止事项**：

1. 在 macOS/Linux doctor / 环境条把「未安装 pwsh」标成环境故障。
2. 在 Unix native catalog 展示 `irm … | iex` 或把 PowerShell 写进 `requires`。
3. 在前端 remediation 给 macOS 用户推 `winget`，或给 Windows 用户推 `brew`（可用 `platform` 标记过滤）。
4. 在多个 Adapter 内复制 `which node` / PowerShell 探测；统一走 `runtime/`。

## 6. 前端设计

- 技术：React + TypeScript + Vite + Tailwind + shadcn/Radix（**只选一套 UI**）+ recharts + react-router + CodeMirror。**当前未**引入 TanStack Query / i18next（方案历史提及，以 `package.json` 为准）。
- 结构：`lib/backend/tauri`（唯一 invoke）→ `lib/api` façade → 页面本地 state；mock 仅 `dev:mock`。事件桥为目标态，现以前端主动拉取为主。
- 页面：Dashboard（含用量）/ Chat / Agents / Connections（目标：跨 Agent 钱包）/ Adapter（侧栏「桥与适配」，只管桥 runtime）/ Skills / MCP（只读清单）/ Projects / Settings（含 Backups）。日常绑定从 Dashboard「连接/切换」、Connections「接到…」发起。领域目标见 [connection-binding-model.md](connection-binding-model.md)。
- 详细交互见 [ui-design.md](ui-design.md)。

## 7. 分期路线图

| 阶段 | 内容 |
|---|---|
| **P0 脚手架** | cargo workspace（core/gui/cli）、SQLite 层 + 迁移、AgentAdapter trait + 注册表、基础 detect；**Runtime detect（至少 node/npm）**；CLI：`doctor`（含 runtimes）/ `agent list` / `config path` / 可选 `env list`（见 cli-and-config §8） |
| **P1 MVP 上半** | Claude Code + Codex adapter；Providers CRUD + 切换（backfill/原子写/备份）；Skills 管理；Dashboard + Connections/Skills；CLI：provider/skill/backup + `-o json`/`-y`；presets 进 core；**Agents 页展示「环境未就绪」态（只检测+复制命令，可不自动装 Node）** |
| **P2 MVP 下半** | Kimi + Grok adapter；**`agent install` 两阶段 + 可选引导装 Node（winget/命令）**；UsageParser + Dashboard 用量段；Accounts 池 + 文件型凭据一键切换 |
| **P3** | 各平台凭据管理完善；OAuth 添加账号（loopback PKCE） |
| **P4 二期候选** | macOS/Linux 适配；配置备份同步；token 自动刷新守护；代理模式（热切换不改 live 文件，独立评估）；可选「账号可用模型探测」（非 Usage 附属） |

## 8. 当前实现状态（以代码与测试为准）

### 8.1 已落地

| 规划项 | 状态 |
|---|---|
| 文档集（方案 / 架构 / UI / 票与绑定 / CLI / 日志 / 能力矩阵 / Chat 过程） | ✅ |
| cargo workspace：`agenthub-core` / `agenthub-cli` / `src-tauri`（gui） | ✅ |
| 八家 Adapter + `capability()` 矩阵 + 一致性测试 | ✅ |
| Runtime detect + 两阶段 install（ensure_env→agent） | ✅ GUI Agents 页 + CLI `env` / `agent install` |
| Provider 池 CRUD + live 切换（backfill/backup/原子写/锁） | ✅ Tauri + CLI |
| Account 池 + 文件型 import/switch/apikey + OAuth 手动 refresh | ✅ |
| OAuth PKCE loopback | ✅ 已支持的平台见代码与 CLI |
| Skills 投影 + install/uninstall/update/project/market | ✅ core + CLI + Tauri；前端 Skills 页接线 |
| Backup live list/create/restore/delete | ✅ |
| Usage 增量采集 + Dashboard 图表/明细/ParserHealth | ✅（部分 Agent 按能力矩阵 Unsupported） |
| Chat 多 Agent 对话 + 过程面板 | ✅ 结构化流以能力矩阵为准 |
| Projects 列表/删除/摘录 | ✅（部分 Agent Partial） |
| 前端 backend 分层（tauri / mocks / contracts / api façade） | ✅；`pnpm build` 强制 Tauri + 护栏 |
| CLI `run` 多 Agent headless | ✅ |
| 日志 tracing 文件 + 脱敏 | ✅ 见 logging.md |
| Adapter 规则分析 / 预览 / profile 管理 | ✅ 当前可 bind：Kimi Provider→Claude/Pi reshape、Kimi Provider→Codex bridge、Anthropic Provider/Account→Pi reshape；② Claude/Codex/Grok OAuth Account→Pi 已可 experimental bind，分别写入 `auth.json` 的 `anthropic` / `openai-codex` / `xai` 槽（规则 v1），并由 Pi 拥有该槽刷新；③ 带 access token 的 Codex `auth_json` 订阅→Claude Responses 已可 experimental `local_bridge` bind（规则 v1），目标只写 loopback URL 与本地 bearer；App Server/OauthOther 仍关闭。其余见[适配规则矩阵](provider-api-oauth-adaptation.md#4-当前实现矩阵)。写入入口 `bind`/`unbind`，投影不进钱包，见 [connection-binding-model.md](connection-binding-model.md) |
| Hub 统一连接流程 ConnectFlowDialog | ✅ Phase 1 外壳已落地。目标 UI：全局钱包 + 真票「接到…」。`plan.canApply` 只表示现在能写入 |
| MCP 本机配置清单 | ✅ core 只读扫描 + Tauri command + 前端页面；不修改或注入配置；管理/注入仍 Planned，无假 UI |
| Settings 持久化 | ✅ L1 SQLite 白名单（`SETTINGS_WHITELIST`）：`theme` / `language` / `log_level` / `log_retention_days` / `skill_market_source` / `close_to_tray` / `usage_collect_interval_min`。用量间隔：`None`=从未写入（前端默认 30）、`0`=仅手动、上限 1440。主题以 core 为准：Settings Select 预览、Save 落盘，启动 `getSettings` 对账。`autoStart` 为 OS 登录项；`closeToTray` 写 core 并同步 AppState |
| 凭据落盘加密 | **范围外**（不实现，非待办） |

### 8.2 未实现 / 仅部分 / 范围外

| 项 | 状态 | 说明 |
|---|---|---|
| 备份**导出包**（换机） | ❌ | `exports/` 预留；无 command；`Backend.features.backupExport=false`，无 UI 入口 |
| 测速 / 切换撤销 | ✅ | 生产已接线：`undo_switch_*` + `test_provider_latency`；`Backend.features` 打开入口；导出包仍关 |
| 自身 **DB 备份**（`backups/db/`） | ❌ | 仅 live 快照 |
| Dashboard **生产告警** | 🟡 | 生产从 doctor 派生 auth/env/update 告警（本地 dismiss）；无独立告警总线。mock 可演示额外样例 |
| Tauri **事件桥** | ❌ | 文档目标；现以前端 refetch 为主 |
| MCP **管理 / 注入**、`ModelSelect`、`SessionResume` | Planned | `Mcp` 矩阵仍表示管理/注入能力；独立的只读 MCP inventory 已落地，不改变矩阵状态 |
| 票 / 绑定读模型与钱包重做 | ✅ 读模型、进口打标、`plan` 收口、拒投影、`bind`/`unbind` 写入已落地；§6.4 部分落地（OpenAI/xAI/GLM/DeepSeek API → Pi，属 ①）；Grok API 之外的边仍未开放 bind；§6.5 Claude bind 已开（GLM/DeepSeek → Claude，属 ①）；② Claude/Codex/Grok 订阅 Account → Pi 已可 experimental bind，写入 Pi `auth.json` 并由 Pi 拥有刷新；③ Codex `auth_json` 订阅→Claude Responses 已可 experimental `local_bridge` bind，App Server/OauthOther 仍关闭；见 [product-decisions.md](product-decisions.md)；§6.6 未做 | [connection-binding-model.md](connection-binding-model.md)：`list_ticket_wallet` / `plan_ticket` / `bind_ticket` / `unbind_ticket`。`canApply` = 现在 bind 会成功（有实现且 secret 可按票 `source_kind` 解析）。可写边：Kimi 会员 Provider → Claude/Pi/Codex，Anthropic / OpenAI / xAI API（Provider 与 Account）→ Pi，GLM Coding Plan / DeepSeek API（Provider 与 Account）→ Pi 自定义 provider，Anthropic API（Provider 与 Account）→ Codex，GLM Coding Plan / DeepSeek API（Provider 与 Account）→ Claude，带 access token 的 Codex `auth_json` 订阅 → Claude Responses（③）。Kimi 会员 Account 仍无分类规则。Codex App Server、OauthOther → Claude 未开。写入走 bind，apply 为薄兼容委托 |
| Adapter 本地 Bridge 产品接线 | 🟡 部分实现 | core host、协议转换、Tauri controller、UI 控件、auto-start 恢复与退出 drain 已进入当前工作区；具体可执行状态见[适配规则矩阵](provider-api-oauth-adaptation.md#4-当前实现矩阵)，端到端验收尚未收口 |
| Adapter 用户级 sidecar | 🎯 目标已决策 / 未实现 | 当前 `BridgeRuntimeHost` 仍由 Tauri `AppState` 持有；待完成 Tauri-neutral control contract、`agenthub-adapterd`、本地 IPC、单实例/版本+schema 握手、SQLite shared/exclusive schema lease、更新/卸载 saga 和分阶段切换，见 [adapter-sidecar-design.md](adapter-sidecar-design.md) |
| 远程 Skill 市场 | 🟡 部分实现 | 已接线公开市场搜索/安装；依赖网络与本机 Git |
| Token **后台自动刷新守护** | ❌ | 有手动 refresh |
| Settings 语言切换 / i18n | ❌ | `language` 可写入 L1，UI 仅为中文只读说明，无 i18next |
| Usage **后台守护 / 文件监听** | ❌ | 仅前台 interval + 手动 |
| 官方模型商店 / 账号可用模型探测 | ❌ | 明确非目标（用量去重模型列表除外） |
| WebDAV / 代理模式 / macOS·Linux 一等公民 | P4 候选 | 未开工 |
| Chat 后续阶段（diff 预览、过程落库等） | ❌ | 见 chat-process-streaming.md |
| 部分 Agent 的 ConfigWrite / 账号切换 / Usage | Unsupported | 设计边界，见能力矩阵 |
| CLI Provider create/update/delete | ❌ | 与 GUI 不对称，脚本侧靠 import-live + switch |
| TanStack Query / i18next 等 | ❌ | 方案提及；`package.json` 未依赖 |
| 凭据 keyring/AES/主密码 | 范围外 | 见 AGENTS.md |
| DeepSeek Harness（`dsh`）生产接入 | ✅ P1–P5 | `AgentId::Dsh`、npm detect/install、home patch + 凭据引用、Skills/Usage/Projects、headless text run、DeepSeek API → `dsh` `config_sync`。DeepSeek→Claude experimental `native_endpoint` 已开。StructuredStream 仍 Planned，见 [deepseek-harness-integration.md](deepseek-harness-integration.md) |

### 8.3 前端导航（与代码 `App.tsx` 一致）

- Workspace：Chat / Agents / Skills / MCP / Projects。
- Manage：Dashboard（含用量）/ Connections / 桥与适配 / Settings（含 Backups）。
- 推荐发起入口：Dashboard 卡片「连接/切换」、Connections「接到…」。`/adapter` 只管理桥 runtime。目标钱包见 [connection-binding-model.md](connection-binding-model.md)。

旧路由 `/router` → `/adapter`；`/usage` → `/?section=usage`；`/backups` → `/settings?tab=backups`；`/providers`·`/accounts` → `/connections`。

## 9. 风险与开放问题

1. **官方凭据落点随版本变化**：部分 Agent 的主登录态未必落在公开配置文件中。账号切换以文件型凭据导入/备份为先，未确认的路径不强行写入。
2. **日志格式漂移**：各家 sessions 格式会随版本变。UsageParser 设计为容错（跳过失配记录 + 统计失败率 + 按 agent 版本选择解析器）。
3. **合规边界**：定位是个人本机工具，默认不提供公网分发或多账号共享。三路复用是产品能力；③ 的非官方通道风险对用户可见。用户须遵守各上游服务条款。
4. **写第三方配置的跟进成本**：各家配置格式都会变，适配层需要持续维护。本机桥只服务 ③，按 fixtures 与回滚开放边；①② 不起桥。
5. **Skills 真源假设**：以 `~/.agents/skills` 为唯一真源；若用户长期只在 Agent 目录改 skill，需补导入/回收，否则仅是单向投影器。
6. **前置环境安装的权限与策略**：公司机可能禁用 winget/MSI、Node 装完但 GUI 进程 PATH 未刷新、需要「新开终端/重启 AgentHub」才能看到 `node`。产品文案与 `doctor` 需覆盖 **PATH 刷新 / 重启提示**；自动装 Runtime 失败必须降级为可复制命令，禁止假成功。
7. **不替官方背锅**：Runtime/Agent 安装脚本来自上游；AgentHub 只编排与展示。网络失败、镜像源、证书问题在 UI 中归类为「环境/网络」并给出官方文档入口。
8. **sidecar 双主、版本与 schema 漂移**：GUI、CLI 和 sidecar 若同时持有 Bridge host、分别写 `local_bridge` profile，或旧进程跨 SQLite migration 继续访问，会产生重复监听、跨进程半事务和数据库不兼容。必须坚持每个 data dir 单一 runtime owner、mutation 经 IPC、Database handle 生命周期 shared schema lease / migration exclusive lease、rule/revision/schema 重验和不兼容版本拒绝写入；普通 GUI 退出不再等于停止 sidecar。
