# 模型厂商、API 与 OAuth 适配规则

> 状态：**当前工作区规则**，不代表已发布版本。
> 最近核对：2026-08-15。
> 本文是厂商入口、凭据类型和**协议图上的边**的规则真源。**产品方向**（① API 直连 / ② 原生订阅复用 / ③ 本机桥）以 [product-decisions.md](product-decisions.md) 为准。领域对象（票 / 绑定 / 规划器）见 [connection-binding-model.md](connection-binding-model.md)；页面与运行时见 [adapter-design.md](adapter-design.md)、[ui-design.md](ui-design.md)。§4 是**当前可执行矩阵**，不是 UI 白名单，也不是产品终点。

## 1. 先看结论

是否能够接到某个 Agent，由票的表面和图上的边决定，而不是由「从哪个 Agent 导入」或「account 还是 provider」决定：

```text
票面（产品 + 凭据类 + 上游协议） × Agent 入口（accepts + writer） → native | reshape | bridge | 不可行
```

商品组合（如 Kimi 会员 → Claude）是图的一次求值。扩大靠登记新票面、声明 Agent `accepts`/`writer`、给图加边。见 [connection-binding-model.md](connection-binding-model.md)。

是否能够适配，仍由下面四项共同决定：

```text
来源产品 + 凭据类型 + 上游协议 + 目标客户端协议
```

厂商名称相同、都使用 OAuth，或都宣传“OpenAI 兼容”，都不足以证明可以互换。

| 概念 | 含义 | 例子 |
|---|---|---|
| 来源产品 | Key 或 Token 实际属于哪个产品 | Kimi Code 会员、Kimi 开放平台 |
| 凭据类型 | 凭据的签发与刷新方式 | API Key、PKCE OAuth、device code |
| 上游协议 | 服务实际接受的请求格式 | Anthropic Messages、Chat Completions、Responses |
| 目标客户端 | 最终读取配置并发起请求的 Agent | Claude Code、Codex、Pi |

硬规则：

1. **API Key 与 OAuth 分开判断**：支持某厂商 API Key，不等于支持其订阅 OAuth。
2. **协议必须写全名**：`OpenAI-compatible` 必须进一步区分 Chat Completions 与 Responses。
3. **同厂商不同产品不得混用**：Base URL、Key、额度和授权范围都可能不同。
4. **只认显式来源标记**：进口写下 `surface`；不能根据名称、标签或 URL 猜测。未识别标 `unknown`，规划结果是不可行，而不是把「接到…」藏掉。
5. **默认拒绝写入**：没有代码规则和测试的组合一律不能 `bind`。用户仍看得到原因。
6. **不复制凭据**：绑定只引用票；真实凭据只在写入 live 或请求上游时短暂解析。生成投影不是新票。

### 1.1 跨 Agent 复用：三路，订阅不等于要起桥

产品按 [product-decisions.md](product-decisions.md) 的三路判定，不要把所有订阅写成第 3 路：

| 路 | 何时 | 起桥 |
|---|---|---|
| ① API 端点直连 | 上游 Key 已提供目标协议（双协议 Key 是典型） | 否 |
| ② 原生订阅复用 | 目标有同一 OAuth 契约槽（如 Pi 的 Anthropic / Codex / xAI 槽） | 否 |
| ③ 本机协议桥 | 协议或契约对不上，图上有转换边（如 Codex 订阅 → Claude） | 是，仅 loopback |

安全与运营边界（约束部署形态，不否决产品）：

- ③ 的上游 token 不可导出、不可显示、不可复制到目标 Agent；目标只得到本地 loopback bearer。② 写的是目标自己的官方槽，不是把 token 翻译成另一家 Key。
- 不监听公网地址，不作为远程服务、团队共享端点、多租户网关、转售或额度池。
- 每条边仍要单独做分类、refresh、协议 fixtures 与回滚；不能因为「同为订阅」或「同为双协议」就自动 `canApply=true`。
- 打开 `bind` 的条件是工程就绪。③ 的非官方通道风险对用户可见并需 opt-in，**不再**当作「未获官方书面批准就不能做这条产品」。

§4 里「订阅仍关 / canApply=false」只描述**当前实现**，不描述产品方向。规划结果应对用户可见。

## 2. 厂商与产品入口

下表描述协议事实，不等于 AgentHub 已实现对应的跨 Agent 路由。实现状态以 [§4](#4-当前实现矩阵) 为准。

| 厂商 / 产品 | 常见凭据 | 协议或客户端约束 | AgentHub 当前结论 |
|---|---|---|---|
| Anthropic API / Claude Code | Anthropic API Key；Claude 官方登录 | Claude Code 可连接 Anthropic Messages 兼容网关 | Key → Pi 是 ①（已可 bind）。Claude 订阅 → Pi 是 ②（产品要做，写 Pi Anthropic 槽）。Claude 订阅 → Codex 是 ③ 或暂不可行 |
| OpenAI API / ChatGPT / Codex | OpenAI API Key；ChatGPT subscription 登录 | Codex 支持 ChatGPT subscription 登录；自定义 Provider 仍要求 Responses | Key → Pi 是 ①（已可 bind）。Codex 订阅 → Pi 是 ②（写 `openai-codex` 槽）。Codex 订阅 → Claude 是 ③（产品要做，**当前** `canApply=false`）。OpenAI → Grok 不造边 |
| Kimi Code 会员平台 | 会员 API Key，**不是 OAuth** | 同一产品提供 Anthropic Messages 与 OpenAI Chat Completions 兼容入口 | 已有 Claude 直连、Codex 实验 Bridge、Pi 预览规则 |
| Kimi 开放平台 | 开放平台 API Key | 使用独立 Base URL、额度和产品契约 | 不与 Kimi Code 会员 Key 混用；当前无 Adapter 路由 |
| 智谱 GLM Coding Plan | Coding Plan API Key，**不是 OAuth** | 提供 Anthropic Messages 与 OpenAI Chat Completions 入口；套餐仅限官方支持的工具环境 | 已登记票面；① Claude bind 已开；① → Pi / → Codex 未开或要掉进 ③（Chat ≠ Responses） |
| DeepSeek API | DeepSeek API Key，**不是 OAuth** | 提供 Anthropic Messages 与 OpenAI Chat Completions 兼容入口；部分 Anthropic 字段会被忽略或不支持 | 已登记票面；① Claude bind 已开；① → DSH 已开；① → Pi 未开；→ Codex 属 ③（Chat ≠ Responses） |
| xAI / Grok | xAI API Key；xAI 登录 | API 与账号授权是不同入口 | 显式 xAI API Key → Pi 可 bind；xAI → Grok 是原生切换，不进矩阵 |
| Google Gemini | Gemini API Key 或 Google 授权 | 原生 API 与 OpenAI 兼容入口需分别声明 | 仅作为候选来源；当前无 Adapter 路由 |

### 2.1 Kimi Code 会员双协议入口

| 协议 | Base URL | 常用 Endpoint 示例 |
|---|---|---|
| OpenAI Chat Completions 兼容 | `https://api.kimi.com/coding/v1` | `https://api.kimi.com/coding/v1/chat/completions` |
| Anthropic Messages 兼容 | `https://api.kimi.com/coding/` | `https://api.kimi.com/coding/v1/messages` |

`OpenAI 兼容`在这里特指 **Chat Completions**，不包含 Codex 所需的 Responses。因此：

- Kimi Code 会员 → Claude Code 可以使用 Anthropic 兼容入口直连。
- Kimi Code 会员 → Codex 需要本地协议转换，不能只改 Base URL。

### 2.2 Kimi Code 与 Kimi 开放平台

| 对比项 | Kimi Code 会员平台 | Kimi 开放平台 |
|---|---|---|
| Base URL | OpenAI Chat：`https://api.kimi.com/coding/v1`<br>Anthropic：`https://api.kimi.com/coding/` | `https://api.moonshot.cn/v1` |
| 凭据 | Kimi Code Console 创建的会员 API Key | Kimi 开放平台 API Key |
| 额度 | Kimi Code 会员编程权益 | 开放平台计费与额度 |
| 是否可混用 | 否 | 否 |

Kimi CLI `/login` 生成的 managed OAuth 又是第三种来源，不能伪装成上述任一 API Key。

### 2.3 GLM Coding Plan

| 协议 | Base URL | 适配边界 |
|---|---|---|
| Anthropic Messages | `https://open.bigmodel.cn/api/anthropic` | 可作为 Claude Code 等官方支持工具的原生端点候选 |
| OpenAI Chat Completions | `https://open.bigmodel.cn/api/coding/paas/v4` | 只代表 Chat Completions，不代表 Codex Responses |

GLM Coding Plan 的凭据和使用范围必须单独识别：

- 个人版和团队版均使用 Coding Plan API Key；官方明确说明团队套餐 Key 与平台其他 API Key 不通用。
- 套餐额度仅限官方列出的工具与产品环境，新增规则前必须确认目标工具仍在支持列表中。
- 官方 Coding Tool Helper 当前可管理 Claude Code、OpenCode、Crush 和 Factory Droid；它只能证明这些工具存在官方配置路径，不代表 AgentHub 已实现适配。
- GLM Coding Plan 已登记票面；**Claude bind 已开**（①，experimental `native_endpoint`）。GLM → Pi（①）未开；→ Codex 属 ③。

### 2.4 DeepSeek API

| 协议 | Base URL | 适配边界 |
|---|---|---|
| OpenAI Chat Completions | `https://api.deepseek.com` | Base URL 不追加 `/v1`；不代表 Codex Responses |
| Anthropic Messages | `https://api.deepseek.com/anthropic` | 官方支持 Anthropic SDK 与 Claude Code，但不保证所有 Anthropic 扩展字段无损 |

DeepSeek 使用平台签发的 API Key，不是 OAuth。官方 Anthropic 兼容表中存在“忽略”或“不支持”的字段，因此新增规则时必须按目标 Agent 实测文本、流式输出、thinking、工具调用、停止原因和用量，不能只验证请求成功。

DeepSeek API 票和 DeepSeek Harness（Agent `dsh`）不是同一对象：

| 目标 | 预期路线 | 当前状态 |
|---|---|---|
| Claude Code | `native_endpoint`：Anthropic 兼容入口 | **可实验应用**；`rule_id=deepseek-api-to-claude-v1`；preset `deepseek-api` 与 `deepseek` 均识别 |
| DeepSeek Harness（`dsh`） | `config_sync`：凭据引用 + 官方 provider 槽（常见 `deepseek-official`） | **可应用**；`rule_id=deepseek-api-to-dsh-v1`。识别靠 preset `deepseek-api` / `deepseek` 或 host `api.deepseek.com`，**不要**仅凭 `agent_id=dsh` 升级 |
| Codex | 默认 `unsupported` | Chat Completions 不代表 Responses |

DeepSeek API 已登记票面；**Claude bind 已开**（①，experimental `native_endpoint`）。→ DSH 已开（①）；→ Pi（①）未开；→ Codex 属 ③。接到 `dsh` 时走对方官方 LLM adapter，不把 Harness 当 Messages↔Responses 桥，也不把 OAuth 票写入其凭据缝。

## 3. 路由类型

| 路由 | 使用条件 | 是否运行本地服务 |
|---|---|---|
| `config_sync` | 目标 Agent 原生支持同一凭据和协议，仅需转换配置结构 | 否 |
| `native_endpoint` | 上游原生提供目标协议，只需写 Base URL、模型与凭据引用 | 否 |
| `local_bridge` | 授权允许，但上下游协议不同，且已有经过测试的转换器 | 是，仅 loopback |
| `unsupported` | 产品、凭据、协议、版本或授权边界未验证 | 否 |

Bridge 转换的是请求、流式事件、工具调用、停止原因和用量字段，不会把 OAuth Token “转换”为另一家 API Key。

## 4. 当前实现矩阵

下表是**现在能写入的边**，不是产品上限。目标扩大方式见 [connection-binding-model.md §6](connection-binding-model.md#6-扩大在本模型里怎么做)。

`plan()` 是**唯一规划出口**：route / maturity / canApply / reason 只在这里计算。矩阵仍是图；`canApply` = 矩阵开放 ∩ plan 私有 `write_gate`。`write_gate` 表示「有 bind 实现 ∧ secret 可按该票 `source_kind` 解析」。Account 与同表面 Provider 走同一条边（相同 route / support / reason 主旨）。本步可写的 Account 同边是 **Anthropic / OpenAI / xAI API Key account → Pi**、**Anthropic API Key account → Codex**，以及 **GLM Coding Plan / DeepSeek API account → Claude**；写入入口是 `bind`（`apply_adapter` 为薄兼容委托）。不要把「无规则」当成 Account 不可写的原因。

| 显式来源 | 目标 | 分析结果 | 当前可执行状态 |
|---|---|---|---|
| Kimi Provider，`agent_id=kimi` 且 `meta.preset=kimi-code-membership` | Claude Code | stable `native_endpoint` | **可应用**；普通 Apply 服务当前唯一白名单 |
| 同上 | Codex | experimental `local_bridge` | **可实验应用**；`plan.canApply=true`，由 Tauri 专用 Bridge 路径执行，尚未完成端到端验收 |
| 同上 | Pi | stable `config_sync` | **可应用**；写入 Pi `models.json` 的 `kimi-for-coding` 槽，凭据只引用 |
| Anthropic Provider（显式 Anthropic API Key） | Pi | stable `config_sync` | **可 bind**；写入 Pi `models.json` 的 `anthropic` 槽，凭据只引用 |
| Anthropic Account（`credentials.format=api_key`） | Pi | stable `config_sync` | **可 bind**；与上一行同边；`adapterSourceRef.kind=account`，不先复制成 Provider 票 |
| Anthropic Provider（显式 Anthropic API Key） | Codex | experimental `local_bridge` | **可实验应用**；`plan.canApply=true`，下游 Responses → 上游 Anthropic Messages（`x-api-key` + `anthropic-version`），由 Tauri 专用 Bridge 路径执行，尚未完成端到端验收 |
| Anthropic Account（`credentials.format=api_key`） | Codex | experimental `local_bridge` | **可实验应用**；与上一行同边；`adapterSourceRef.kind=account`，不先复制成 Provider 票 |
| OpenAI Provider / Account（preset / extra.provider / `api.openai.com`） | Pi | stable `config_sync` | **可 bind**；写入 Pi `models.json` 的 `openai` 槽（API Key 槽，不是 `openai-codex` OAuth），凭据只引用 |
| xAI Provider / Account（preset / extra.provider / `api.x.ai`） | Pi | stable `config_sync` | **可 bind**；写入 Pi `models.json` 的 `xai` 槽，凭据只引用。xAI → Grok 是原生切换，不进矩阵 |
| GLM Coding Plan Provider / Account（preset / extra.provider / 官方 host） | Claude Code | experimental `native_endpoint` | **可实验应用**；写入 `https://open.bigmodel.cn/api/anthropic`，凭据只引用 |
| DeepSeek API Provider / Account（preset `deepseek-api` / `deepseek` / 官方 host） | Claude Code | experimental `native_endpoint` | **可实验应用**；写入 `https://api.deepseek.com/anthropic`，凭据只引用 |
| DeepSeek API Provider（preset `deepseek-api` / `deepseek` 或 host `api.deepseek.com`） | DeepSeek Harness（`dsh`） | stable `config_sync` | **可应用**；写入 home 级官方 provider 引用，Key 只进 `.credentials.yaml`，不进 `cordis.patch.yml` |
| Codex OAuth Account，`credentials.format=auth_json`（ChatGPT subscription） | Claude Code | **产品目标边**（本机桥） | **当前实现** `maturity=preview`，`plan.canApply=false`：还不能创建 profile、启动 bridge 或写入 Claude。Phase 1 纯协议内核已在 core；缺的是 transport / session / apply，不是产品否决。见 [product-decisions.md](product-decisions.md) |
| 其他来源、目标或未标记记录 | 任意 | `unsupported` | 不产生写操作 |

补充边界：

- Kimi managed OAuth 不会被识别为 Kimi Code 会员 API Key。
- Kimi Code 会员识别：**`meta.preset=kimi-code-membership`**，或配置中出现官方端点 **`api.kimi.com/coding`**（无 preset 的 live import 仍可识别）。仅 `agent_id=kimi` 或 Moonshot 开放平台 **不会**升为会员。
- 普通 OpenAI、xAI 只认显式标记（preset / extra.provider / 官方 host）；自定义中转保持 `unknown`，不可 bind。OpenAI/xAI → Pi 已可 bind；Kimi→Grok、OpenAI→Grok 不造边；xAI→Grok 不进矩阵（native）。
- GLM Coding Plan、DeepSeek API 已登记票面（speaks 可双协议），classify 只认显式标记；**Claude bind 已开**（①，experimental `native_endpoint`，Provider 与 Account）；DeepSeek → DSH **已可应用**（①，Provider，`deepseek-api-to-dsh-v1`）。GLM/DeepSeek → Pi（①）未开。②③（订阅 → Pi / 订阅桥）实现未开，见 [product-decisions.md](product-decisions.md)。
- Gemini、Kimi 开放平台或任意“兼容 API”目前都不会自动升级为 Adapter 规则。
- `stable` / `experimental` / `preview` / `none` 是 `plan.maturity`：矩阵开放+Stable → `stable`；矩阵开放+Experimental → `experimental`；有 cell 但 gates 关或仅可解释 → `preview`；无边 / Other → `none`。`canApply` 仍只表示现在能写入。
- Kimi → Codex 与 Anthropic API Key → Codex 是当前两条 Bridge 可写路径，不代表已经提供通用协议网关。
- 当前 Bridge 数据面按 profile/route 选择上游：Kimi 走 Chat Completions + bearer；Anthropic 走 Messages + `x-api-key` / `anthropic-version`。它不是 Codex OAuth 上游、Anthropic Messages 下游或通用 Responses 网关。

## 5. OAuth 边界

AgentHub 当前可发起的登录与跨 Agent 适配是两套能力：

| 登录目标 | AgentHub 当前入口 | 跨 Agent 复用（产品 / 实现） |
|---|---|---|
| Claude | PKCE | ② → Pi Anthropic 槽（产品要做）。③ → Codex（或暂不可行）。**当前**跨 Agent bind 未开 |
| Codex / ChatGPT | PKCE | ② → Pi `openai-codex` 槽（产品要做）。③ → Claude 本机桥（产品要做，**当前** `canApply=false`） |
| Grok / xAI | PKCE | ② → Pi xAI 槽（产品要做）。③ → Claude（或暂不可行）。**当前**跨 Agent bind 未开 |
| Pi | Anthropic PKCE、OpenAI Codex PKCE、xAI device code | **第 2 路的标准落点**：只写入 Pi 对应槽；不能推导其他 Agent 也有这些槽 |
| Kimi | 当前没有 AgentHub OAuth 登录入口 | 会员 API Key 走 ①/③，与 Kimi CLI managed OAuth 必须分开 |

OAuth access/refresh token 带有客户端、受众、范围和刷新语义。只有目标客户端公开支持相同契约，并且 AgentHub 增加显式规则与测试后，才允许 `config_sync`；否则应引导用户使用目标客户端自己的登录流程。

### 5.1 Codex / ChatGPT subscription → Claude Code：第 3 路，实现未开

该组合是 **③ 本机协议桥** 的旗舰边（对齐 cc-switch 的 Codex OAuth 反代、CLIProxyAPI 的 Codex 兼容口），**不是** ②：Claude Code 没有 ChatGPT 订阅槽。目标：Claude Code 通过 `ANTHROPIC_BASE_URL` 与 `ANTHROPIC_AUTH_TOKEN` 调用**本机** bridge，而不是把 ChatGPT OAuth token 写入 Claude Code。Codex 订阅 → Pi 走 ②，不要和本条混写。

**当前实现**仍不能 bind：`canApply=false`、不创建 profile、不启动 bridge。这是工程缺口，不是产品否决。见 [product-decisions.md](product-decisions.md)。

打开 `canApply` 前必须选定一条可工作的上游 transport，并用 fixtures 证明协议闭环。参考项目能跑是产品证据，不是可以把对方源码贴进仓库的许可。官方 App Server 与 Responses 通道都要做 spike；**不再**要求「官方契约 / 条款书面批准」才允许把这条边当产品：

| 候选 | 必须回答的问题 | 未通过时 |
|---|---|---|
| Codex App Server transport | 请求/流式事件语义是否足以承载 Claude Code 回合；工具、上下文与取消如何映射；是否会造成双 Agent、双工具执行或意外副作用 | 继续 `canApply=false`；记下缺口，换下一条候选 |
| Codex Responses transport（含参考项目已走通的本机反代通道） | OAuth + Responses 能否稳定刷新、流式/工具/取消能否闭环、失败会不会留下死 loopback | 继续 `canApply=false`；不把未测通道标成已开放 |

被选定的 transport 必须证明：身份只用于当前用户，token 不跨 IPC 泄露，刷新不导致并发风暴，协议闭环正确，且失败不会留下可用的 Claude Code loopback 配置。两条都未就绪时，UI 仍展示这条产品边为可预览，并给出 Claude 自身登录或已支持 API Key 作为临时替代。

### 5.2 订阅桥接的分层契约（设计目标，未实现）

订阅桥接不得把现有 Kimi resolver 或 Adapter 页面扩展成隐式 OAuth proxy。目标职责如下：

```text
Connection / Account（core services owner）
  → SourceIdentity + credential classifier
  → SubscriptionSessionProvider
  → UpstreamTransport
  → ProtocolKernel / IR
  → DownstreamSurface
  → user-level agenthub-adapterd sidecar
```

| 层 | 职责与边界 |
|---|---|
| `SourceIdentity` / credential classifier | 只以来源产品、账户、`credential_kind`、授权范围和显式 metadata 分类；拒绝名称猜测及 API key/OAuth 混用。 |
| `SubscriptionSessionProvider` | 通过 core 的 AccountService 解析、refresh、single-flight 和状态查询；只返回短生命周期的授权请求上下文，绝不经 GUI/sidecar IPC 返回原始 secret。当前 Codex Account 为 `auth_json`，AccountService 具备 refresh 能力；`AdapterSecretResolver` 目前只解析 Kimi Provider，不能假定其已支持 Codex。 |
| `UpstreamTransport` | 封装一个经门禁批准的 App Server spike 或 Codex Responses transport；不让协议映射层、UI 或目标客户端猜端点。 |
| `ProtocolKernel` / IR | 纯请求、事件和错误映射；不读数据库、不刷新凭据、不监听端口。 |
| `DownstreamSurface` | 按协议暴露最小 loopback surface：本候选为 Anthropic Messages；现有 Kimi 路径仍为 Responses。 |
| sidecar runtime | `agenthub-adapterd` 是 `local_bridge` 唯一运行时/监听 owner；Connections、Account、Provider 与数据库/live-config 事务仍由 core services owner 持有。 |
| capability matrix | 对每一 source × credential × transport × target × protocol × version 记录门禁、限制、fixtures 与验证日期；缺项即 fail-closed。真源：`crates/agenthub-core/src/models/adapter_capability_matrix.rs`（`ADAPTER_CAPABILITY_MATRIX` / `decide_adapter_capability` / `CODEX_SUBSCRIPTION_TO_CLAUDE_REASON`）。analyze 对外附带结构化 `ruleId` + `gateKind`（如 `subscription_candidate`），UI 不得只靠解析 reason 文案。`plan()` 是唯一规划出口；`plan.can_apply` = 矩阵开放 ∩ plan 私有 `write_gate`（有 bind 实现且 secret 可按 `source_kind` 解析；本步 Account 同边可写仅 Anthropic API → Pi）。模型映射预留（**未接线**）：`adapter_model_mapping.rs`。状态分层预留（**未接线**）：`adapter_state_model.rs`。 |

### 5.3 Codex → Claude Code 的目标数据流与语义

通过门禁后的单次请求流应为：

```text
Claude Code
  → Anthropic Messages + loopback bearer
  → agenthub-adapterd / DownstreamSurface
  → ProtocolKernel IR
  → approved Codex transport（由 SubscriptionSessionProvider 注入授权上下文）
  → ProtocolKernel IR
  → Anthropic SSE
  → Claude Code
```

映射必须由 fixtures 明确约束，而非“请求成功”即可开放：

| 语义 | 必须处理 |
|---|---|
| 请求与上下文 | system/developer 指令、消息与多轮历史、模型、token 上限、工具定义、工具结果、metadata；无等价项必须在发送前 fail-closed 或显式 limitation。 |
| 流式输出 | 将 Responses/Codex 事件归一到 IR，再按 Anthropic SSE 顺序发出 message start、text delta、tool-use delta、usage、message delta/stop 与 error；保持 Unicode 与 JSON 分片边界正确。 |
| 工具与 thinking | 工具 id/name/参数增量/结果须能闭环；thinking/reasoning 仅在两端有可验证等价语义时映射，不能伪造、解密或重建签名块。要验证不会同时让 Claude Code 与 Codex 作为独立 Agent 各执行一轮工具。 |
| 结束与错误 | 映射 stop reason、输入/输出/缓存用量、认证/限流/协议错误；客户端取消应立即取消上游并终止 SSE。 |

重试安全性是状态机的一部分：只有在**首个有效流事件前**的可判定瞬态失败可在严格次数和 `Retry-After` 约束下重试；一旦已经向 Claude Code 输出任何有效事件，禁止重放、换账号重试或重新执行工具回合。每个账户 refresh 必须 single-flight；账户失效应隔离并返回稳定错误，不把其余账户或 token 暴露给调用方。

## 6. 判定顺序

```text
选择票 + 目标 Agent
  → 票面（产品、凭据类、speaks）与 Agent（accepts、writer）
  → 票本来就是给这个 Agent？                         是：native
  → OAuth 且目标有同一授权契约槽？                   是：reshape（② 原生订阅，不起桥）
  → API Key 且 speaks ∩ accepts 非空？               是：reshape（① 直连，不起桥）
  → 图上是否有已测试的转换边？                       是：bridge（③）
  → 不可行，给出原因和替代路径
```

规则分析、计划与执行必须使用同一规则版本。`bridge` 只服务 ③。新增边不得只在 UI 绕过 `plan`。`plan.canApply=false` 时用户仍应看见原因。

对于 ③，流程在“是否有已测试的转换器”前还必须检查 capability matrix 的工程门禁（分类、secret、fixtures、回滚）；任一门禁缺失则不能 `bind`，但规划结果应对用户可见。opt-in 不能替代这些工程门禁。订阅先判 ②，不要一上来当 ③。

## 7. 新增或更新规则

每条规则至少记录：

- 来源产品、区域、凭据类型和显式识别字段；
- 上游协议、Base URL、必要 Endpoint 与官方资料；
- 目标 Agent、目标协议和适用版本；
- 路由、支持级别、限制与 `verified_at`；
- 分析、计划、写入和失败回滚测试；Bridge 还需协议 fixtures 与端到端测试。

出现以下变化时必须重新核对：厂商端点或认证方式变更、目标客户端协议升级、OAuth 刷新语义变化、规则代码或测试变化。未完成验证前保留原状态或降级为 `unsupported`，不能只更新文案日期。

### 7.1 Codex → Claude Code 的阶段、测试与验收门槛

| 阶段 | 交付与门禁 | `canApply` | 当前进度（2026-08-12） |
|---|---|---|---|
| 0. 证据与 fixtures | 固化身份分类样例、Messages/IR/Responses/SSE 正反例 fixtures；确认参考实现许可边界与选定 transport | `false` | **进行中**：`crates/agenthub-core/src/bridge/protocol/fixtures/` 已有 Messages / Responses / SSE 正反例；transport 选定证据仍未闭环 |
| 1. 纯协议内核 | 无网络、无 secret 的 Anthropic Messages ↔ IR ↔ Responses 转换及状态机测试 | `false` | **内核已落地、门禁仍关闭**：`IrEvent` / `RetryGate`、`parse_messages_request`、`to_responses_request`、`responses_output_to_ir`、`ResponsesStreamToIr`、`encode_anthropic_sse` 与协议单测已在 core；**不得**据此开放 Apply |
| 2. 认证 / transport spike | 分别验证 App Server 与 Responses（含参考项目已走通的本机反代通道）；验证 OAuth refresh、single-flight、取消、工具副作用与不泄露 secret | `false` | **未开始**（或未完成证据）；Account OAuth refresh 能力存在 ≠ 已选定上游 transport |
| 3. sidecar profile 与 Apply saga | 至少一条 transport 在协议/refresh/回滚上通过后，实现 loopback bearer、profile、core-owner IPC、目标配置写入和完整失败回滚；其他候选可明确淘汰 | 工程就绪后可为 `true` | **未开始**；控制面 IPC 与 Connections 领域仍见 sidecar 设计文档；**不得**跳过 phase 2 |
| 4. dogfood / experimental rollout | 当前用户、本机、显式 opt-in 的小范围验证与持续回归；发现上游/条款/语义漂移立即降级 | 受控且可撤销 | **未开始** |
| 以后 | 每个供应商/产品/目标组合重新取证 | 默认 `false` | 默认拒绝 |

测试矩阵至少覆盖：文本与多轮上下文、system/developer、tool definition / call / result / 并行调用、thinking 降级、usage、stop reason、上游错误、取消、SSE 分片与 Unicode；首事件前/后重试分界；refresh single-flight 与账户失效隔离；loopback bearer 拒绝缺失/错误 token；日志与 IPC 不含授权 JSON、access/refresh token、prompt、工具参数或响应正文；sidecar/GUI 进程和数据库所有权；端口冲突、配置 revision 冲突、启动后写入失败与写入后验证失败的逆序回滚。

验收要求是：Claude Code 只通过 `ANTHROPIC_BASE_URL`/`ANTHROPIC_AUTH_TOKEN` 访问 loopback；上游 token 从未写入 Claude 配置、日志或 IPC 响应；一轮文本流和至少一轮工具闭环没有双执行；输出后没有重放；任一失败不会遗留 active profile 或指向失效 listener 的 live 配置；所有门禁、fixture 和端到端 dogfood 证据可追溯到 capability matrix。

### 7.2 参考实现与许可证

`cc-switch`、`CLIProxyAPI` 与 Management Center 是**产品对齐对象**：① 官方兼容入口直连、② 订阅写进目标槽、③ 协议转换 / 订阅反代、管理面登录/配额/探测。它们也是协议与 refresh 的设计证据，但不是上游官方契约，也不能把对方源码贴进本仓库。不学 CLIProxyAPI「永远起代理」。`sub2api` 可作为完整 Anthropic ↔ Responses 状态机、首事件前重试、输出后禁止重放和账号失效隔离的测试参考；`AionUi` 仅可作为轻量非流式转换参考。

在复制或改编任何代码前必须单独审查许可证与边界：cc-switch 为 MIT、sub2api 为 LGPL、AionUi 为 Apache-2.0。优先重写协议实现与 fixtures；未经审查不得把参考项目代码混入本仓库。产品取舍见 [product-decisions.md](product-decisions.md)。

## 8. 官方资料

- [OpenAI Codex Authentication](https://developers.openai.com/codex/auth)
- [OpenAI Codex Configuration Reference](https://developers.openai.com/codex/config-reference)
- [OpenAI Codex App Server](https://developers.openai.com/codex/app-server/)
- [Anthropic Claude Code LLM gateway](https://docs.anthropic.com/en/docs/claude-code/llm-gateway)
- [Anthropic Claude Code getting started](https://docs.anthropic.com/en/docs/claude-code/getting-started)
- [Kimi Code 概览与 API Access](https://www.kimi.com/code/docs/en/)
- [Kimi Code 接入 Claude Code](https://www.kimi.com/code/docs/en/third-party-tools/claude-code.html)
- [Kimi Code 接入 Codex](https://www.kimi.com/code/docs/en/third-party-tools/codex.html)
- [Kimi Code CLI Providers](https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/providers.html)
- [Kimi 开放平台文档](https://platform.kimi.com/docs/overview)
- [GLM Coding Plan 一键安装助手](https://docs.bigmodel.cn/cn/coding-plan/extension/coding-tool-helper)
- [GLM Coding Plan 接入工具与双协议端点](https://docs.bigmodel.cn/cn/coding-plan/tool/others)
- [GLM Coding Plan 快速开始](https://docs.bigmodel.cn/cn/coding-plan/quick-start)
- [DeepSeek Anthropic API compatibility](https://api-docs.deepseek.com/guides/anthropic_api)
- [DeepSeek 接入 Claude Code](https://api-docs.deepseek.com/quick_start/agent_integrations/claude_code/)
- [DeepSeek Models & Pricing（双协议 Base URL）](https://api-docs.deepseek.com/quick_start/pricing/)
- [DeepSeek Harness 产品页](https://deepseek.com/harness/en/)
- [DeepSeek Harness 架构](https://deepseek-harness.github.io/deepseek-harness/en/reference/)
- AgentHub 侧 DSH 接入方案：[deepseek-harness-integration.md](deepseek-harness-integration.md)
- [Pi AI providers 与 OAuth](https://github.com/earendil-works/pi/blob/main/packages/ai/README.md)
- [Gemini API OpenAI compatibility](https://ai.google.dev/gemini-api/docs/openai)
