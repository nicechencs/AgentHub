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

## 3. 设计原则与 Sub2API 取舍

[Sub2API](https://github.com/Wei-Shaw/sub2api) 的有用部分是调度分层，不是产品形态。AgentHub 只借鉴「公开入口、模型路由、账号候选、失败切换」四层，不借公网网关、多租户和计费系统。

必须保持以下不变量：

1. 本地公开路由与上游授权生命周期分离；增删授权不改变客户端端口和本地令牌。
2. `plan()` 仍是授权能否加入目标 Agent 的唯一产品门禁；模型规则不得绕过 capability matrix。
3. 先解析 endpoint、model 和协议方言，再从解析结果给出的候选中选择授权。
4. `/models`、配置预览和实际 dispatch 共用同一个 resolver、同一代索引。
5. Codex 与 Grok 即使都使用 Responses，也不能根据 URL、路径或模型名前缀猜测 Provider。
6. 未知、冲突和歧义模型 fail-closed；不默认做任意跨 Provider 智能路由。

| 借鉴 | 明确不照搬 |
|---|---|
| 一个入口、显式模型规则、后端授权池 | 公网监听、CORS、拼车、计费、多租户 key |
| 先选 Provider lane，再选合格账号 | 根据模型前缀猜 Provider |
| 账号级模型与 endpoint eligibility | 管理面和运行时各维护一套路由真相 |
| sticky、候选排除、首输出前 bounded failover | 任意 Provider 自动 fallback |
| 401、403/404、429 的不同错误粒度 | 把所有 400、403 或 429 都当成整号失败 |
| 成员 isolate / restore | 每次 `/models` 同步查询所有上游 |

[Composite Groups](https://github.com/Wei-Shaw/sub2api/blob/main/docs/COMPOSITE_GROUPS.md) 可参考显式 `public_model → provider/upstream_model/endpoint` 的解析顺序；[OpenAI scheduling](https://github.com/Wei-Shaw/sub2api/blob/main/backend/internal/service/openai_gateway_scheduling.go) 可参考候选过滤和粘性。公开 [issue #5862](https://github.com/Wei-Shaw/sub2api/issues/5862) 也说明：管理面保存了模型路由，并不代表 scheduler 一定使用了同一规则。因此「共享 resolver」必须是架构契约，而不是实现约定。

产品边界不变：仅本人登录、仅 `127.0.0.1`、`plan()` 门禁、不为国产 OAuth 开边或转 API、不讨论凭据落盘加密。见 [产品边界](../decisions/product-boundaries.md)。

## 4. 候选领域模型

产品仍可称「SurfacePool」，内核统一使用 `RoutePool`。默认每个目标 Agent / surface 建一个池，以保持产品简单；领域模型不把它锁死成一对一，后续可以为同一 Agent 建立不同策略、不同 bearer 的多个池。

```text
HubGateway
  固定 loopback 端口
  承载多个 RoutePool

RoutePool（产品投影为 SurfacePool）
  稳定 route id、本机 bearer、target agent、downstream surface/dialect
  listener 生命周期、调度策略、policy revision

RouteMember
  引用一份 Ticket / Account / Provider 授权
  enabled、priority、position、成员状态

ModelRouteRule
  public_model + endpoint
    → upstream provider/dialect + upstream_model

MemberCapabilitySnapshot
  member + upstream_model + endpoint
    → supported / unsupported / unknown + evidence + expiry

EffectiveRouteIndex
  (route, endpoint, public_model)
    → DispatchCandidate[]
```

`Ticket` 仍只表示真实授权，不等于下游配置。`RouteMember` 只保存授权引用，不复制 token；运行时再解析凭据、Provider、endpoint 和授权指纹。同一池内按授权指纹去重，不能让两个成员实际引用同一授权。

### 4.1 `RoutePool`

`RoutePool` 是公开入口的稳定根，负责：

- route id、target agent、surface 和 downstream dialect；
- 本地 bearer 与固定端口投影；
- listener、auto-start 和 lifecycle；
- `priority_failover` 或 `round_robin` 调度策略；
- 单调递增的 policy revision。

现有 `AdapterProfile` 在迁移期作为 `RoutePool` 的兼容投影。旧 `source_kind/source_id` 暂时表示 lead member，不再作为 v2 调度真源。

持久化必须有明确的默认池身份。建议每个池保存 `is_default`，并对 `(target_agent_id, downstream_surface, is_default = true)` 建立唯一约束。底层 bind / unbind 命令接受显式 `route_pool_id`；只有产品快捷入口在未指定时解析该 Agent / surface 的唯一默认池，不能靠创建时间或当前运行状态猜测。

### 4.2 `RouteMember`

建议规范化保存：

```text
id
route_pool_id
source_kind
source_id
enabled
priority
position
```

第一版不实现 weight、least-conn 或余额调度。默认策略为 `priority_failover`；需要让多个同构授权真正分担并发请求时，再显式选择 `round_robin`。

### 4.3 `ModelRouteRule`

建议保存：

```text
route_pool_id
public_model
endpoint_family
upstream_provider
upstream_dialect
upstream_model
priority
enabled
```

第一版只支持 exact model。一个 `public_model` 只有在规则显式声明候选等价时，才允许映射到多个 Provider；不能只因名称相同就合并。

### 4.4 能力与可用性

`MemberCapabilitySnapshot` 表达稳定能力：模型和 endpoint 是否被该成员支持，证据来自静态 mapping、远程发现或运行时观测。它与临时可用性分开：

- capability：`supported | unsupported | unknown`；
- availability：授权失效、cooldown、并发已满、网络故障等运行时状态。

短暂 429 或并发满不能直接修改稳定模型目录。能力刷新生成新的 index generation，再原子替换；进行中的请求继续使用其进入时取得的 snapshot。

### 4.5 `DispatchCandidate`

跨产品成员不能只是一个 credential。每个候选至少携带：

```text
member_id
upstream_endpoint
upstream_model
upstream_provider
upstream_dialect
transport_key
capability_generation
```

这保证切到 Grok 成员时，同时切换 Grok endpoint、模型映射和 transport，不会拿 Grok token 请求 Codex 上游，反之亦然。

## 5. 持久化、绑定与兼容迁移

`bind(票, Agent)` 的候选语义改为 enrollment：

- 将该票加入目标 Agent 默认 `RoutePool`；
- 第一次切换到 v2 默认池时原子写入客户端配置，之后增删成员不再改 `base_url` 和本地 bearer；
- `unbind` 只让该票退池；池空后才清客户端投影；
- `native_endpoint` / `config_sync` 默认不进池，只有用户明确「交给本机网关」时才 enrollment，不得静默劫持官方直连。

迁移必须满足：

1. 新增规范化 route-member 关系和 policy revision，不能把多个 `source_id` 塞进一个无约束 JSON 字段。
2. 每个旧 local-bridge profile 读取为「一个 RoutePool + 一个 lead member」；不自动合并多个旧 profile。同一 Agent / surface 有多个旧 profile 时，优先把 active binding 对应 profile 标为默认池；没有 active binding 时按稳定 id 顺序选择一个默认池，其他保留为非默认兼容池。
3. 保留原 profile id、本地 bearer、target agent 和 auto-start。`profile.port` 作为 legacy 投影保留，但 v2 runtime 使用统一 `gateway_port`：原本已复用 primary port 的 profile 可以无感迁移；使用不同显式端口的 profile 继续按 v1 运行，直到用户显式切换 v2，再最多原子改写一次客户端配置。
4. 迁移期保留 `source_kind/source_id` 作为 lead member 投影；新写路径同步该投影，提供回滚窗口。
5. 没有显式 `ModelRouteRule` 的旧 profile 继续使用现有 `AdapterModelMappingTable` 兼容解析。
6. start / restore 一次性加载 route、members、model rules 并构建完整索引；成员失败只形成成员级状态，不能留下半初始化 runtime。
7. bind / unbind、成员删除和客户端投影更新必须幂等；中途失败需要补偿，不得留下幽灵成员。统一端口被占用或 listener 启动失败时，必须保持旧 v1 listener 和客户端投影不变。
8. 成员更新通过 route revision 原子 reload；in-flight 请求固定使用旧 snapshot 和已取得的 permit。

## 6. 统一解析、调度与模型目录

### 6.1 请求路径

```text
本机 bearer 鉴权
  → 定位 RoutePool，识别 downstream surface / dialect / endpoint
  → 读取 EffectiveRouteIndex snapshot N
  → resolve(public_model)
  → 得到受限 DispatchCandidate[]
  → scheduler 只在候选集合内过滤健康、冷却和并发
  → sticky / priority_failover / round_robin 选一个 candidate
  → 按该 candidate 的 Provider transport 重新 prepare 原始请求
  → 发送 attempt
  → Provider-specific error classifier
  → 尚未提交下游字节且允许重放：排除当前候选后 bounded failover
  → dialect-aware response validator / encoder
```

Resolver 接口应形成单一真相源：

```text
resolve(route, endpoint, public_model) → DispatchCandidate[]
list_models(route, endpoint) → public models from the same index
```

调度器只能缩小 resolver 返回的候选集合，不能重新解释模型或扩大候选。现有 `AccountPicker` 可以复用，但 v2 接口必须接收 `candidates + affinity_key + excluded_members`，不能继续无条件 `pick_new()`。

跨 member 或 Provider failover 时，必须从原始 admitted request 重新 prepare；不得复用第一个 Provider 已经修改过的 body 或 headers。

### 6.2 调度策略

选择顺序建议固定为：

1. Resolver 已完成 exact model、endpoint、Provider 和 dialect 过滤。
2. 排除 disabled、auth invalid、cooldown、并发已满和本请求已失败的候选。
3. 若存在有效 affinity，优先原成员。
4. `priority_failover`：按 priority、position、member id 稳定排序。
5. `round_robin`：只在同 priority、同 transport/dialect 的同构候选中轮询。
6. 获取成员并发 permit，生成 attempt。

每次请求的 attempt 数不得超过唯一候选数和全局上限；失败成员进入本请求 exclusion set，避免循环。

Affinity 必须按 RoutePool 隔离，不能直接以客户端提供的 session id 作为全局键。建议键为 `(route_id, downstream_dialect, hash(session_identifier))`，绑定值包含 `member_id / provider / upstream_dialect / index_generation`。每次读取都重新确认成员仍属于当前 resolver 候选集合；成员删除、禁用、授权指纹变化、模型规则或 dialect 变化时立即失效。相同 `prompt_cache_key` 或 conversation id 出现在另一个 bearer / pool 时不得命中同一粘性记录。

### 6.3 `/models`

```text
公开模型集合
  = enabled ModelRouteRule / legacy mapping
  ∩ 至少存在一个 stable-capable DispatchCandidate
```

`GET /models` 与 `/v1/models` 从请求 dispatch 使用的同一 `EffectiveRouteIndex` 枚举，不同步 fan-out 请求所有上游。API Provider 可以后台刷新远程 `/models`；Codex/Grok subscription 可以使用静态声明、mapping 和运行时证据。

- 单成员刷新失败时保留未过期的最后成功快照，不能清空整个池。
- 临时 cooldown、并发满或短暂网络故障不让模型从目录消失。
- 永久失去能力后，模型才从下一代索引移除。
- 暂时没有健康候选时，请求返回明确 `pool_exhausted`，必要时带 `Retry-After`。
- 标准模型接口只返回稳定公共 model id；成员来源、快照时间和 availability 放在 Routes 管理状态中。
- 不同 RoutePool bearer 的模型目录不得互相泄露。

必须用性质测试锁定：`/models` 返回的每个模型至少能 resolve 一个 capability candidate；scheduler 返回的成员一定属于该候选集合。

## 7. Codex / Grok Responses 方言

协议维度必须正交表达：

```text
DownstreamSurface = responses
DownstreamDialect = codex | grok | generic
UpstreamWire      = responses
UpstreamDialect   = codex | grok
AuthMethod        = oauth | api_key
```

本地 bearer / RoutePool 确定 downstream Agent 和 dialect；`RouteResolver` 确定 upstream dialect；选中的 `DispatchCandidate` 确定 transport、endpoint 和 auth。不能靠同一个 `/v1/responses` 路径猜 Codex 或 Grok。

保留独立 `CodexTransport` 与 `GrokTransport`。共享层只处理 Responses envelope、SSE 生命周期、通用 usage 和重试契约；Provider 层分别处理：

- Codex：字段 allowlist、`store:false`、system folding、模型过滤；
- Grok：身份 headers、tool normalization、prompt cache / session、reasoning replay 和 recovery。

「两边都是 Responses」不自动等于 passthrough。只有以下条件同时满足才允许透明转发：

```text
surface 相同
AND dialect compatibility 显式标记为 transparent
AND response sanitizer / validator 通过
```

其他情况必须使用方向明确的 pair adapter：

- `CodexIngress → GrokUpstream`；
- `GrokIngress → CodexUpstream`。

两个方向分别取证、分别用 capability edge 和 feature flag 放行。契约必须覆盖 request、response、SSE、reasoning、tool call / output、usage、错误事件和 Provider 特有字段过滤。

`previous_response_id`、prompt cache、Grok session seed 和 encrypted reasoning recovery 等状态默认绑定原 Provider / member。粘性候选失效时，只有适配器明确证明可迁移才允许切换；否则返回可解释错误，不做静默跨账号或跨 Provider 重放。

## 8. 错误分类与重试边界

不能只用 HTTP status 决定是否隔离整号。Provider transport 必须返回 typed error：

| 错误 | 作用范围 | 候选行为 |
|---|---|---|
| 普通 400 / 422 | request | 不切成员，直接返回 |
| Grok reasoning 可恢复错误 | 当前 attempt | 同成员有限修复重试 |
| 401 | authorization | 同成员 reload 一次；仍失败则授权级隔离并切候选 |
| 模型 / endpoint 无权限的 403 / 404 | member-model-endpoint | 更新能力状态，可切其他候选 |
| 一般策略 403 | Provider classifier 决定 | 不得盲目视为模型错误或整号失效 |
| 账号总配额 429 | member | 按 `Retry-After` / reset 冷却，可在提交前切候选 |
| 模型级限流 429 | member-model | 只冷却相应模型 bucket |
| 网络、超时、5xx | transient attempt | 可重放且未提交下游字节时 bounded failover |
| 空 200 | Provider-specific | 由 transport 判定是否非法，不能全局视为失败 |
| 已提交下游字节后的断流 | stream | 禁止自动重放，结束当前流 |

硬边界是「任何下游字节已经 commit」，不是只判断有没有 text token。带 Provider 侧工具副作用或状态型 continuation 的请求默认不可重放。

OAuth reload 必须按授权指纹做 singleflight，而不是每个请求各自 refresh。并发 401 时只允许一个请求执行刷新，其他请求等待后重新读取 auth revision；旧 revision 的刷新结果不得覆盖新凭据。只有刷新后的同一 revision 再次失败，才把该授权标记为不可调度。若同一授权被多个 RoutePool 引用，这个协调状态也必须共享，但不得共享各池的模型目录或 affinity。

每次 attempt 记录 `request_id / route_id / member_id / provider / model / attempt / retry_reason / usage / final_status`。上游可能已接受但下游没有收到响应时，标记 `billable_possible / unknown`，不能当成零消耗。日志和错误体不得包含本地 bearer、上游 token、完整 prompt 或工具参数。

## 9. 评估切片

以下是可单独验收的候选切片，不是已批准排期。每一刀都必须由 feature flag 或 capability matrix fail-closed；下游客户端地址尽量只改一次。

### P0 契约测试与固定入口

先增加当前缺陷的失败测试，并固定 Gateway 设置端口的重启、占用冲突和旧端口迁移语义。暂不改变生产调度。

验收：A 只支持 `m1`、B 只支持 `m2` 时，测试能证明 `m1` 不得选择 B；已使用 primary port 的路由重启后端口不漂移；显式旧端口在未切换 v2 前保持原状；统一端口占用失败不会覆盖旧客户端配置；现有 profile bearer 隔离不变。

### P1 `RoutePool` / `RouteMember` 持久化

增加成员 CRUD、排序、enabled、priority、revision、默认池唯一约束和 legacy 单成员投影。建立 route/surface-scoped Hub token；初版仍只使用 lead member。

Flag：`route_pool_v2`，UI 默认隐藏。

验收：旧 profile 的 id、bearer 和 auto-start 不变；多旧 profile 能确定性选出唯一默认池；显式旧端口只在用户切换 v2 时原子改写一次；重启后成员引用稳定；重复授权指纹被拒绝；不自动合并旧 profile。

### P2 统一 Resolver，保持单成员等价行为

建立 `DispatchCandidate` 和 `EffectiveRouteIndex`；dispatch 改为 resolve 后再 schedule；Provider prepare 移入每次 attempt。v2 route 不再隐式扫描其他 profile 做 model switch。

Flag：`route_index_v2`。

验收：未知或歧义模型不访问上游；scheduler 不能选择候选集之外的成员；legacy route 行为保持一致。

### P3 成员能力与 `/models`

合并静态 mapping、远程 discovery 和运行时 evidence，原子更新索引；`/models` 从 resolver index 枚举，不再直接读取 edge 级静态名单。

`route_index_v2` 必须同时控制 dispatch 与 `/models`，禁止拆成两个独立生产开关。

验收：A:`m1`、B:`m2` 时目录为 `m1,m2`，且请求永远不会选错成员；部分刷新失败不清空其他成员；两个 bearer 的目录互不泄露。

### P4 同 transport / dialect 多授权

复用 `AccountPicker`、`MemberHealthSink` 和 capability matrix，但改成 candidate-aware。只对完整取证的同构 cell 打开 `multi_account`；默认 `priority_failover`，需要真实分担请求时显式启用 `round_robin`。

验收：共享模型按选定策略分配；独占模型只进入对应成员；sticky 候选失效后可解释地重选；并发不突破成员限制。

### P5 细粒度健康与 failover

实现 typed error classifier、member / member-model cooldown、`Retry-After`、attempt exclusion 和并发 permit 释放。先对 mock 和单一已取证 Provider 开放。

验收：401 reload 后才隔离；403/404 不误伤整号；429 作用域正确；网络/5xx 只在安全边界内切换；任何下游字节提交后都不重放。

### P6 Codex ↔ Grok 双向 Responses

分别实现并取证 `CodexIngress → GrokUpstream` 和 `GrokIngress → CodexUpstream`。两个方向使用独立 feature flag，并继续受 capability matrix 控制，默认 Experimental / off。

验收：双向 stream / non-stream golden fixtures 通过；Provider 特有字段不泄漏；模型映射不存在时在访问上游前失败；stateful continuation 不静默漂移成员。

### P7 显式 mixed-provider composite route

一个 public model 可以显式配置多个 Provider lane；先按模型规则确定 lane，再在 lane 内选授权。跨 Provider failover 仅限规则声明等价、请求可重放且尚未提交下游字节。

Flag：`mixed_provider_pool`，默认关闭。

验收：不按模型名猜 Provider；未配置等价关系时不跨 Provider；管理状态能解释每个模型的候选和拒绝原因。

### P8 产品面

Routes 展示固定入口、surface、成员、稳定模型能力和临时 availability；Connections 继续只管登录；「用于某 Agent」表示 enrollment。提供官方直连「交给本机网关」的显式操作。

验收：日常用户只看到默认 Agent/surface 池；高级多池能力不增加默认操作复杂度；当前实现未开放的 edge 不出现在可选项中。

## 10. 测试门槛

| 层级 | 必测内容 |
|---|---|
| 迁移 | legacy 单 source 合成单 member；多 profile 默认池选择；显式旧端口切换/回滚；id/bearer/auto-start；bind/unbind 幂等与补偿 |
| Resolver | exact 匹配；endpoint 隔离；未知/歧义 fail-closed；public/upstream model 映射 |
| `/models` | A:`m1`/B:`m2` 并集；去重；stale snapshot；部分刷新失败；跨 bearer 隔离 |
| Scheduler | priority failover；round-robin；route-scoped sticky；跨 bearer 同 session id 隔离；规则更新/成员删除后的 affinity 失效；并发 limit |
| 错误 | 并发 401 refresh singleflight 与 auth revision；403 entitlement；404 member-model；429 scope/`Retry-After`；5xx/network |
| SSE | 提交前切换；提交后禁止重放；chunk/CRLF；completed 缺失；客户端断开释放 permit |
| Codex ↔ Grok | 双向 stream/non-stream、reasoning、tool call/output、parallel tools、usage、错误事件 |
| 状态连续性 | prompt cache、session、`previous_response_id`、Grok encrypted reasoning recovery |
| 生命周期 | start/update/restore/stop 幂等；snapshot 原子替换；无幽灵成员 |
| 安全 | token 不跨 route/surface；模型目录不串池；错误体和日志脱敏 |
| 性质测试 | `/models` 每个模型均有 resolver candidate；scheduler 输出一定属于 candidate set |

现有 `two_profiles_two_bearers_two_surfaces_do_not_cross`、`shared_port_tokens_do_not_cross_on_models`、`multi_account_isolates_a_then_b_serves_and_a_returns_after_restore`、Responses SSE 和 Grok reasoning fixtures 应保留并增加 pool 级版本。

## 11. 开做前要拍板的点

1. **Hub token 粒度。** 建议初版 route/surface-scoped；默认池可表现为每 Agent 一把。不要用一个全局 token 覆盖三个 surface。
2. **默认调度策略。** 建议 `priority_failover`；需要多授权真实分担时由用户切到 `round_robin`。
3. **官方直连是否入池。** 默认不接管；Routes 提供显式「交给本机网关」。
4. **池成员是否可裁剪。** 跨产品池必须显式 enrollment；不能把同票面账号自动全部收入异构池。
5. **`/models` 空名单。** fail-closed，不把未知模型发给任意上游。

## 12. 门槛与非目标

在 RoutePool 持久化、统一 resolver、`/models` 同源索引、按 candidate 重算 body 和至少一条跨产品边取证完成前，不得把本文写进现行 Routes 说明或 CLI 帮助。

非目标：

- 只把现有边上的 `multi_account` 设为 `true`，当作本提案已经落地。
- 给每张票继续发不同本机 token，指望客户端自己换钥匙。
- 做成对外 `sk-xxx` 公网网关或拼车服务。
- 在页面层做调度。调度必须留在 `bridge` host，资格仍由 `plan()` 决定。
- 根据模型名前缀猜 Provider，或在未声明等价关系时跨 Provider 自动 fallback。
- weight、least-conn、余额调度、公网监听、计费、多租户。
- sidecar 进程迁移（见 [Local Route Sidecar](adapter-sidecar.md)）；本提案不改变 runtime 进程边界。
- 凭据落盘加密；国产 OAuth 开边或 OAuth 转 API。

## 13. 与历史记录的关系

| 记录 | 关系 |
|---|---|
| [a4-unified-loopback-gateway.md](../archive/a4-unified-loopback-gateway.md) | 已落地的同口 Gateway；本提案建立在它上面，不重做 listener |
| [multi-account-routing-rfc.md](../archive/multi-account-routing-rfc.md) | 同类多号内核；对应本提案 P4，不是 P7 跨产品池 |
| [routing-connection-refactor-plan.md](../archive/routing-connection-refactor-plan.md) | 历史泳道拆分；其中 C2 仍部分未闭环，不得当作现行待办 |

## 相关页面

- [Connections、Routes 与绑定](../concepts/connections-and-routing.md)
- [Adapters 与本机 Bridge](../concepts/adapters-and-bridges.md)
- [本机 Routes API](../reference/local-route-api.md)
- [Route 兼容性](../reference/route-compatibility.md)
- [产品边界](../decisions/product-boundaries.md)
- [Local Route Sidecar](adapter-sidecar.md)
- [提案索引](README.md)
