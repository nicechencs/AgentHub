# 连接：票、绑定与协议图

> **现行状态（2026-08-19）：** 用户看到的是「登录」，不是「票 / 钱包」。票 / Ticket / 钱包是实现名。Grok→Claude 走本机路由；自动生成的配置不出现在登录列表；sidecar 未迁。预览芯片是「直连 / 用这份登录 / 本机路由 / 当前不支持」，不再标圈号。
> 状态：**§6 第 1–3 步已落地；§6.4 部分落地（Kimi/OpenAI API → Grok、OpenAI/xAI/GLM/DeepSeek API → Pi 属直接改配置；GLM/DeepSeek API → Codex 属直接改配置；Anthropic API Key → Codex 属本机转发）；§6.5 Claude/Codex bind 已开（GLM/DeepSeek → Claude/Codex 属直接改配置），GLM/DeepSeek → Pi 已可 experimental bind，写进对方认的登录——Claude/Codex/Grok 订阅 → Pi 已可 experimental bind，本机转发——Codex Responses 与 Grok Responses 订阅 → Claude / Codex 已可 experimental bind，Codex 订阅 → Grok 写 `api_backend=responses`；Claude 订阅 → Codex 产品不做，App Server/OauthOther 仍关闭；dsh writer 已接入（`AgentId::Dsh` + `deepseek-api-to-dsh-v1`）。未做的是 sidecar 迁移**。
> 日期：2026-08-15。  
> 本文是实现用的领域模型，不是给最终用户看的说明书。读者向说明（三种接法、白话图）见 [product-decisions.md](product-decisions.md)。页面、Hub 入口、Adapter、厂商规则文档以本文为准改对象名；**当前实现状态**仍以 [agenthub-plan.md §8](agenthub-plan.md#8-当前实现状态以代码与测试为准) 和 [provider-api-oauth-adaptation.md §4](provider-api-oauth-adaptation.md#4-当前实现矩阵) 为准。  
> 关联：[product-decisions.md](product-decisions.md)、[architecture.md](architecture.md)、[ui-design.md](ui-design.md)、[adapter-design.md](adapter-design.md)、[hub-redesign-plan.md](hub-redesign-plan.md)、[provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md)、[account-authorization-pool.md](account-authorization-pool.md)、[adapter-sidecar-design.md](adapter-sidecar-design.md)。

日常说法对照（本文仍用左边，方便和代码对齐）：

| 本文用词 | 日常说法 |
|---|---|
| 票 / Ticket | 一份登录（一把 Key 或一次订阅） |
| Agent | 编程工具 |
| 槽 / Slot | 对方认的那一处配置 / 登录 |
| 边 / Edge | 某一份登录接到某一个工具的做法 |
| ① reshape / `native_endpoint` | 直接改配置 |
| ② reshape / `config_sync` | 写进对方认的登录 |
| ③ `bridge` / `local_bridge` | 本机转发 |

## 0. 一句话

AgentHub 不「共享链接」，它把**一份登录**接到一个编程工具。直接改配置、写进对方认的登录、本机转发，对实现都是同一种写入：`bind(票, Agent)`。扩大 = 认出更多登录、让更多工具能被写入、给已测过的转换补上做法。现有 Connections / Dashboard / ConnectFlow **可以按本文重做**，不必守着按工具分页和「只给白名单显示按钮」。

## 1. 用户任务

用户只做两件事：

1. **让某个 Agent 用起来**（人在 Agent 这边）
2. **让某份登录被更多工具用**（人在登录列表这边）

「官方登录 / API Key / 中转 / 本机路由 / 自动生成的配置 / account 还是 provider」都是实现。用户不该先判断协议再决定去哪个页。

## 2. 领域对象

长期对外只保留三个对象。存储层继续用 accounts + providers 表可以，但 API、规划器和 UI 必须先看见下面这三个词。

### 2.1 票（Ticket）

一次授权，不是「属于某个 Agent 的配置行」。

| 字段 | 含义 |
|---|---|
| `id` | `account:<id>` / `provider:<id>` |
| `sourceKind` | `account` / `provider` |
| `sourceId` | 底层行 id |
| `agentId` | 出身 Agent（导入时的归属，不是能力） |
| `label` | 展示名 |
| `surface` | 产品表面：Kimi 会员、Anthropic API、OpenAI Key、Codex 订阅…… |
| `credentialClass` | 仅 `api_key` / `oauth` / `unknown` |
| `speaks[]` | 这张票对**上游**能说的协议（Messages、Chat Completions、Responses……） |
| `importedFrom` | 仅审计：从哪个 Agent 导入。**不是能力** |

登录列表 DTO **没有** `secretRef`：密钥经 `AdapterSecretResolver` 按 `source_kind` 解析，不进票对象。自动生成的配置不是票：`PROJECTION_NOT_A_TICKET`（「自动生成的配置不是登录 / 禁止再当登录」），不能再当 `bind` 来源。

规则：

- 出身不决定能不能接到别的 Agent。从 Claude 导入的 Anthropic Key 和账号池里同一把 Key 是同一类票。
- 去重仍按 [账号池](account-authorization-pool.md)：同一张授权票合并，同人不同票并存。
- 未识别的标 `unknown`。路由失败原因是「未识别」，不是把入口藏掉。

### 2.2 Agent

一个客户端，不是凭据仓库。

| 字段 | 含义 |
|---|---|
| `id` | `claude` / `codex` / …… |
| `accepts[]` | 这个客户端听什么 **wire 协议** 和 **OAuth 契约槽**（两者都要登记，否则判不出第 2 路） |
| `writer` | 能否写 live。Cursor 当前为无，不能当绑定落点 |
| `constraints` | 例如官方登录不能被桥冒充成另一家授权 |

新增 Agent 时先登记 `accepts` + `writer`，路由从图上长出来，而不是再开一张商品白名单。步骤补记在 [adding-an-agent.md](adding-an-agent.md)。

### 2.3 绑定（Binding）

「这张票，此刻被这个 Agent 以哪种路线使用」。

| 字段 | 含义 |
|---|---|
| `ticketId` | 指向用户导入的票 |
| `agentId` | 目标 Agent |
| `route` | `native` / `reshape` / `bridge` |
| `active` | 该 Agent 当前是否用它。每个 Agent 同时只有一条 active |
| `profileId` | 可选；对应 adapter profile |
| `bridge` | 仅 `route=bridge`：loopback 端口 / 是否在跑。不是一张新票 |

硬规则：

1. **票不因被绑定而分裂。** 一把 Kimi 会员 Provider 或 Account Key 可以同时绑 Claude（reshape）和 Codex（bridge）。登录列表里仍是一行。
2. **绑定可以很多，active 每个 Agent 只有一条。**
3. **自动生成的配置不是票。** 桥写出来的 localhost Provider / profile 是绑定的私有运行时材料，不出现在登录列表，更不能再当 `bind` 的来源（禁止二次当作登录）。

### 2.4 与现有存储的映射（过渡期）

| 目标对象 | 当前落点 | 过渡策略 |
|---|---|---|
| Ticket | `accounts` + `providers` 两行模型 | 先做只读聚合；进口打 `surface`；生成 Provider 从登录列表剔除 |
| Binding（`TicketBinding`） | `is_current` + `AdapterProfile` + 生成 Provider | 先做读模型；再 `bind`/`unbind` 成为唯一写入 |
| ActiveBinding（勿简称 Binding） | `ConnectionService` 的 Agent 当前行指针 | 与产品 Binding 同词不同物；改 current 不得误伤登录绑定 |
| 规划器 | `plan()` 唯一出口；内部矩阵 ∩ 私有 write_gate（有 bind 实现且 secret 可按 `source_kind` 解析） | `plan(ticket, agent)` 为唯一真理；Anthropic / OpenAI / xAI API Account → Pi 可写 |

Account / Provider / live 事务仍由 core service 单点负责，不建设 `connectionsd`。`local_bridge` 的目标为用户级 sidecar（见 [sidecar 契约](adapter-sidecar-design.md)）；当前仍由 Tauri `AppState` / `BridgeRuntimeHost` 进程内托管。

## 3. 路由是协议图，不是商品表

判定只问三件事：

```text
这张票能对上游说什么？
这个 Agent 听什么？
中间缺的那一跳，有没有转换器、目标能不能写配置？
```

### 3.1 四种路线（有优先级）

| `route` | 何时 | 现有实现名 | 用户三路 | 用户感知 |
|---|---|---|---|---|
| `native` | 票本来就是给这个 Agent 的 | 账号/供应商切换 | 写进对方认的登录（本 Agent） | 切换，不起桥 |
| `reshape` | 共同协议或共同 OAuth 契约槽，只改配置形状 | `config_sync` / `native_endpoint` | 直接改配置 或 写进对方认的登录 | 写配置，凭据只引用，不起桥 |
| `bridge` | 协议/契约对不上，图上有边 | `local_bridge` | 本机转发 / 本机路由 | 起 loopback，目标只持本地 token |
| 不可行 | 无 writer、无表面、无边、登录态不能当 HTTP 上游 | `unsupported` | —— | 说明原因和替代，不提供「强制转换」 |

优先直接改配置和写进对方认的登录，对不上再本机转发。本机路由只是第 3 路的手段，不是订阅的默认。OAuth 先看目标有没有同一授权契约槽（写进对方认的登录）；没有且存在转换边才进本机转发。API Key 与 OAuth 分开判。三路是用户说明，不新增领域枚举。见 [product-decisions.md](product-decisions.md)。

`plan.reusePath` 是派生展示字段，非第五个 route；领域 route 仍是 `native` | `reshape` | `bridge` | 不可行。

### 3.2 协议图

边是 `(上游协议 → 下游协议)` + 转换器 + fixtures + 成熟度。商品组合（Kimi 会员 × Claude）是图的一次求值结果，不是规则本身。

| 成熟度 | 用户看见 | 能否 `bind` |
|---|---|---|
| 稳定 | 可选 | 能 |
| 实验 | 可选，标明要起本机服务 / 语义可能损 | 能，预览写清楚 |
| 可预览 | 看得到，置灰或需确认 | 默认不能写 |
| 无边 | 看得到原因 | 不能 |

`plan.canApply` **只**表示「这条绑定现在能写上去」。不要用它表示「用户不准问起这件事」。

厂商端点、凭据类型、某条边开没开，仍以 [provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md) 为规则真源。该文档维护的是图上的边，不是 UI 白名单。

### 3.3 规划器

```text
plan(ticket, agent) →
  route | 不可行
  成熟度
  将写入的 live / 是否起桥 / 端口与模型
  原因原文（不可行时）
```

废掉「矩阵一格 ∩ `implemented_apply_whitelist` ∩ 前端 `reuse-offer`」三套真理。实现进度用成熟度表达，不靠把按钮藏起来。

## 4. 唯一写入：bind / unbind

```text
bind(ticket, agent) → Binding
unbind(binding)     → 停桥、恢复该 Agent 上一份 live、票还在
```

内部 saga 仍是：备份 live →（如需）起桥 → 写目标配置（只含本地引用）→ 记下绑定 → 失败逆序回滚。安全切换、锁、备份的 owner 不变。

运行时：

- `reshape` / `native`：不常驻进程
- `bridge`：只听 loopback；目标只持短寿命本地 bearer；上游 secret 留在 Hub / sidecar
- 不监听公网，不多账号轮换，不把一张票拆成多人 Key
- refresh single-flight 发生在**票**这一层，所有绑定共享同一次刷新
- 流式：首字节前可换路线/重试，写出后禁止重放

产品能力：三路复用、协议成图、下游身份与上游 secret 分离、首字节边界、按账号 refresh、管理面的登录/配额/探测。本产品不做公网入口、多账号拼车、默认常驻代理，也不把本机转发自动生成的配置再当作登录列表里的登录。凭据落盘加密仍为项目范围外。产品真源：[product-decisions.md](product-decisions.md)。

## 5. 界面（目标态，允许重做）

现有按 Agent 分页的登录列表、行按钮白名单、Dashboard 诊断 / Connections 藏入口的拆法，**不是终态**。目标 UI 按绑定对齐，视觉体系仍走 [ui-design.md](ui-design.md) 的 token，不另起一套。

### 5.1 两个入口，一个对象

| 入口 | 用户问题 | 对话框 |
|---|---|---|
| Dashboard 卡片 | 这个工具用哪份登录？ | `bind`，target 固定，选登录 |
| Connections 登录列表 | 这份登录再接到谁？ | `bind`，ticket 固定，选 Agent |

不再要求用户理解 Adapter。`/routes` 只管理本机路由运行时：端口、启停、自动恢复、失败详情、解绑。创建绑定不在本页。`/adapter`、`/router`、`/bridges` 永久跳过来。

### 5.2 Connections = 全局登录列表

默认视图是**跨工具的登录列表**，不再以 Agent 分页为第一导航。

```
┌─ 连接 ─────────────────────────────────────────────────────┐
│ 登录列表 · 5 份登录                                         [+ 添加] │
│ 筛选 [全部] [官方登录] [API Key] [未识别]   搜登录 / 搜用途        │
│                                                                   │
│ ● Kimi 会员          [API Key] [会员]                             │
│   正用于：Claude（改配置）· Codex（本机路由 · 运行中）               │
│   [接到…] [详情]                                                  │
│                                                                   │
│ ○ Anthropic Key      [API Key] [官方]                             │
│   正用于：Pi（改配置）                                            │
│   [接到…] [设为某 Agent 当前] [详情]                              │
│                                                                   │
│ ○ me@…               [官方登录] [Claude]                          │
│   正用于：Claude（切换）                                          │
│   [接到…]  → 打开后不可行的目标置灰 + 原因                        │
└─────────────────────────────────────────────────────────────┘
```

规则：

- 每一张**真票**都有「接到…」。未识别、无边、目标无 writer，都在对话框里置灰 + 原因，不在列表上假装这件事不存在。
- 自动生成的配置**不出现**在登录列表。已有用途记在源登录的「正用于」上，并标现行芯片 **直连 / 用这份登录 / 本机路由**（三种做法仍是直接改配置 / 写进对方认的登录 / 本机转发；过渡期「经兼容路由」只表示改配置，不要当成转发）。
- 「切换」只用于该票与其 `importedFrom` / 原生 Agent 的 `native` 绑定。接到别的 Agent 一律叫「接到…」（文案可再打磨，语义是 bind）。
- 深链 `?agent=` 仍可用：打开登录列表并高亮该 Agent 的 active 绑定，而不是把整个登录列表切成该 Agent 的私有列表。
- 添加票：导入登录态、新 API Key。进口必须写下 `surface`。

### 5.3 Dashboard = Agent 的绑定

卡片展示：**当前绑定的登录**（不是「当前 Provider 行」）、路线芯片「直连 / 用这份登录 / 本机路由」、仅本机转发显示路由是否在跑。

- 主动作仍是打开同一套 bind 对话框（target 固定）。
- 来源不再按「本 Agent 表里的行 / 别人表里的行」分组，而按 **native 候选** 与 **可规划的其他登录** 分组。
- 接不上的登录留在列表里，置灰 + 原因。
- 路由徽标**只服务当前生效的本机转发**：点进 `/routes?profile=`（无 id 则 `/routes`），tip「管理本机路由」。管的是这条绑定的 runtime，不是再创建一条自动生成的配置，也不是孤立回收通道。孤立 / 非当前本机路由没有徽标。

### 5.4 对话框

一个 `ConnectFlow`（可改名，不必叫 Adapter）：

1. 固定一端（登录或 Agent），选另一端。
2. `plan()` 出路线、成熟度、将发生的写入。
3. 确认 → `bind()`。成功以「该 Agent 的 active 绑定」为准，不说「去登录列表里再切一次自动生成的配置」。
4. 失败保留现场；busy 禁止关窗重提。

OAuth 未完成：引导去补登录，不在对话框里发起新授权。空登录列表：引导添加登录。资源加载失败：错误 + 重试，不得当成空池，也不得整页藏「接到…」。

### 5.5 Routes（本机路由）

规范路由 `/routes`。侧栏英文 Routes，有本机路由才出现；Settings → 本机永远有「本机路由」入口。

列出全部 `route=local_bridge`（`partitionLocalBridgeRuntimes`）：来源仍在或 last-known binding 命中的进主列表；其余非空 `sourceId` 进孤立分区。行与详情都是**单层**进程健康 + 端口，不画「配置已生效 / 桥接运行中」。来源/目标是纯文字，**禁止**链到 `/connections?agent=`（自动生成的配置不得出现在登录列表）。

没有「选来源 → 分析 → apply」创建区。解绑只走 unbind，不要只删自动生成的配置行而留下指向死端口的 live，也不走 `removeAdapter`。Connections「本机路由」点进对应 runtime（`/routes?profile=`）。

## 6. 扩大在本模型里怎么做

按需加三样，而不是加长商品白名单：

1. **识别更多票面**  
   导入时写下 `surface`（preset、官方 host、OAuth 形状）。存量用现有 classify 回填。GLM Coding Plan、DeepSeek、OpenAI Key、xAI Key 都是新 surface。
2. **声明更多 Agent 入口**  
   Grok 听 Chat、Pi 听槽位、Cursor 无 writer。新 Agent 先登记再长路由。
3. **给协议图加边**  
   已有 Messages↔Responses 内核、Chat→Responses 桥。缺边就补转换器 + fixtures。没有边就如实不可行。Grok→Claude 本机路由的生成 Provider 是 `adapterSecretMode=local_token` + `grok-subscription-to-claude-v1`，与 Codex→Claude 一样按已知 local-token 桥放行。

建议顺序（工程，不是再讨论「能不能做」）：

1. 读模型：Ticket / Binding 聚合；登录列表去掉自动生成的配置；「接到…」对真登录常驻。
2. 进口打标；规划器收口。
3. 现有四条可 apply 路径改写成 `bind` 实现（Kimi→Claude/Pi reshape，Kimi→Codex bridge，Anthropic→Pi reshape）。
4. 加边：Anthropic→Codex 桥（协议腿 + experimental bind 已开）、Kimi/OpenAI API → Grok native、Grok 订阅 → Claude / Codex Responses 本机路由、Codex 订阅 → Grok `api_backend=responses`、OpenAI/xAI Key → Pi。
5. 新 surface：GLM / DeepSeek 按双协议入口登记。
6. 新 Agent writer：DeepSeek Harness（`dsh`）已接入；**DeepSeek API → `dsh` `config_sync`**、**DeepSeek API → Claude experimental `native_endpoint`** 与 **DeepSeek API → Codex experimental `native_endpoint`** 都走现有 `AdapterCapabilityMatrix` / `AdapterApplyService`。不要把 Harness 当本机转发。
7. **跨 Agent 复用三路**（产品已定，见 [product-decisions.md](product-decisions.md)）：直接改配置——Kimi/OpenAI API → Grok 已写官方 Chat TOML；写进对方认的登录——Claude / Codex / Grok 订阅 → Pi 已写契约槽；本机转发——Codex Responses 与 Grok Responses 订阅 → Claude / Codex 的本机路由已可 experimental bind，Codex 订阅 → Grok 写 `api_backend=responses`，App Server/OauthOther 仍关闭；Claude 订阅 → Codex 是产品关闭，不是待评估候选。

做不到、且应看得见的上限：Cursor 当目标（无 writer）、未标记的自定义中转、把自动生成的配置再当登录、公网多账号共享。暂时不能当 HTTP 上游的登录态要写明缺哪一跳，不能写成「订阅一律不做」。

## 7. 现状对照（防止倒读）

| 主题 | 当前代码 | 本文目标 |
|---|---|---|
| 用户对象 | account / provider 两行 | Ticket |
| 谁在用 | `is_current` + profile 反查 | Binding |
| 规划 | 前端走 `plan_ticket`；`plan()` 唯一出口；write_gate = 有 bind 实现 ∧ secret 可按 `source_kind` 解析 | `plan(ticket, agent)` 为唯一真理 |
| 写入 | `bind_ticket` / `unbind_ticket`；`apply_adapter` 薄委托 bind | `bind` / `unbind` |
| Connections | 全局登录列表；真登录都有「接到…」；写入是 bind/unbind | 全局登录列表；真登录都有「接到…」 |
| 诊断 | 同一对话框里置灰 + plan 原因 | 同一对话框里置灰 + 原因 |
| 生成物 | 不进登录列表；记在源登录「正用于」 | 绑定的私有 runtime |
| 扩大 | 加商品白名单 | 加 surface / writer / 图边 |
