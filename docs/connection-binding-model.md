# 连接：票、绑定与协议图

> 状态：**§6 第 1–3 步已落地；§6.4 部分落地（OpenAI/xAI API → Pi reshape；Anthropic API Key → Codex 本地桥）；Grok 边仍不可行；§6.5 Claude bind 已开（GLM/DeepSeek → Claude experimental native_endpoint），Grok/订阅仍关；§6.6 未做**。  
> 日期：2026-08-15。  
> 本文是跨 Agent「把已有凭据接到另一个 Agent」的领域真源。页面、Hub 入口、Adapter、厂商规则文档以本文为准改表述；**当前实现状态**仍以 [agenthub-plan.md §8](agenthub-plan.md#8-当前实现状态以代码与测试为准) 和 [provider-api-oauth-adaptation.md §4](provider-api-oauth-adaptation.md#4-当前实现矩阵) 为准。  
> 关联：[architecture.md](architecture.md)、[ui-design.md](ui-design.md)、[adapter-design.md](adapter-design.md)、[hub-redesign-plan.md](hub-redesign-plan.md)、[provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md)、[account-authorization-pool.md](account-authorization-pool.md)、[adapter-sidecar-design.md](adapter-sidecar-design.md)。

## 0. 一句话

AgentHub 不「共享链接」，它**绑定票**。直连、改配置、本机桥都是同一种写入：`bind(票, Agent)`。扩大 = 认出更多票面、让更多 Agent 能被写入、给协议图补已验证的边。现有 Connections / Dashboard / ConnectFlow **可以按本文重做**，不必守着 Agent 分页和「只给白名单显示按钮」。

## 1. 用户任务

用户只做两件事：

1. **让某个 Agent 用起来**（人在 Agent 这边）
2. **让某张票被更多 Agent 用**（人在钱包这边）

「官方登录 / API Key / 中转 / 桥 / 投影 Provider / account 还是 provider」都是实现。用户不该先判断协议再决定去哪个页。

## 2. 领域对象

长期对外只保留三个对象。存储层继续用 accounts + providers 表可以，但 API、规划器和 UI 必须先看见下面这三个词。

### 2.1 票（Ticket）

一次授权，不是「属于某个 Agent 的配置行」。

| 字段 | 含义 |
|---|---|
| `id` | 稳定身份 |
| `surface` | 产品表面：Kimi 会员、Anthropic API、OpenAI Key、Codex 订阅…… |
| `credentialClass` | `api_key` / `oauth_refreshable` / …… |
| `speaks[]` | 这张票对**上游**能说的协议（Messages、Chat Completions、Responses……） |
| `secretRef` | 只引用，不复制，不写进目标 Agent 的 live |
| `importedFrom` | 仅审计：从哪个 Agent 导入。**不是能力** |

规则：

- 出身不决定能不能接到别的 Agent。从 Claude 导入的 Anthropic Key 和账号池里同一把 Key 是同一类票。
- 去重仍按 [账号池](account-authorization-pool.md)：同一张授权票合并，同人不同票并存。
- 未识别的标 `unknown`。路由失败原因是「未识别」，不是把入口藏掉。

### 2.2 Agent

一个客户端，不是凭据仓库。

| 字段 | 含义 |
|---|---|
| `id` | `claude` / `codex` / …… |
| `accepts[]` | 这个客户端听什么协议或配置槽 |
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
| `runtime?` | 仅 `bridge`：loopback、本地 bearer、桥进程。不是一张新票 |

硬规则：

1. **票不因被绑定而分裂。** 一把 Kimi 会员 Key 可以同时绑 Claude（reshape）和 Codex（bridge）。钱包里仍是一行。
2. **绑定可以很多，active 每个 Agent 只有一条。**
3. **投影不是票。** 桥写出来的 localhost Provider / profile 是绑定的私有运行时材料，默认不进钱包，更不能再当 `bind` 的来源（禁止二次投影）。

### 2.4 与现有存储的映射（过渡期）

| 目标对象 | 当前落点 | 过渡策略 |
|---|---|---|
| Ticket | `accounts` + `providers` 两行模型 | 先做只读聚合；进口打 `surface`；生成 Provider 从钱包剔除 |
| Binding | `is_current` + `AdapterProfile` + 生成 Provider | 先做读模型；再 `bind`/`unbind` 成为唯一写入 |
| 规划器 | `plan()` 唯一出口；内部矩阵 ∩ 私有 write_gate（有 bind 实现且 secret 可按 `source_kind` 解析） | `plan(ticket, agent)` 为唯一真理；Anthropic / OpenAI / xAI API Account → Pi 可写 |

Account / Provider / live 事务仍由 core service 单点负责，不建设 `connectionsd`。`local_bridge` 的 listener 仍按 [sidecar 契约](adapter-sidecar-design.md) 走用户级进程。

## 3. 路由是协议图，不是商品表

判定只问三件事：

```text
这张票能对上游说什么？
这个 Agent 听什么？
中间缺的那一跳，有没有转换器、目标能不能写配置？
```

### 3.1 四种路线（有优先级）

| `route` | 何时 | 现有实现名 | 用户感知 |
|---|---|---|---|
| `native` | 票本来就是给这个 Agent 的 | 账号/供应商切换 | 切换，不起桥 |
| `reshape` | 票说的和 Agent 听的是同一种协议，只是配置形状不同 | `config_sync` / `native_endpoint` | 写配置，凭据只引用 |
| `bridge` | 协议不同，图上有边 | `local_bridge` | 起本机 loopback，目标只持本地 token |
| 不可行 | 无 writer、无表面、无边、OAuth 不是 HTTP 上游 | `unsupported` | 说明原因和替代，不提供「强制转换」 |

优先直连/改配置，桥是兜底。OAuth 与 API Key 分开判：OAuth 只有能稳定当成某种上游协议时才进入图，否则只给签发它的那个 Agent 做 `native`。

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
- 不监听公网，不做号池换号，不把一张票拆成多人 Key
- refresh single-flight 发生在**票**这一层，所有绑定共享同一次刷新
- 流式：首字节前可换路线/重试，写出后禁止重放

参考实现（cc-switch、CLIProxyAPI、sub2api）只借鉴方法：协议成图、下游身份与上游 secret 分离、首字节边界、按账号 refresh。不借鉴产品：拼车、公网入口、把投影再当票。许可边界仍要单独审；优先重写，不混入参考项目源码。凭据落盘加密仍为项目范围外。

## 5. 界面（目标态，允许重做）

现有 Agent tab 钱包、行按钮白名单、Dashboard 诊断 / Connections 藏入口的拆法，**不是终态**。目标 UI 按绑定对齐，视觉体系仍走 [ui-design.md](ui-design.md) 的 token，不另起一套。

### 5.1 两个入口，一个对象

| 入口 | 用户问题 | 对话框 |
|---|---|---|
| Dashboard 卡片 | 这个 Agent 用哪张票？ | `bind`，target 固定，选票 |
| Connections 钱包 | 这张票再接到谁？ | `bind`，ticket 固定，选 Agent |

不再要求用户理解 Adapter。`/adapter` 只管理桥进程：端口、启停、自动恢复、失败详情。创建绑定不在本页。

### 5.2 Connections = 全局钱包

默认视图是**跨 Agent 的票列表**，不再以 Agent 分页为第一导航。

```
┌─ 连接 ─────────────────────────────────────────────────────┐
│ 钱包 · 5 张票                                               [+ 添加] │
│ 筛选 [全部] [官方登录] [API Key] [未识别]   搜票 / 搜用途          │
│                                                                   │
│ ● Kimi 会员          [API Key] [会员]                             │
│   正用于：Claude（改配置）· Codex（本机桥 · 运行中）               │
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
- 生成投影**不出现**在钱包。已有「经兼容路由」的用途记在源票的「正用于」上。
- 「切换」只用于该票与其 `importedFrom` / 原生 Agent 的 `native` 绑定。接到别的 Agent 一律叫「接到…」（文案可再打磨，语义是 bind）。
- 深链 `?agent=` 仍可用：打开钱包并高亮该 Agent 的 active 绑定，而不是把整个钱包切成该 Agent 的私有列表。
- 添加票：导入登录态、新 API Key。进口必须写下 `surface`。

### 5.3 Dashboard = Agent 的绑定

卡片展示：**当前绑定的票**（不是「当前 Provider 行」）、路线（直连 / 改配置 / 本机桥）、桥是否在跑。

- 主动作仍是打开同一套 bind 对话框（target 固定）。
- 来源不再按「本 Agent 表里的行 / 别人表里的行」分组，而按 **native 候选** 与 **可规划的其他票** 分组。
- 不可行票留在列表里，置灰 + 原因。
- 桥徽标点进「桥与适配」，管的是这条绑定的 runtime，不是再创建一条投影。

### 5.4 对话框

一个 `ConnectFlow`（可改名，不必叫 Adapter）：

1. 固定一端（票或 Agent），选另一端。
2. `plan()` 出路线、成熟度、将发生的写入。
3. 确认 → `bind()`。成功以「该 Agent 的 active 绑定」为准，不说「去钱包里再切一次生成供应商」。
4. 失败保留现场；busy 禁止关窗重提。

OAuth 未完成：引导去补登录，不在对话框里发起新授权。空钱包：引导添加票。资源加载失败：错误 + 重试，不得当成空池，也不得整页藏「接到…」。

### 5.5 桥与适配页

只列出 **`route=bridge` 且已 bind 的运行时**：来源票名称、目标 Agent、端口、健康、start/stop/retry。没有「选来源 → 分析 → apply」创建区。删绑定走 `unbind`，不要只删投影行而留下指向死端口的 live。

## 6. 扩大在本模型里怎么做

按需加三样，而不是加长商品白名单：

1. **识别更多票面**  
   导入时写下 `surface`（preset、官方 host、OAuth 形状）。存量用现有 classify 回填。GLM Coding Plan、DeepSeek、OpenAI Key、xAI Key 都是新 surface。
2. **声明更多 Agent 入口**  
   Grok 听 Chat、Pi 听槽位、Cursor 无 writer。新 Agent 先登记再长路由。
3. **给协议图加边**  
   已有 Messages↔Responses 内核、Chat→Responses 桥。缺边就补转换器 + fixtures。没有边就如实不可行。

建议顺序（工程，不是再讨论「能不能做」）：

1. 读模型：Ticket / Binding 聚合；钱包去掉生成投影；「接到…」对真票常驻。
2. 进口打标；规划器收口。
3. 现有四条可 apply 路径改写成 `bind` 实现（Kimi→Claude/Pi reshape，Kimi→Codex bridge，Anthropic→Pi reshape）。
4. 加边：Anthropic→Codex 桥（协议腿 + experimental bind 已开）、Kimi→Grok reshape、OpenAI/xAI Key → Pi/Grok。
5. 新 surface：GLM / DeepSeek 按双协议入口登记。
6. 新 Agent writer：DeepSeek Harness（`dsh`）已接入；**DeepSeek API → `dsh` `config_sync`** 与 **DeepSeek API → Claude experimental `native_endpoint`** 都走现有 `AdapterCapabilityMatrix` / `AdapterApplyService`。不要把 Harness 当协议桥。
7. 能当 HTTP 上游的订阅（如 Codex 订阅→Claude）接执行器后打开那条边。

做不到、且应看得见的上限：Cursor 当目标（无 writer）、未标记的自定义中转、不能当 HTTP 上游的官方登录、二次投影、公网号池。

## 7. 现状对照（防止倒读）

| 主题 | 当前代码 | 本文目标 |
|---|---|---|
| 用户对象 | account / provider 两行 | Ticket |
| 谁在用 | `is_current` + profile 反查 | Binding |
| 规划 | 前端走 `plan_ticket`；`plan()` 唯一出口；write_gate = 有 bind 实现 ∧ secret 可按 `source_kind` 解析 | `plan(ticket, agent)` 为唯一真理 |
| 写入 | `bind_ticket` / `unbind_ticket`；`apply_adapter` 薄委托 bind | `bind` / `unbind` |
| Connections | 全局钱包；真票都有「接到…」；写入是 bind/unbind | 全局钱包；真票都有「接到…」 |
| 诊断 | 同一对话框里置灰 + plan 原因 | 同一对话框里置灰 + 原因 |
| 生成物 | 不进钱包；记在源票「正用于」 | 绑定的私有 runtime |
| 扩大 | 加商品白名单 | 加 surface / writer / 图边 |
