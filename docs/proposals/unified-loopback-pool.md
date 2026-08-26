---
title: 本机同口授权池
type: proposal
status: proposed
owner: maintainers
updated: 2026-08-26
---

# 本机同口授权池

> 状态：提案
>
> 这是尚未承诺实施的候选方向，不是当前实现契约。不得据此宣称已经同口调度、已经按模型选授权，或把 Routes 页改写成网关管理大盘。

## 1. 当前基线

本机路由已经是单进程共享 Gateway：多条 `local_bridge` 在 `port=0` 时复用 `primary_port`，鉴权看本机 bearer，不看 TCP 端口。对话路径仍是 `/v1/messages`、`/v1/responses`、`/v1/chat/completions`；`GET /v1/models` 与 `/models` 按**当前鉴权命中的那条边**合成名单，不代理上游目录。

挡住「所有授权走同一端口、一份客户端配置、失败切换、按模型选号」的是语义，不是监听口：

1. **本机令牌 = 一条边 = 一份授权。** 写入 Codex / Grok / Claude 的 token 命中某一个 profile，不是命中该 Agent 的授权池。
2. **每个 Agent 同时只有一条 active binding。** 后一次 `bind` 会盖掉前一张票；客户端配置只能留下一份 token。
3. **已有两套半成品都不够覆盖跨产品池。**
   - `AccountPicker` / `RequestFsm` 只聚合同一 `TicketSurface + credentialClass` 的成员，且所有 local-bridge 边 `multi_account = false`。
   - `switch_edge_for_model` 只在 lead 映射 miss 时做请求级切边；`GET /models` 不合并多授权名单；上游 401 / 配额失败不会因此换号。

现行契约见 [Connections、Routes 与绑定](../concepts/connections-and-routing.md)、[本机 Routes API](../reference/local-route-api.md) 和 [Route 兼容性](../reference/route-compatibility.md)。同类多号的历史设计见归档 [multi-account-routing-rfc.md](../archive/multi-account-routing-rfc.md)；它只覆盖同票面轮询，不是本文的跨产品同口池。

## 2. 候选目标

下游客户端只认一个 loopback 入口：

```text
http://127.0.0.1:<固定端口>
Authorization: Bearer <本机 Hub 令牌>
```

路径仍按客户端方言分：

| 客户端 | 路径 | 上游候选 |
|---|---|---|
| Claude | `/v1/messages` | 钱包里 `plan(票, claude)` 且 `route = local_bridge` 的授权 |
| Codex / Grok | `/v1/responses` | `plan(票, 该 Agent)` 且可写 local-bridge 的授权 |
| Kimi / DSH | `/v1/chat/completions` | 同上 |

`GET /v1/models` 返回该 surface 上当前可服务的模型并集，而不是某一张票的映射表。一份授权在写出下游第一个字节之前因 401、配额 429 或冷却不可用时，换下一份合格成员；流已经开始则不再切换。

登录仍由 Connections 管理；Hub 持有上游凭据。客户端配置里只有本机口和本机令牌。

## 3. 从 Sub2API 借鉴什么

[Sub2API](https://github.com/Wei-Shaw/sub2api) 的有用部分是调度，不是产品形态。它对外暴露一把 `sk-xxx` 和一个 OpenAI 兼容口，后面池化多种凭证。AgentHub 只借调度，不借公网网关。

| 借鉴 | 明确不借鉴 |
|---|---|
| 一个入口、多上游 | 公网监听、CORS、拼车、计费、多租户 key |
| 模型名单并集，按 model 过滤渠道 | 权重、least-conn、按余额/压力分配 |
| 失败冷却；配额 429 整号暂停 | 把协议 400 一律当 failover |
| 流式首 token 前才能切号 | 切号后复用上一轮解析过的请求体 |
| 成员 isolate / restore | 把订阅 OAuth 伪装成可转售的 API Key |

产品边界不变：仅本人登录、仅 `127.0.0.1`、`plan()` 门禁、不为国产 OAuth 开边或转 API、不讨论凭据落盘加密。见 [产品边界](../decisions/product-boundaries.md)。

## 4. 候选领域模型

把「边」拆成入口与上游两层，避免 profile 同时当客户端配置又当一份授权。

```text
HubGateway
  设置中钉死的 loopback 端口
  一个或按 Agent 各一个本机 Hub 令牌
  三个 DownstreamSurface

SurfacePool（每个目标 Agent / surface 一个）
  成员 = plan(票, 该 Agent) 可写 local_bridge 的授权
  每成员：transport、listed_models、健康、冷却截止

Ticket（Connections）
  只表示真实授权，不等于下游配置
```

候选请求路径：

```text
本机 Hub 令牌鉴权
  → 路径定 surface（先鉴权；错 path 再 404）
  → GET /models：并集（去掉 leftover 与冷却中的号）
  → 对话：按 model 过滤能服务的成员
       → 固定顺序里第一个健康成员（可加会话粘性）
       → 按该成员的 UpstreamTransport 重算 body（禁止复用上一轮）
       → 尚未写出下游字节且失败：isolate / 冷却，换下一个
       → 已写出：结束，不再切
```

选成员的优先级建议固定为：

1. 该成员能否映射这个 model（映射表 + listed / passthrough）。
2. 健康：可调度，且不在冷却。
3. 粘性：Codex conversation / Grok session seed 尽量不换号。
4. 否则按票 id 固定序。不做负载均衡。

失败分类：

| 上游结果 | 候选行为 |
|---|---|
| OAuth 401 | 先同号 refresh，再 isolate 切号 |
| 配额 / 窗口 429 | 整号冷却到 reset，切号 |
| 网络 / 5xx / 空 200 | 短冷却，切号 |
| 协议 400（字段、角色、方言） | 不切号，避免 Responses 方言误打 |
| 流已发出 | 禁止切号（沿用现有 `EmissionState`） |

`bind(票, Agent)` 的候选语义：

- 将该票加入该 Agent 的 SurfacePool（enrollment）。
- 若这是该 Agent 第一次入池：写客户端配置指向 Hub 口 + Hub 令牌。
- 不再把客户端改成「只认这一张票」。

`unbind` 是退池；池空了再清客户端投影。

`native_endpoint` / `config_sync` 默认不进池。只有用户明确「这张官方登录也交给本机网关」时，才收成池成员。不得静默劫持官方直连。

## 5. 评估切片

这些是可单独验收的切片，不是已批准的排期。每一刀都应能单独发布；下游客户端地址尽量只改一次。

### P0 钉死同口

固定 Gateway 端口（设置项），启动始终绑定该口，禁止换口后再回写客户端。不同登录的 local-bridge 也收敛到这一口。客户端仍可暂时各写各的 token。

验收：多条运行中路由的 `status.port` 相同；重启不改客户端 `base_url` 端口。

### P1 每个 Agent 一把 Hub 令牌

鉴权从「token → 单个 profile」改为「Hub 令牌 → SurfacePool」。池里暂时仍只有当前 lead 票，行为与今天等价。换票 bind 不改客户端 base_url/token，只换池成员。

令牌建议按 Agent 分开（一把泄漏不会打到另一客户端的池）；三表面共用一把也可以，用 path 区分，需在开做前拍板。

验收：两次 bind 不同票到同一 Codex，客户端配置中的口和 token 不变。

### P2 打开同类多号

把已有 `AccountPicker`、`RequestFsm`、`resolve_pool_members` 接到生产：`MemberHealthSink` 回写账号行；启动时把 `Unknown` / `NeedsAttention` 映射为 `TryOnce`；Routes 详情重新展示成员健康。只对已取证的边打开 `multi_account`（建议先同类 API Key，再同类订阅）。

验收：两份同票面授权，A 在首字节前 401 后由 B 承接；`GET /models` 先做同类并集。

### P3 跨产品入池并按模型选号

成员资格改为：

```text
eligible(ticket, agent) =
  plan(ticket, agent).route == local_bridge
  && plan.canApply
  && 该边 transport 已落地
```

同一 Codex 客户端后面可以同时有 ChatGPT 订阅、Grok 订阅、OpenRouter Key、Anthropic Key。必须新做：按模型过滤再 pick；`/models` 并集并剔除冷却中的号；每次尝试按该成员 Transport 重算 body（Codex `store:false` 白名单 vs Grok CLI 头 / tools / cache）；配额 429 冷却；Codex 会话粘性。矩阵继续 fail-closed。跨产品 failover 按边取证后才开。

验收：lead 映射不到的 model 打到能服务的成员；Grok 订阅 401 且尚未写出时，可落到另一份合格 Responses 成员；协议 400 不误切。

### P4 产品面

Routes 展示一个口、三个 surface、成员表、每号能提供的模型和冷却原因。Connections 继续只管登录；「用于某 Agent」表示入池，不是独占。可选：固定序可调、OpenRouter 标成 backup。不做权重、公网、计费。

## 6. 开做前要拍板的点

1. **官方直连要不要进池。** 不进则官方 Codex 仍写原生槽，Hub 看不到它的失败。进则「接到该 Agent」一律写 loopback。建议默认不劫持，路由页提供显式「交给本机网关」。
2. **Grok 与 Codex 是否共用一把 Hub 令牌。** 同口已经足够；按 Agent 分令牌更贴近现有 writer。
3. **池成员要不要可裁剪。** 历史 RFC 选「同票面自动全进」。跨产品后应改为显式 enrollment（bind = 入池）。
4. **`/models` 空名单。** 并集为空时 fail-closed，不要静默把任意 model 打到未知上游。

## 7. 门槛与非目标

在 P1 的 Hub 令牌语义、P3 的按成员重算 body、以及至少一条跨产品边的实机取证完成前，不得把本文写进现行 Routes 说明或 CLI 帮助。

非目标：

- 只把现有边上的 `multi_account` 设为 `true`，当作本提案已经落地。
- 给每张票继续发不同本机 token，指望客户端自己换钥匙。
- 做成对外 `sk-xxx` 公网网关或拼车服务。
- 在页面层做调度。调度必须留在 `bridge` host，资格仍由 `plan()` 决定。
- 权重、least-conn、余额调度、公网监听、计费、多租户。
- sidecar 进程迁移（见 [Local Route Sidecar](adapter-sidecar.md)）；本提案不改变 runtime 进程边界。
- 凭据落盘加密；国产 OAuth 开边或 OAuth 转 API。

## 8. 与历史记录的关系

| 记录 | 关系 |
|---|---|
| [a4-unified-loopback-gateway.md](../archive/a4-unified-loopback-gateway.md) | 已落地的同口 Gateway；本提案建立在它上面，不重做 listener |
| [multi-account-routing-rfc.md](../archive/multi-account-routing-rfc.md) | 同类多号内核；对应本提案 P2，不是 P3 跨产品池 |
| [routing-connection-refactor-plan.md](../archive/routing-connection-refactor-plan.md) | 历史泳道拆分；其中 C2 仍部分未闭环，不得当作现行待办 |

## 相关页面

- [Connections、Routes 与绑定](../concepts/connections-and-routing.md)
- [Adapters 与本机 Bridge](../concepts/adapters-and-bridges.md)
- [本机 Routes API](../reference/local-route-api.md)
- [Route 兼容性](../reference/route-compatibility.md)
- [产品边界](../decisions/product-boundaries.md)
- [Local Route Sidecar](adapter-sidecar.md)
- [提案索引](README.md)
