# Agent 能力矩阵设计（v1.0 已落地）

> 状态：**P0-P4 与 R00-R08 已落地**（2026-08-07 与代码及测试对齐）；P13/OCP 已由 test-only `demo-agent` 验证。矩阵仍只回答“能不能”，平台端口收口与兼容边界见 [platform-capability-remediation.md](platform-capability-remediation.md)。
> 真源在 adapter `capability()`；GUI/CLI 由 `registry.matrix()` 下发；CLI：`agenthub agent capabilities [--markdown]`。  
> 关联：[architecture.md](architecture.md)、[platform-capability-refactor.md](platform-capability-refactor.md)、[adding-an-agent.md](adding-an-agent.md)。  
> **矩阵表格请以 CLI 输出为准**；下文 §5 为文档快照。  
> **实现来源**：`Full`/`Partial` 对应行为须在 adapter 或已注册的 platform 端口中存在；`Planned` 不得伪装为可调用。
> **身份边界**：矩阵仍服务内置 Agent 的兼容 `AgentId` façade；Usage 非空持久化、RunService 和 Project legacy DTO/ID 解析的 `AgentId` 约束属于后续可选迁移。

## 1. 为什么要做（历史背景，问题已收敛）

接入的 Agent 从 4 家涨到 7 家时，「某个 Agent 能做什么」曾散落在多处（trait 默认方法、`ProcessMode` 白名单、stream_parse match、service 分支、前端手抄 capabilities）。其中前端 `usage: true` 与 core 未实现 Usage 曾明显漂移。

本设计落地后：

- 真源 = 各 adapter 的穷尽 `match` on `Capability`
- 服务层经 `registry.require(...)` 闸门
- 前端 `src/config/agents.ts` **仅展示元数据**（无 capabilities 镜像）；生产能力来自 doctor / agent 列表下发
- Usage 已实现：六家 Full，Cursor Unsupported（见 §5）

仍用四级状态表达：**Partial**（可放行须提示）、**Unsupported**（对方边界）、**Planned**（我们未接）。

## 2. 设计原则

1. **能力是代码事实，不是用户配置。** 真源在 adapter 源码里，随实现一起改；不外部化成 JSON/TOML。
2. **区分「能不能」与「怎么做」。** 见 §3，这是避免矩阵膨胀成上帝表的关键。
3. **编译期穷尽优于运行时查表。** 能力键用 enum，adapter 用不带 `_ =>` 的 `match` 应答；新增能力时 7 个 adapter 全部编译失败，逼出逐家决策。
4. **声明必须被测试绑死。** 没有一致性测试的声明只是第 6 份会漂移的注释。

## 3. 关键判断：什么该进矩阵

现存的 `match agent` 分支是**两类东西**，必须分开治理：

| 类别 | 语义 | 去向 | 例子 |
|---|---|---|---|
| **「能不能」** | 调用方据此决定放行或拒绝 | 进能力矩阵 | `supports_skills`、`write_config` 是否 fail-closed、是否支持结构化流 |
| **「怎么做」** | 是参数/数据，不是能力 | 下沉为 adapter 方法 | npm 包名、安装脚本 URL、`agent_home` 映射、`managed_toml_provider_keys` |

第二类**不要**塞进矩阵。`install_service.rs:107-170` 的 `npm_package` / `native_ps1_url` / `native_sh_url`、`utils/paths.rs:71-94` 的 `agent_home`、`adapters/mod.rs:181-199` 的 `managed_toml_provider_keys` 都应该变成 trait 方法（`fn npm_package(&self) -> Option<&'static str>`、`fn home_dir(&self) -> Result<PathBuf>` …），和实现放在同一个文件里。这是**并行的**清理项，不属于本设计的范围，但两件事一起做才能真正消掉 `adding-an-agent.md` 的清单。

账号池「同人多授权 / 同票去重」属于第二类（怎么认授权），**不进矩阵**；见 [account-authorization-pool.md](account-authorization-pool.md)。矩阵只回答 `AccountSwitch` / `ApiKeyAccount` 能不能。

## 4. 数据模型

```rust
// crates/agenthub-core/src/models/capability.rs

/// 能力键。新增变体会让所有 adapter 编译失败 —— 这是刻意的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    // —— 已有调用方 ——
    ConfigWrite,
    AccountSwitch,
    ApiKeyAccount,
    Skills,
    LiveBackup,
    StructuredStream,
    DangerousMode,
    ProjectHistory,
    ProjectDelete,
    ProviderPresets,
    // —— 预留（当前无调用方，见 §6）——
    Usage,
    Mcp,
    ModelSelect,
    SessionResume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapabilityLevel {
    /// 已接入且完整。
    Full,
    /// 已接入但有降级，调用方可放行但必须向用户说明。
    Partial,
    /// 目标 CLI 本身做不到，或无稳定契约（fail-closed）。不会因 AgentHub 开发而改变。
    Unsupported,
    /// CLI 侧可行，AgentHub 尚未接入。矩阵在这里兼作路线图。
    Planned,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CapabilityState {
    pub level: CapabilityLevel,
    /// 降级 / 不支持 / 未接入的原因。直接用于 UI tooltip 与 CLI 报错文案。
    /// 用 `&'static str`：能力原因是编译期事实，无需运行时格式化。
    pub reason: Option<&'static str>,
    /// 预留：某能力需要 CLI 版本门槛时填。当前全为 None，不提前建模。
    pub min_version: Option<&'static str>,
}
```

**四级而非布尔**，三个理由：

- `Partial` 表达 Kimi 危险模式、Cursor 项目列表这类"能用但要提示"的状态；
- `Unsupported` 与 `Planned` 语义完全不同——前者是**对方 CLI 的边界**（永久），后者是**我们的待办**（会变）。合并二者会让 UI 无法区分"做不到"和"还没做"，也会让矩阵失去当路线图的价值；
- `reason` 让所有拒绝都自带解释。现在 `skill_service.rs:382` 给用户的是 `skills are not supported for kimi`，矩阵化后能给出"Kimi CLI 无技能目录模型"。

## 5. 现状矩阵（7 家 × 14 项；CLI 快照 2026-08-03）

> 生成：`cargo run -p agenthub-cli -- agent capabilities --markdown`

| Capability | claude | codex | kimi | grok | pi | workbuddy | cursor |
|---|---|---|---|---|---|---|---|
| ConfigWrite | Full | Full | Full | Full | Full | Full | **Unsup** |
| AccountSwitch | Full | Full | Full | Full | Full | **Unsup** | **Unsup** |
| ApiKeyAccount | Full | **Partial** | Full | Full | **Partial** | **Unsup** | **Partial** |
| Skills | Full | Full | **Unsup** | Full | Full | Full | Full |
| LiveBackup | Full | Full | Full | Full | Full | Full | **Unsup** |
| StructuredStream | Full | Full | Full | Full | Full | **Unsup** | **Unsup** |
| DangerousMode | Full | Full | **Partial** | Full | **Partial** | Full | Full |
| ProjectHistory | Full | Full | Full | Full | Full | Full | **Partial** |
| ProjectDelete | Full | Full | Full | Full | Full | Full | **Unsup** |
| ProviderPresets | Full | Full | Full | Full | **Unsup** | **Unsup** | **Unsup** |
| Usage | Full | Full | Full | Full | Full | Full | **Unsup** |
| Mcp | Planned | Planned | Planned | Planned | Planned | Planned | Planned |
| ModelSelect | Planned | Planned | Planned | Planned | Planned | Planned | Planned |
| SessionResume | Planned | Planned | Planned | Planned | Planned | Planned | Planned |

非 Full 单元格的依据与拟定 `reason`（路径/解析细节以 adapter 与 `project_service` 源码为准，不在此展开）：

| 单元格 | reason |
|---|---|
| ConfigWrite / cursor | 无稳定配置写入契约，fail-closed |
| AccountSwitch / workbuddy | UI：`暂不支持账号池切换` |
| AccountSwitch / cursor | UI：`账号由 Cursor 管理`；不读写 IDE 私有账号库 |
| ApiKeyAccount / codex | 可入池；live 应用仅支持官方 OAuth 凭据形态 |
| ApiKeyAccount / pi | 可入池；provider schema 不稳定，不写回 live |
| ApiKeyAccount / cursor | 仅入池；需用户自行完成官方登录或环境变量 |
| Skills / kimi | Kimi CLI 无技能目录模型 |
| LiveBackup / cursor | 无稳定配置/凭据文件契约，不备份 IDE 内部库 |
| StructuredStream / workbuddy·cursor | CLI 仅提供 text 输出，无结构化事件流 |
| DangerousMode / kimi | headless 下危险开关与提示模式互斥，不生效 |
| DangerousMode / pi | 仅信任项目本地文件，非完全跳过确认 |
| ProjectHistory / cursor | 仅工作区目录列表，无会话 transcript |
| ProjectDelete / cursor | 目录内部结构无安全浅删契约 |
| ProjectHistory / 各支持家 | 只读扫描 agent home 下已知会话/项目布局；mtime 索引可加速 |
| ProviderPresets / pi·workbuddy | 暂无内置 provider 模板（手工 Provider 仍可写回） |
| ProviderPresets / cursor | 无 provider 配置契约 |

**注意**：`Skills` 与 `ProviderPresets` 之外，本矩阵不重复 `install_channels()` 已返回的结构化数据——安装渠道是数据不是能力（§3）。

## 6. 预留与已实现能力键说明

| Capability | 说明 | 当前事实（2026-08-03） |
|---|---|---|
| `Usage` | token / 计费统计 | **已实现**：`UsageService` + session 日志解析；六家 Full。Cursor **Unsupported**（IDE 内部用量库，范围外）。矩阵声明须与 `usage::supports_usage` 一致。 |
| `Mcp` | MCP server 管理 / 注入 | **全 Planned**。配置层可能保留/备份 MCP 文件，但无 MCP 管理 Service；headless run 不传 MCP 参数。 |
| `ModelSelect` | 运行时指定模型 | **全 Planned**。模型经 live config / provider 池切换，非独立运行时目录。 |
| `SessionResume` | 续接历史会话 | **全 Planned**。Chat 不用各 CLI 原生续会话能力。 |

**填表纪律**：无本地验证证据前不得把 Planned 改成 Full。依据 `adding-an-agent.md`——「本地验证 > 仅 README」。

**约束**：`Mcp` / `ModelSelect` / `SessionResume` 在对应 Service 落地前**不得产生调用方**。`require` 遇 `Planned` 与 `Unsupported` 同样拒绝，错误文案不同。

## 7. 真源与防漂移

### 7.1 声明在 adapter 内

trait 新增一个**无默认实现**的方法，强制每家表态：

```rust
// adapters/mod.rs
pub trait AgentAdapter: Send + Sync {
    // …既有方法…
    fn capability(&self, cap: Capability) -> CapabilityState;
}
```

各 adapter 用**不带通配符**的 `match` 实现：

```rust
// adapters/cursor.rs
fn capability(&self, cap: Capability) -> CapabilityState {
    use Capability::*;
    match cap {
        ConfigWrite => unsupported("无稳定配置写入契约，fail-closed"),
        AccountSwitch => unsupported("账号由 Cursor 管理"),
        ApiKeyAccount => partial("仅入账号池；需自行设 CURSOR_API_KEY 或 agent login"),
        Skills => full(),
        LiveBackup => unsupported("无稳定配置/凭据文件"),
        StructuredStream => unsupported("Agent CLI 仅提供 text 输出"),
        DangerousMode => full(),
        ProjectHistory => partial("仅工作区目录列表，无会话 transcript"),
        ProjectDelete => unsupported("无安全浅删契约"),
        ProviderPresets => unsupported("无 provider 配置契约"),
        Usage => unsupported("IDE 内部用量库，明确范围外"),
        Mcp | ModelSelect | SessionResume => planned("待验证接入"),
    }
}
```

禁用 `_ =>` 是本设计的支点：能力键是 enum + 穷尽匹配，所以"新增能力时漏配某家 Agent"从运行时 bug 变成编译错误。

不引入 `capabilities! { }` 之类的声明宏——省下的字数不抵新人读不懂真源的代价。

### 7.2 汇总在 registry

```rust
impl AdapterRegistry {
    /// 全局视图，供 GUI / CLI / 文档生成使用。
    pub fn matrix(&self) -> BTreeMap<AgentId, BTreeMap<Capability, CapabilityState>>;
}
```

### 7.3 一致性测试（矩阵可信的前提）

声明与行为必须绑死，否则矩阵仍只是注释。参照已有的 `well_known_paths_cover_all_agents_non_empty`（`adapters/mod.rs:732`）风格，加一组遍历全 registry 的断言：

```rust
#[test]
fn declared_capabilities_match_actual_behavior() {
    for adapter in register_all().all() {
        let id = adapter.id();

        if adapter.capability(Capability::Skills).is_blocked() {
            assert!(adapter.skills_dir().is_none(), "{id} 声明无 skills 却给了目录");
            assert!(!adapter.supports_skills());
        }
        if adapter.capability(Capability::ConfigWrite).is_blocked() {
            assert!(matches!(
                adapter.write_config(&probe_config(id)),
                Err(AppError::Unsupported(_))
            ));
        }
        if adapter.capability(Capability::LiveBackup).is_blocked() {
            assert!(adapter.live_backup_paths().is_empty());
        }
        if adapter.capability(Capability::StructuredStream).is_blocked() {
            assert!(!ProcessMode::Auto.wants_structured_for(id));
        }
        if adapter.capability(Capability::ProviderPresets).is_blocked() {
            assert!(presets::list_for(id).is_empty());
        }
    }
}

/// 所有非 Full 状态必须给出原因 —— 否则 UI 只能显示一个没有解释的置灰。
#[test]
fn non_full_capabilities_must_explain_themselves() {
    for adapter in register_all().all() {
        for cap in Capability::ALL {
            let state = adapter.capability(cap);
            if state.level != CapabilityLevel::Full {
                assert!(state.reason.is_some(), "{}/{cap:?} 缺少 reason", adapter.id());
            }
        }
    }
}
```

## 8. 服务层收口

现在每处判断各自拼错误消息。收敛成单一闸门：

```rust
impl AdapterRegistry {
    pub fn require(&self, agent: AgentId, cap: Capability) -> Result<Arc<dyn AgentAdapter>> {
        let adapter = self.get(agent)?;
        let state = adapter.capability(cap);
        match state.level {
            CapabilityLevel::Full | CapabilityLevel::Partial => Ok(adapter),
            CapabilityLevel::Unsupported => Err(AppError::Unsupported(format!(
                "{} 不支持{}：{}",
                agent.display_name(), cap.label(), state.reason.unwrap_or("未提供原因")
            ))),
            CapabilityLevel::Planned => Err(AppError::Unsupported(format!(
                "{}的{}尚未接入 AgentHub：{}",
                agent.display_name(), cap.label(), state.reason.unwrap_or("路线图项")
            ))),
        }
    }
}
```

于是 `skill_service.rs:382-384` 这类代码变成一行：

```rust
let adapter = self.registry.require(agent, Capability::Skills)?;
```

`Partial` **放行**——它是"能用但要提示"，拒绝会让功能倒退。降级提示由调用方按 `reason` 渲染。

## 9. 静态能力 vs 运行时可用

两个正交维度，**不得合并**：

| 维度 | 来源 | 例子 |
|---|---|---|
| 静态能力 | `capability()` | Cursor 不支持账号切换（装了也不行） |
| 运行时状态 | `DetectResult` | 没装 / 版本不够 |

有效状态是两者的合取，但必须分开建模——否则 UI 只能显示同一个置灰，而用户在两种情况下需要采取**完全不同的行动**（一个是"换个 Agent"，一个是"去装"）。

```rust
impl AdapterRegistry {
    pub fn effective(&self, agent: AgentId, cap: Capability, detect: &DetectResult)
        -> EffectiveCapability;
}
```

`min_version` 字段现在全为 `None`。等真出现版本门槛的能力再填——没有实例的提前建模是空转。

## 10. 前端契约

`src/config/agents.ts` 只保留**展示元数据**（`id/name/color/letter/installChannels`），`capabilities` 字段整个删除，改由后端下发。

能力是静态的，直接并入现有 `listAgents()` 响应即可，不增加往返：

```ts
export type CapabilityLevel = 'full' | 'partial' | 'unsupported' | 'planned';
export interface AgentCapability {
  level: CapabilityLevel;
  reason?: string;
}
export type AgentCapabilities = Record<Capability, AgentCapability>;
```

UI 侧的具体变化：

| 位置 | 现状 | 改后 |
|---|---|---|
| `src/pages/accounts/index.tsx` | `accountDisabledAgents(statuses)` 只读 detect/doctor 矩阵；无 statuses 时不猜 MOCK |
| `src/pages/connections/index.tsx:133` | 硬编码一句通用 `disabledReason` | 渲染后端 `reason` |
| `src/pages/skills/SkillMatrix.tsx:176` | `agent_unsupported` 无原因 | 单元格 tooltip 显示 `reason` |
| Chat 危险模式开关 | Kimi 静默失效 | `partial` 渲染为黄色 badge + 提示 |
| Dashboard 用量 / Chat 危险模式 | 布尔或静默 | 由 `Usage` / `DangerousMode` 能力驱动（含 partial 提示） |

`partial` 态正好落实 [ui-design.md](ui-design.md) §1.5「能力不齐是常态」——现有布尔模型表达不了它。

## 11. 不要做

- **不要**把矩阵外部化成 JSON/TOML 配置文件。挪出代码即失去编译期穷尽检查，还多给用户一个改坏的入口。
- **不要**用 `HashMap<String, bool>`。字符串键 = 放弃穷尽检查 = 回到今天。
- **不要**在 adapter 的 `match` 里写 `_ =>` 兜底。
- **不要**把「怎么做」类数据（npm 包名、URL、home 路径）塞进矩阵（§3）。
- **不要**为预留能力键（§6）提前写调用方。
- **不要**在前端与 Rust 并行维护两份能力表——这正是本设计要消灭的东西。
- **不要**凭上游文档给 `Planned` 项填 `Full`，须本地验证。

## 12. 分阶段落地

| 阶段 | 内容 | 影响面 | 风险 |
|---|---|---|---|
| **P0** | `models/capability.rs` + trait `capability()` + 7 家 adapter 声明 + §7.3 一致性测试 | 纯新增，无调用点改动 | 无 |
| **P1** | 服务层换 `registry.require(...)`；删除 `supports_account_switch` / `supports_skills` | `account_service` / `skill_service` / `project_service` | 中：错误文案变化，需同步测试断言 |
| **P2** | `wants_structured_for` 白名单迁入矩阵；`stream_parse` 的 `match` 加矩阵断言守卫 | `models/run.rs`、`utils/stream_parse` | 中：触及 Chat 主链路 |
| **P3** | 矩阵下发 GUI/CLI；前端删除 capabilities 镜像；`partial` 态 UI | Tauri command + 前端多页 | 低 |
| **P4** | `agenthub agent capabilities [--json\|--markdown]`；由矩阵生成文档表格 | CLI + docs | 低 |

P1/P2 之间可插入 §3 的「怎么做」下沉清理（`npm_package` / `agent_home` / `managed_toml_provider_keys` 迁入 trait），两者合并后才能真正缩短 `adding-an-agent.md`。

## 13. 验收

- `Capability::ALL.len()` × `AgentId::ALL.len()` 个单元格全部有显式声明（编译器保证）。
- 所有非 `Full` 单元格都有 `reason`（测试保证）。
- 声明与行为一致（§7.3 测试保证）。
- 全仓搜不到 `supports_account_switch` / `supports_skills` 的残留调用。
- `src/config/agents.ts` 不再含 `capabilities` 字段。
- 接入第 8 家 Agent 时，遗漏的能力声明表现为**编译错误**而非文档清单里的一行。
- `agenthub agent capabilities --markdown` 的输出与 §5 表格一致（P4 后由生成器覆写本节）。
