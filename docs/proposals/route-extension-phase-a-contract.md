---
title: 路由官方扩展 A 阶段纸面契约
type: proposal
status: proposed
owner: maintainers
updated: 2026-10-03
---

# 路由官方扩展 A 阶段纸面契约

本文冻结[路由官方扩展与独立运行方案](adapter-sidecar.md) A 阶段的纸面契约：窄运行接口、`active`/`prepared` 分离、锁顺序、版本与撤销水位、操作留存、fixture/故障计划以及隔离路径与证据格式。这些都是 `proposed` 候选，**不能当现行产品接口**。隔离 scratch 下已经实现 `Handshake`、`Status`、`AcquireOrRenewOwner`、desired-config（`BootstrapDesired` / `PrepareDesired` / `CommitDesired` / `AbortDesired` / `GetOperation`）与合成 Key 的 Messages JSON/SSE；`ActivateProbeListen` 仍是 scratch 快捷，不是产品提交。本页不做插件平台，不把隔离切片扩成 live sidecar，也不授权默认网关切换、真实配置写入、插件商店或 E/F。

现行写入入口仍是 `AdapterControl` 的 `plan` / `bind` / `unbind` 与进程内 `adapter_bridge_controller`；现行状态 DTO 仍是带 `local_token` 的 `AdapterBridgeStatus`。下面的 `RouteRuntimeControl` 消息不得替代这些现行接口。

对齐[模块化提案「功能模块与官方扩展」](modularity.md#8-功能模块与官方扩展)：路由是首个随应用交付、固定注册的官方扩展候选；登录与连接仍由 core 唯一负责；不把所有功能改成可下载插件，不引入动态 ABI 或第二份领域库。产品边界仍以[产品边界](../decisions/product-boundaries.md)为准，本页不改。

## 1. 范围与非目标

| 本页做 | 本页不做 |
|---|---|
| 冻结候选消息字段、前置条件、成功/失败与错误码 | 把隔离切片接到产品写入、默认网关或真实应用目录 |
| 按现有 `adapter_control` 与 `adapter_bridge_controller` 调用链写出锁顺序设计 | 改 Rust / TypeScript、改锁实现、改数据库迁移 |
| 冻结 generation、watermark、reconcile 与操作留存规则 | 把提案写成「A 过程门槛已通过、可以做产品 B」 |
| 写明 fixture 对照、故障计划、隔离目录与证据格式 | 把隔离 probe 扩成 A 所需的三协议 / 故障 / 撤销证据，或接入真实用户数据 |

候选标识仍是 `agenthub.routes`，候选二进制名仍是 `agenthub-adapterd`，均非当前产品可用接口。隔离 scratch 下的 v0 控制面已经可运行。调查基线仍是 annotated tag `baseline/routes-before-extension-20261003`（`7c2b6fe2be67bbaa0b509deee6d6002981c60fd9`），本契约不移动该 tag。

## 2. 通道、信封与禁止项

控制通道与登录通道可共用受保护传输，但消息权限、日志规则和数据类型必须分开。通道身份经受保护的启动通道建立，不使用模型请求的入口 Key 作为控制权限。仅有随机名称或 loopback 地址不算访问控制。

### 2.1 普通控制信封

除下面写明的豁免外，普通控制请求都带下列字段。字段名是候选契约，不是现行 JSON。`Handshake` 请求不带 `owner_term`。本 epoch 尚无 owner 时的首次 `AcquireOrRenewOwner`（`mode = acquire`）也不带、也不匹配已有 `owner_term`，因为此时还没有可匹配的 term；成功回复才发放第一个 `owner_term`。`takeover` 只可带已失效的 `previous_term`，新 term 同样只出现在成功回复里。`renew` 以及此后的控制请求必须带上并匹配当前 `owner_term`。

| 字段 | 类型 | 作用 |
|---|---|---|
| `request_id` | 字符串 | 幂等键；保留期内相同 id 必须配相同 `payload_hash` |
| `payload_hash` | 十六进制字符串 | 对语义载荷做规范化哈希，不含 `request_id` 本身 |
| `instance_epoch` | 不透明字符串 | 本次 runner 进程实例；`Handshake` 发放，之后必填 |
| `owner_term` | 单调正整数 | 本 epoch 内的 owner 代数；`Handshake` 请求不带 |
| `owner_id` | 不透明字符串 | 当前 core 所有者标识 |
| `app_data_dir` | 规范化绝对路径 | 应用数据目录范围，防止不同安装或旧 core 找错进程 |

`Handshake` 成功回复发放 `instance_id` 与 `instance_epoch`。除首次 `acquire` 与 `takeover` 请求本身外，之后的控制请求必须同时匹配 `instance_epoch`、`owner_term`、`owner_id` 与 `app_data_dir`。首次 `acquire` 只匹配 `instance_epoch` 与 `app_data_dir`；`takeover` 匹配 epoch、目录，以及可选的已失效 `previous_term`，不要求匹配一个仍然有效的 term。

### 2.2 禁止出现在普通控制消息中的材料

下列内容不得进入普通控制请求、回复、错误、`Status`、操作留存或日志：

- 入口 Key（界面「入口 Key」，内部 `local_token` / bearer）
- 上游登录材料、refresh token、OAuth 材料
- 启动认证秘密
- 真实模型请求正文、工具参数、prompt

入口 Key 只允许出现在私密通道 `ValidateIngressKey`。登录材料只允许出现在私密通道 `ResolveAuth` 的回复内存中。准备令牌只允许出现在 `PrepareDesired` / `BootstrapDesired` 回复、对应的 `CommitDesired` / `AbortDesired` 请求，以及 core 待完成操作记录；`Status` 与日志只许留令牌指纹。

### 2.3 共用错误对象

失败回复使用同一形状，不含禁止材料：

| 字段 | 说明 |
|---|---|
| `code` | 下表候选错误码 |
| `message` | 可展示短句，不含 Key 或登录材料 |
| `retryable` | 调用方是否可在保留期内用同一 `request_id` 再查或重试 |
| `operation_id` | 若与某次准备/提交有关则带上 |
| `instance_epoch` / `owner_term` |  runner 当前观察到的值，供核对；旧值冲突时仍返回 |

候选错误码（均 `proposed`，不是现行 `adapter.*` 产品码）：

| 码 | 含义 |
|---|---|
| `route.runtime.unauthenticated` | 启动通道或 owner 未建立 |
| `route.runtime.protocol_mismatch` | 运行协议版本不兼容 |
| `route.runtime.config_format_mismatch` | 配置格式版本不兼容 |
| `route.runtime.package_mismatch` | 扩展包版本不兼容 |
| `route.runtime.scope_mismatch` | `app_data_dir` 与 runner 范围不一致 |
| `route.runtime.stale_epoch` | 旧 `instance_epoch`，永久拒绝 |
| `route.runtime.stale_term` | 旧 `owner_term`，永久拒绝 |
| `route.runtime.owner_conflict` | 仍有有效 owner，拒绝接管或第二 owner |
| `route.runtime.not_owner` | 调用方不是当前 owner |
| `route.runtime.secret_on_control` | 普通控制消息带了入口 Key 或登录材料 |
| `route.runtime.revision_low` | 请求版本低于当前 `active_revision` |
| `route.runtime.hash_conflict` | 同版本不同 `hash` |
| `route.runtime.base_mismatch` | `base_revision` 或 `expected_epoch` 不匹配 |
| `route.runtime.active_not_null` | 对非空实例误用 `BootstrapDesired` |
| `route.runtime.active_null` | 需要 `active` 基准时实例仍为空 |
| `route.runtime.prepare_conflict` | 已有未过期的其他 `prepared` |
| `route.runtime.prepare_expired` | 准备令牌或 `prepared` 已过期 |
| `route.runtime.token_invalid` | 准备令牌与操作/版本/owner 不匹配 |
| `route.runtime.operation_in_progress` | 同 `operation_id` 仍在执行 |
| `route.runtime.operation_unknown` | 无留存记录，且状态不足以断定结果 |
| `route.runtime.payload_mismatch` | 同 `request_id` 不同 `payload_hash` |
| `route.runtime.port_in_use` | 已登记固定端口被占；产品面仍映射现有 `adapter.port_in_use`，不杀进程、不换口 |
| `route.runtime.drain_timeout` | 排空超时，在途被终止 |
| `route.runtime.host_unavailable` | 无法联系 runner |
| `route.runtime.core_unreachable` | runner 联系不到 core；新 HTTP 必须拒绝 |
| `route.runtime.auth_generation_stale` | `ResolveAuth` 迟到回复 |
| `route.runtime.ingress_generation_stale` | `ValidateIngressKey` 迟到回复 |
| `route.runtime.watermark_block` | 命中撤销水位，材料或入口判定不可用 |
| `route.runtime.not_serving` | 无有效 owner、生命周期不可服务或监听未就绪 |

## 3. 窄运行接口

下列消息是候选 `RouteRuntimeControl`，不是现成命令或已定库。成功与失败都要能用 `GetOperation` 与 `Status` 对照核实。

### 3.1 Handshake

检查运行协议、配置格式、扩展包版本及应用数据目录范围，返回本次进程实例标识。每次 runner 启动产生新的 `instance_epoch`；新 epoch 从 actual 空开始，`active_revision = null`。

| 方向 | 字段 |
|---|---|
| 请求 | `protocol_version`，`config_format_version`，`package_version`，`app_data_dir`；**不带** `owner_term`、入口 Key、登录材料 |
| 成功 | `instance_id`，`instance_epoch`，双方接受的协议/格式/包版本，能力列表（实现能力，最终可用范围仍取 core 支持边与实现能力的交集），`active = null`，`prepared = null` |

前置条件：已通过受保护启动通道；该 runner 尚未把本目录交给另一个仍有效的 epoch。

成功：建立实例身份，**不**发放 `owner_term`，**不**开始监听，**不**接收模型请求。

失败：`protocol_mismatch` / `config_format_mismatch` / `package_mismatch` / `scope_mismatch` / `unauthenticated` / `secret_on_control`。

### 3.2 AcquireOrRenewOwner

当前 core 所有者取得或续期运行许可。取得许可本身不激活监听。

| 方向 | 字段 |
|---|---|
| 请求 | 控制信封。`acquire`（本 epoch 尚无 owner）**豁免** `owner_term`，信封里不带该字段。`takeover` 不带当前有效 term，只可带已失效的 `previous_term`。`renew` 必须带并匹配当前 `owner_term`。另有 `mode = acquire \| renew \| takeover`，`lease_budget`（候选，待 B 实测；方案起始值见提案第 5 节，不是当前默认） |
| 成功 | `owner_term`，`owner_lease_until`，`mode` 实际结果 |

前置条件：

- 已 `Handshake`，`instance_epoch` 匹配。
- `acquire`：本 epoch 尚无有效 owner。
- `renew`：调用方是当前 owner，lease 仍有效；**不改变** `owner_term`。
- `takeover`：原 owner 已撤销或过期；发放本 epoch 内单调递增的新 `owner_term`。仍有有效 owner 时拒绝。

成功：

- 首次取得或接管：`owner_term` 递增；接管必须先清登录缓存、按 core 持久状态重建撤销 watermark，并完成旧请求排空及撤销屏障核对。
- 正常续期：term 不变，只延长 `owner_lease`。
- 生命周期仍不可服务，直到后续 `CommitDesired` 且监听就绪。

失败：`stale_epoch` / `stale_term` / `owner_conflict` / `not_owner` / `unauthenticated`。

### 3.3 BootstrapDesired

仅限新 `instance_epoch` 的已认证唯一 owner，接受一次完整的已持久 desired 版本并建立 `prepared`。`active` 保持 `null`，不得伪造旧 actual，不得绕过后续版本规则。

| 方向 | 字段 |
|---|---|
| 请求 | 控制信封，`operation_id`，完整快照（见第 4.3 节），`config_revision`，`hash`，`expected_epoch` |
| 成功 | `operation_id`，`prepared { revision, hash, operation_id, prepare_token, token_expiry, base_active_revision: null }`，`active = null` |

前置条件：当前 owner；`expected_epoch` 等于本实例；`active_revision` 为 `null`；本 epoch 尚未对另一份快照 bootstrap 成功。同 epoch 新 owner 接管**不得**再次 bootstrap，必须走 `PrepareDesired`。

成功：只建立 `prepared`，拒绝模型请求，不得把 bootstrap 或核对成功当作提交成功。

失败：`active_not_null` / `not_owner` / `stale_epoch` / `stale_term` / `hash_conflict` / `prepare_conflict` / `payload_mismatch`。

### 3.4 PrepareDesired

按当前 `active` 的基准版本准备完整网关配置快照。允许旧 `active` 与新 `prepared` 并存，不接受旧 `owner_term`。

| 方向 | 字段 |
|---|---|
| 请求 | 控制信封，`operation_id`，完整快照，`config_revision`（新的前向版本），`hash`，`base_revision`，`expected_epoch` |
| 成功 | `prepared { revision, hash, operation_id, prepare_token, token_expiry, base_active_revision }`；`active` 不变 |

前置条件：当前 owner；`active_revision` 非空且等于 `base_revision`；`expected_epoch` 匹配；`config_revision` 严格大于 `active_revision`；至多一份未过期 `prepared`。同一 `operation_id` 且同一 `payload_hash` 在保留期内幂等返回原准备。另一 `operation_id` 的未过期准备必须先 `AbortDesired`。

成功：旧版本继续服务已授权请求；新请求仍走 `active`，直到 `CommitDesired`。

失败：`active_null` / `base_mismatch` / `revision_low` / `hash_conflict` / `prepare_conflict` / `stale_term` / `prepare_expired`。

### 3.5 CommitDesired

以准备令牌和操作幂等键把 `prepared` 原子晋升为 `active`。不得把 bootstrap 或核对成功当作提交成功。

| 方向 | 字段 |
|---|---|
| 请求 | 控制信封，`operation_id`，`prepare_token`，`config_revision`，`hash` |
| 成功 | `active { revision, hash }` 等于原 `prepared`；`prepared = null`；若 owner 有效、监听就绪且授权可校验，生命周期转为可服务 |

前置条件：token 引用同一 owner、`owner_term`、实例、操作和版本；`prepared` 未过期；`config_revision`/`hash` 与 `prepared` 一致。

成功：新请求使用新快照；在途请求保留已有快照。CAS 一旦观察到后续成功版本，就不能撤销或覆盖该版本。

失败：`token_invalid` / `prepare_expired` / `stale_term` / `payload_mismatch` / `operation_in_progress`。确认超时先 `GetOperation`，不假定成功，也不盲目重复激活。

### 3.6 AbortDesired

只撤销指定操作的 `prepared`，不撤销仍有效的 `active`。

| 方向 | 字段 |
|---|---|
| 请求 | 控制信封，`operation_id`，`prepare_token` |
| 成功 | `prepared = null`；`active` 保持原值（包括原本就是 `null`） |

前置条件：token 与操作匹配；只作用于该 `operation_id` 的准备。准备已过期时，结果与成功 abort 相同（`prepared` 已空），`GetOperation` 报 `expired` 或 `aborted`。

失败：`token_invalid` / `stale_term` / 若 `operation_id` 已 `committed` 则拒绝（不能用 abort 回滚已晋升的 `active`）。

### 3.7 GetOperation

查询一次控制操作，并同时返回当前 `active`/`prepared`，供超时后核实与恢复。

| 方向 | 字段 |
|---|---|
| 请求 | 控制信封，`operation_id` 或 `request_id` |
| 成功 | `operation_state`，`active`，`prepared`（无 token），`lifecycle`，`port`，`in_flight_count` |

`operation_state`：

| 值 | 含义 |
|---|---|
| `in_progress` | 已接受但尚未完成 |
| `prepared` | 准备已建立，尚未提交 |
| `committed` | 该操作已把 prepared 晋升为 active |
| `aborted` | 该操作的 prepared 已撤 |
| `expired` | 准备过期或留存过期 |
| `unknown` | 没有可核实记录 |

历史 `committed` 不表示新实例仍在服务。查询未知或记录已过期时先核对 `Status`，不换新 id 盲目重做。

失败：`stale_epoch` / `stale_term` / `payload_mismatch`（以 `request_id` 查询但载荷指纹不一致）。

### 3.8 Status

返回实例标识、版本、可选 `prepared`、生命周期、端口、活动请求数和脱敏错误。扩展状态不复用含 `local_token` 的 DTO。界面复制入口 Key 继续经 core 专用接口取得。

| 成功字段 | 说明 |
|---|---|
| `instance_id` / `instance_epoch` / `owner_term` | 当前实例与 owner 代数 |
| `active_revision` / `active_hash` | 已提交配置；`null` 表示尚未 commit。若实现保留 `applied_revision`，它只能是 `active_revision` 的兼容别名 |
| `prepared` | 可选 `{ revision, hash, operation_id, token_expiry, base_active_revision }`，**不含** `prepare_token` |
| `lifecycle` | `empty` / `prepared_only` / `serving` / `not_serving` / `draining` / `stopped` / `host_unavailable` |
| `listen_ready` | 监听是否已绑定 |
| `port` | 当前或已登记端口；被占时失败码走 `port_in_use` |
| `in_flight_count` | 活动请求数 |
| `owner_lease_valid` | 是否仍允许接新请求 |
| `last_error` | 脱敏错误码与时间 |

`active_revision` 不单独表示正在服务。接新请求必须同时满足：有效 owner 许可、生命周期可服务、监听就绪、当前授权校验。无法联系程序时报告 `host_unavailable`，可以展示标明时间的历史状态，但不能显示为当前 `serving`。

失败：`unauthenticated` / `stale_epoch` / `secret_on_control`。core 侧联系失败时由 core 把界面标为 `host_unavailable`，不得把旧状态写成 running。

### 3.9 Drain

停止接新请求、限时排空。不能代替 core 解除连接和恢复 Agent 配置。排空不持有全局门（见第 5 节）。

| 方向 | 字段 |
|---|---|
| 请求 | 控制信封，`drain_budget`（候选上限，方案起始 30 秒，待 B 实测） |
| 成功 | 预算内 `in_flight_count` 回到 0；`lifecycle` 随后为 `not_serving`；`active` 作为版本基准保留；`prepared` 撤掉。成功不等于「发生了超时」 |

前置条件：当前 owner 或 owner 已失效后的受控排空；`instance_epoch` 匹配。

成功：不再接新请求；在途在预算内按第 6.4 节策略结束并回到 0。这一结果记为正常排空完成。

超时是失败，不是成功的一种写法：超出 `drain_budget` 时终止剩余请求并释放资源，操作结果为 `drain_timeout`，记录错误与被终止的请求数量，不能记为正常完成。该次排空仍然已经执行过，结果可查，但不得与成功行叠在一起。

失败：`stale_epoch`（请求被拒绝，排空未开始）/ `drain_timeout`（排空已执行，但预算用尽）。锁文件或 PID 不能单独作为释放证明。

### 3.10 Stop

在 Drain 之后确认释放监听。同一 runner 保留已提交的 `active` 作为版本基准，生命周期为不可服务，不能据此显示 running。不得重新使用旧登录缓存。

| 方向 | 字段 |
|---|---|
| 请求 | 控制信封，可选 `drain_budget` |
| 成功 | 监听已释放（需独立状态与端口探测核对）；登录缓存已清；`prepared = null`；`active` 仍可作为同 epoch 恢复基准；`lifecycle = stopped` |

前置条件：同 Drain。`Stop` 不能代替 `unbind`。

失败：`drain_timeout` / `host_unavailable`。端口释放未经探测确认时，core 标 `needs_attention`，禁止并行 bind。

### 3.11 ResolveAuth（私密通道）

core 按成员、协议、用途和最低有效期发放内存登录材料。独立程序仅可解析当前获准快照中的引用。官方登录刷新在 core 中执行，扩展不复制 refresh token 处理。

| 方向 | 字段 |
|---|---|
| 请求 | `instance_epoch`，`owner_term`，不透明 `source`/`member_id`，`protocol`，`purpose`，`min_ttl`；**不含**登录材料本身 |
| 成功 | 内存登录材料，`auth_generation`，`auth_lease_until`，绑定的 `source`/`member_id`/`purpose`/`instance_epoch`/`owner_term` |

前置条件：当前 owner；引用属于当前 `active` 或（仅用于准备核对、且不得服务模型请求的）`prepared` 快照；core 可达。

成功：runner 可按成员/协议/用途/最短有效期做有界内存缓存。这与入口 Key 的无缓存判定是两件事。登录有效期不延长 owner 许可。

失败：`not_owner` / `watermark_block` / `auth_generation_stale` / `core_unreachable` / `not_serving`。迟到回复比较 epoch、term、generation 与撤销 watermark，任一不匹配即丢弃，不写入缓存、不恢复请求。

### 3.12 ValidateIngressKey（私密通道）

每个新的 HTTP 请求（包括 `/health`、`/models` 和模型请求）都经私密 core 通道校验入口 Key 是否有效并解析目标池。扩展不得缓存这项判定。普通 IPC 控制消息不带入口 Key。

| 方向 | 字段 |
|---|---|
| 请求 | `instance_epoch`，`owner_term`，入口 Key，请求路径/方法等非正文元数据 |
| 成功 | `accepted`，目标 `pool_id`，入口 Key `generation`，绑定的不透明 `key_id`、`instance_epoch`、`owner_term` |

前置条件：当前 owner 许可有效；生命周期可服务；core 可达。core 不可达时拒绝新 HTTP 请求，不允许无限使用缓存登录。

成功：只对这一次 HTTP 请求有效。旧 `generation` 的迟到回复必须丢弃，并让该 HTTP 请求重新经当前 core 校验；不能沿用旧目标池或报告 Key 仍有效。

失败：`not_serving` / `core_unreachable` / `watermark_block` / `ingress_generation_stale`，以及现行产品面的无效 Key 语义（对外仍映射 `invalid_api_key`，不在控制通道日志里写 Key）。

删除或轮转必须拒绝实际已失效的入口 Key。删除主 Key 的同时晋升另一个仍有效的 Key 时，被晋升 Key 保持有效。仅改变 Key 角色不等于轮转，不新增独立晋升 API。

## 4. active 与 prepared

### 4.1 谁可以并存

状态至少分开保存：

| 字段 | 谁写 | 含义 |
|---|---|---|
| `active_revision` / `active_hash` | 仅 `CommitDesired` 成功后 | 本实例已提交的配置版本 |
| `prepared { revision, hash, operation_id, token_expiry, base_active_revision }` | `BootstrapDesired` 或 `PrepareDesired` | 尚未晋升的完整快照 |

规则：

- 旧 `active(v)` 与新 `prepared(w)` 可以并存。
- 同一时刻至多一份 `prepared`。
- `AbortDesired` 或准备过期只撤 `prepared`，不撤仍有效的 `active`。
- `BootstrapDesired` 只建立 `prepared`，新 epoch 的 `active` 保持 `null`，不得伪造旧 actual。
- `CommitDesired` 才把 `prepared` 原子晋升为 `active`，并清空 `prepared`。
- `active = null` 且仅有 `prepared` 时，不能接收模型请求。

### 4.2 状态转换

| 转换 | 规则 |
|---|---|
| `empty -> prepared` | 新 epoch 的 `BootstrapDesired`；不能接收模型请求 |
| `prepared -> active` | `CommitDesired` 原子晋升，active 才开始接收新请求 |
| `active(v) + prepared(w) -> commit active(w)` | 旧版本继续可见，提交成功后新请求使用 `w` |
| `abort` 或过期 | 撤掉 prepared，保持 `active(v)`；若原本为空则保持 `active = null` |
| owner 失效或明确停止 | 撤 prepared、拒绝新请求并排空，清理登录缓存及监听；同一 runner 保留已提交的 `active(v)` 作为版本基准，生命周期不可服务 |
| 同一 epoch 的新 owner 接管 | 递增 term，完成旧请求排空及撤销屏障核对；core 分配新的前向版本 `w`，以 `base_active_revision = v` 执行 prepare/commit；不得再次 bootstrap |
| runner 重新启动 | 新 epoch 的 `active = null`，通过 bootstrap 建立准备再 commit，不继承旧实例 actual |

希望运行的设置（desired）由 core 保存。实际运行状态（actual）来自独立程序；保存过配置不等于正在运行。

### 4.3 完整快照字段

共享端口、池、模型索引和登录引用组成一份完整网关快照，避免分开更新造成半份配置。下列字段是候选 `SnapshotBody`，**不是**当前 `BridgeStartSpec` 的现行序列化。

| 字段 | 进入 `hash` | 说明 |
|---|---|---|
| `listen.host` / `listen.port` | 是 | 只允许 loopback；已登记固定端口冲突映射 `adapter.port_in_use` |
| `pools[].pool_id` | 是 | 池标识 |
| `pools[].downstream_surface` | 是 | Messages / Responses / Chat Completions |
| `pools[].schedule_policy` | 是 | 现行调度策略名；默认关闭的混合供应商边保持关闭 |
| `pools[].members[].member_id` | 是 | 成员标识 |
| `pools[].members[].protocol` | 是 | 上游协议 |
| `pools[].members[].auth_ref` | 是 | 不透明 `source`/`member_id`，不含登录材料 |
| `pools[].members[].listed_models` | 是 | 该成员可服务模型 |
| `model_index` | 是 | 与 `GET /models` 和 dispatch 同一代的规范化索引 |
| `ingress_key_refs[].key_id` | 是 | 不透明入口 Key 引用与目标 `pool_id`、角色 |
| `protocol_capabilities` | 是 | 本快照实际覆盖的协议集合 |
| `feature_flags` | 是 | 仅已有、默认关闭的实验旗标；扩展声明不能新增产品支持边 |
| `config_revision` | 否 | 持久单调版本，由唯一 core 分配 |
| `hash` | 否 | 对 `SnapshotBody` 规范化字节的摘要（候选算法 SHA-256 hex） |

规范化：稳定字段顺序、UTF-8、省略空可选字段、不含登录材料与入口 Key。`config_format_version` 升变才能改字段集合。快照只覆盖配置；健康、冷却、轮询位置和续聊状态的保留或失效规则必须分别定义，不能盲目清空或复制不兼容状态。

`config_revision` 与 `hash` 是拟新增契约，不等同于当前局部 `policy_revision` 或索引 `generation`。core 的版本分配和待完成操作保存在既有应用存储；所需字段及迁移另列实现任务，**本轮不修改数据库**。

## 5. 锁顺序

本节是设计，对照现有写入链，不改 Rust。现有调用链只读自：

- `crates/agenthub-core/src/adapter_control/{contract,coordinator,status}.rs`
- `src-tauri/src/adapter_bridge_controller.rs` 的 `apply_local_bridge` / `apply_local_bridge_locked` / `unbind_local_bridge` / `remove_adapter_with_bridge_cleanup` / `start_local_gateway` / `stop_local_gateway` / `restore_adapter_bridges` / `stop_bridge_runtime`
- `LiveWriteAuthority`（`provider-{agent}.lock`）与 `ProviderService::begin_live_saga`
- `LifecycleShutdownBarrier::enter`

### 5.1 现有调用链（基线事实）

`apply_local_bridge`：生命周期许可 → `lock_profile(profile_id)` → `prepare` → 启动监听与健康核对 → `lock_target(agent)` → `begin_live_saga`（跨进程 switch 锁从 snapshot 持有到 projection/finalize/rollback）。监听在 target 锁之前启动；投影失败再补偿停止本次 saga 拥有的监听。

`unbind_local_bridge` / `remove_adapter_with_bridge_cleanup`：生命周期许可 → `lock_profile` → `lock_target` → `stop_bridge_runtime`（排空）→ core `unbind`（内部再 `begin_live_saga`）。注释写明：**排空放在 Core live-saga 临界区之外，避免排空持有跨进程 provider 锁**。补偿重启前必须 `drop(target_guard)`，否则与 `apply_local_bridge_locked` 再取同一把非可重入 target 锁死锁。

共享网关 `start_local_gateway` / `stop_local_gateway` / 部分 restore：生命周期许可 → `lock_profile("local-gateway")` 作为整网关串行门，再启停监听。`stop_local_gateway` 在该门内逐个 `host.stop`，并写 desired 开关；它不是跨进程 live-write 锁。

### 5.2 统一顺序（候选）

从外到内、从长持有到短持有：

| 级别 | 锁 | 现有对应 | 持有范围 |
|---|---|---|---|
| L0 | 应用数据目录唯一写入所有者锁 | 提案新增；现行尚无整目录 owner，只有每 Agent 的 `LiveWriteAuthority` | 唯一 core 取得规范化应用数据目录的写入权。CLI 只能读，或经该 owner 发起写入，不能各自写数据库。领域写入、待完成操作、撤销持久化持有它；**排空不持有** |
| L1 | 进程生命周期许可 | `LifecycleShutdownBarrier::enter` | 任何启停 saga 入口；关闭时先关门再等 saga 离开，然后才排空 host |
| L2 写入门 | `profile` 门，然后 `target` 门 | `AdapterSagaCoordinator::lock_profile` / `lock_target` | 同一 profile 的生命周期串行；会改该 Agent 现场配置的阶段再取 target。既有 `profile`/`target` 写入门继续由 core 管理 |
| L3 | 网关串行门 | 现行 `lock_profile("local-gateway")` 的整网关角色 | 完整快照的 prepare/commit、同口切换的短时提交。整个共享网关同一时刻只有一个监听负责人 |
| L4 | 每 Agent 跨进程 live-write / switch 锁 | `begin_live_saga` → `provider-{agent}.lock` | 从 snapshot 到 projection/finalize/rollback。持有期间不得再调会取锁的普通 provider API |

排空（Drain/Stop 的监听等待）允许持有 L1 与该 profile 的 L2 `profile` 门，以便同一 profile 不会边排空边启动。**排空不持全局门**：不持有 L0、不把 L3 跨过整个排空等待、不持有 L4。这与现行「Stop outside the Core live-saga critical section」一致，并推广到目录 owner 与网关串行门。

同口切换 saga 中：core 可先用 L0 冻结相关 bind/unbind/启停/Key 变更写入；请求旧 host `Drain` 时释放 L3/L4；确认端口释放后再启动新 runner 做 Handshake / owner / Bootstrap；核对 `active = null` 后才 `CommitDesired`，Commit 只短时持有 L3。

### 5.3 不能反序

| 禁止 | 原因 |
|---|---|
| 先 L2 `target` 再 L2 `profile` | 现行 apply 是 profile → target；反序与持有 target 的 unbind 交叉会死锁 |
| 持有 L2 `target` 时再进入会取同一 target 的 apply | 现行注释：tokio Mutex 不可重入；补偿必须先 drop |
| 排空时持有 L4 | 现行明确把 drain 放在 live-saga 之外；长流会把跨进程锁拖死 |
| 排空时持有 L0 或把 L3 贯穿整个排空等待 | 全局门被长流占用后，其他写入与同口切换无法前进 |
| 持有 L4 时再取 L2 或普通会加锁的 provider API | 现行「do not call ordinary lock-taking provider APIs while it is held」 |
| 先启动新 runner 再排空旧共享端口 | 两个程序竞争同一共享端口；正式同口切换必须先 Drain 并探测释放 |
| CLI 绕过 L0 写数据库 | 破坏唯一写入所有者 |
| 用锁文件或 PID 代替端口探测 | 不能证明监听已释放 |

`GetOperation` / `Status` 为只读核对，不取 L3/L4，也不取 L0 的写入边；它们不能报告成功写入。

## 6. 版本、generation 与撤销水位

### 6.1 作用域

| 标识 | 分配者 | 作用域 | 规则 |
|---|---|---|---|
| `config_revision` | 唯一 core | 该应用数据目录的 desired/actual 配置 | 持久单调；低版本拒绝；同版本同 `hash` 幂等；同版本异 `hash` 冲突；高版本须匹配 `base_revision` 后原子替换。恢复旧内容也必须分配新的前向 revision，只能产生「新版本含旧内容」 |
| `hash` | core 对 `SnapshotBody` 计算 | 与 revision 成对 | 见第 4.3 节 |
| `base_revision` | 调用方按当前 `active_revision` 填写 | 只出现在 `PrepareDesired` 请求（以及准备令牌里绑定的基准） | 必须匹配当时的 `active_revision` 与 `expected_epoch`。`CommitDesired` 请求不带该字段；提交只凭 `prepare_token` 核对已经绑定的基准，不能在提交时另写一个覆盖用的 `base_revision` |
| `expected_epoch` | 调用方 | 一次变更 | 迟到的旧实例响应不能更新当前界面 |
| `instance_epoch` | runner，Handshake 发放 | 一次 runner 进程 | 新 epoch 从 `active = null` 开始 |
| `owner_term` | runner，在 epoch 内发放 | 当前 epoch 的 owner 代数 | 首次取得或接管递增；正常续期不变。旧 epoch 或旧 term 的控制命令、准备令牌、异步登录回复和取消通知永久拒绝 |
| `auth_generation` | 唯一 core | 绑定不透明 `source`/`member_id`、用途、`instance_epoch`、`owner_term` | 每个 `ResolveAuth` 回复都带；runner 只保留撤销 watermark |
| 入口 Key `generation` | 唯一 core | 绑定不透明 `key_id`、用途、`instance_epoch`、`owner_term` | 每个 `ValidateIngressKey` 回复都带；runner **不**缓存 Key 到池的判定 |

两类有界许可：`owner_lease` 允许接收新请求；逐成员 `auth_lease` 表示登录材料有效期。控制连接断开先使 owner 失效，不能等 API Key 失效才停止接新请求。

### 6.2 owner 接管时清缓存

同一 epoch 新 owner 接管必须按顺序：

1. 确认旧 owner 已撤销或过期，递增 `owner_term`。
2. 清理旧 term 的登录缓存。
3. 按 core 持久状态重建撤销 watermark。
4. 完成旧请求排空及撤销屏障核对。
5. core 分配新的前向 `config_revision = w`，`base_active_revision = v`，走 `PrepareDesired` / `CommitDesired`。
6. 核实实际可服务后恢复。

不得再次 bootstrap，不得重用旧登录缓存，不得把旧 term 通知当成当前通知。

### 6.3 撤销顺序

撤销必须先抬 watermark、清缓存、丢迟到回复，再处理在途：

1. core 持久化撤销（入口 Key 或登录引用失效）。
2. 提高对应 watermark（`auth_generation` 或入口 Key `generation`）。
3. 清理旧 auth 缓存。
4. 丢弃 epoch/term/generation 不匹配的迟到回复。
5. 按在途策略处理已授权请求。
6. 当前实例返回屏障 ack。

线性化点：core 持久化撤销 **并且** 收到当前实例屏障 ack。未知或失联只记录 `pending`，不能把删除、轮转或取消报告为完成。owner 失效立即阻止新请求；重连必须先 reconcile 撤销屏障。

### 6.4 在途请求

首期候选策略：未受撤销影响的已授权在途请求继续到客户端取消或自然结束。若上游登录材料或入口 Key 已明确撤销，则 core 发出取消指令，扩展终止对应请求，禁止换池或重放。输出已经开始的请求、工具调用及其他执行结果不确定的模型请求不得自动换成员或重放。回退只改变后续请求。

| 迟到事件 | 处理 |
|---|---|
| 旧 `auth_generation` 的 `ResolveAuth` 回复 | 比较 epoch、term、generation 与 watermark；不匹配即丢弃 |
| 旧 `generation` 的 `ValidateIngressKey` 回复 | 丢弃并让该 HTTP 请求重新经当前 core 校验 |
| 旧 owner 的撤销/取消通知 | 旧 term 永久拒绝；当前 owner 先 reconcile 撤销屏障，再处理在途 |
| core 失联或状态未知 | 仅记 `pending`，阻止新请求，不能把删除、轮转或取消报告为完成 |

## 7. reconcile 决策表

core 在握手、重连、提交超时、崩溃恢复后按本表核对，不得猜成功。

| 观察 | 决策 |
|---|---|
| 同 `request_id` + 同 `payload_hash`，操作 `in_progress` | 等待或再次 `GetOperation`，不换 id |
| 同 `request_id` + 不同 `payload_hash` | 拒绝 `payload_mismatch` |
| `GetOperation = unknown` 且 `Status` 无对应 prepared/active | 记 `pending`/`retryable`，不重放模型请求 |
| `GetOperation = prepared` 且 revision/hash 等于目标 | 只允许随后的 `CommitDesired`，不得把准备当成功 |
| `GetOperation = committed` 且 `active` 等于目标、`prepared` 已空、生命周期可服务、端口就绪 | 才报告运行提交成功 |
| `GetOperation = committed` 但生命周期不可服务 | 不显示 running；先恢复 owner 与监听 |
| `GetOperation = aborted` 或 `expired` | `active` 保持原值；需要时重新 prepare |
| `active_revision` 高于请求版本 | 低版本拒绝；不得回滚已成功版本 |
| 同版本异 `hash` | `hash_conflict` |
| `base_revision` ≠ `active_revision` 或 `expected_epoch` 不匹配 | `base_mismatch` |
| 新 epoch、`active = null` | 只允许 bootstrap → commit；不继承旧 actual |
| 同 epoch、新 `owner_term`、保留 `active(v)` | 清缓存、对账屏障，再 prepare 前向 `w`；禁止 bootstrap |
| 旧 epoch/term 的任何控制或异步回复 | 永久拒绝 |
| core 失联 | 拒绝新 HTTP；在途按第 6.4 节；撤销结果保持 `pending` |
| 端口探测仍占用而 `Stop`/`Drain` 自称完成 | `needs_attention`，禁止并行 bind |
| 补偿将撤销，但已观察到更新的成功 `active` | 不得撤销后来成功的新配置 |

## 8. 操作留存

运行记录只保存非敏感操作元数据、结果和版本，设容量与保留期。它不是第二份领域状态，也不保证跨崩溃 exactly-once。模型 POST 不使用这套控制重试。

| 项 | 规则 |
|---|---|
| 可存字段 | `operation_id`，`request_id`，`payload_hash`，消息种类，`config_revision`，`hash`，`instance_epoch`，`owner_term`，`owner_id`，`operation_state`，起止时间，脱敏 `code`，`in_flight_count` 快照 |
| 禁止字段 | 入口 Key、登录材料、模型正文、工具参数、`prepare_token` 明文（只许指纹） |
| 保留期 | 明确有界；提案未测，实现前再定容量。查询命中保留期之外视为 `unknown`，必须改核对 `Status` |
| 幂等 | 保留期内相同 `request_id` + 相同 `payload_hash` 可查询或重试；相同 id 不同 payload 拒绝 |
| 崩溃 | 新实例新 epoch；旧操作记录不作为新实例正在运行的证据 |
| 待完成操作 | 由 core 存在既有应用存储；本轮不改库表。扩展侧留存只服务 GetOperation |

失败补偿：任一步失败先查询实际结果，再逆序恢复。core 负责恢复文件和领域状态；扩展补偿必须比较实例及实际版本。程序不可达或实例已换时，记录待恢复及原因，显示 `retryable` / `needs_attention`，重连后按第 7 节核对。

## 9. A 阶段退出门槛清单

对照提案第 9、12 节。纸面契约仍只冻结候选产品接口，**不过产品 B**：不做插件平台、不把隔离切片当成 live sidecar 或接到真实应用目录的产品 `CommitDesired`。A 仍缺的过程证据（旧实现故障日志、独立审查、实测性能）可以继续补。隔离 desired-config + Messages 切片已落地，不等于 A 过程门槛已通过。

### 9.1 提案第 9 节 A 行

| 完成证据 | 本契约 |
|---|---|
| 能按旧实现复现并分类故障 | **仍缺、因此不能进 B**：本页按现有调用链分类了启停、补偿、端口占用、排空持锁与不确定结果，但没有真实运行日志复现 |
| 明确租约、版本、补偿和锁顺序 | **已纸面冻结**（第 5–8 节） |
| 不改领域行为 | **已纸面冻结**：纸面契约不改 Rust/TS/库表 |

### 9.2 提案第 12 节 A 行与共用字段

| 交付 | 本契约 |
|---|---|
| 窄运行接口、状态/错误模型 | **已纸面冻结**（第 2–4 节） |
| 锁顺序设计，不拆整个 core | **已纸面冻结**（第 5 节） |
| owner/auth generation 作用域、接管清缓存、撤销 watermark | **已纸面冻结**（第 6 节） |
| 操作留存与 reconcile 决策表 | **已纸面冻结**（第 7–8 节） |
| fixture/故障与隔离证据契约 | **已纸面冻结**（第 10 节，只写契约） |
| `revision` 存储归属及迁移方案 | **已纸面冻结**（第 4.3、5、8 节）：唯一 core 使用既有应用存储；A 不改库。物理表/列与迁移实现在 D 写入整合前另列任务 |
| 独立验证 probe 脚本（`scripts/route-runtime-probe/`） | **隔离 desired-config + Messages 已有**：`messages-isolated.sh` 覆盖 Handshake / owner / Bootstrap / Commit / Prepare / Abort / GetOperation / Status 与合成 Key Messages。A 仍缺三协议、故障、撤销、脱敏与打包证据 |
| 按旧实现实测性能阈值、并发档位、24h/1000/20 长流数值确认 | **仍缺**：提案写明当前不是已测结果 |
| A 交付后的独立审查 | **仍缺**：提案要求独立审查后才能进入产品 B |

B 的 Windows 只读 IPC 实验属于 A 过程门槛通过后的下一阶段，不得反过来成为 A 的退出条件。

### 9.3 本页明确排除

隔离 Go desired-config 切片已经存在，不在此重复设计。不做插件平台 / SDK / ABI；不把隔离 `CommitDesired` 接到真实应用目录；不改回收站策略；不改产品边界；不改发版；不打 tag；不合 `release`；不把本页其余候选说成已经接到产品；不把隔离切片说成 live sidecar。

## 10. fixture 对照、故障计划、隔离路径与证据格式

本节只写契约。不接真实用户数据，不修改宿主用户级配置。隔离 desired-config probe 已有 `scripts/route-runtime-probe/messages-isolated.sh`；A 所需的三协议 / 故障 / 撤销 / 脱敏脚本仍缺。临时产物仍落在运行时 scratch（脚本使用 `/tmp/agenthub-route-runtime-probe/<run_id>/`），不是产品数据目录。

### 10.1 对照规则

| 项 | 契约 |
|---|---|
| 对照对象 | 旧进程内 Rust 实现与未来独立程序必须使用同一 fixture、schema 和请求序列 |
| fixture 内容 | 合成入口 Key 引用、合成上游、固定模型名单、预声明的动态 id/时间字段白名单 |
| 规范化 | 只允许预先声明的动态 id、时间等字段；不能删除事件顺序、工具次数或错误语义 |
| 裁决 | 差异按[本机路由 API](../reference/local-route-api.md)与[路由兼容性](../reference/route-compatibility.md)现行契约裁决；源码中的错误不能直接抄成 golden |
| 正确性重复 | C 阶段正确性用例至少 3 遍；并发档位候选 `1/4/16`，由后续实测冻结 |
| 性能 | 先测旧版再冻结并发档位、平台硬件和预算；C 的结果不能反过来调高阈值。E 的 24 小时 / 1000 请求 / 20 条 5 分钟长流 / 关键崩溃 3 遍均为未测候选 |
| 已有 preflight | `scripts/route-messages-preflight.sh` 只覆盖模型列表与 Messages 两轮，只能以合成参数和受控上游纳入证据 |
| 真实上游 | 分别运行，禁止镜像用户请求到两个上游；真实付费上游不用于千次压测 |

禁止把真实登录、真实 Agent 用户目录、未列出的外网访问写入 fixture。

### 10.2 故障注入计划

与提案第 10 节注入表一致，作为 A 阶段纸面计划：

| 注入点 | 预期 | 必须留存的证据字段 |
|---|---|---|
| `prepare` 失败 | `active` 不变，清理过期 prepared/临时监听 | 操作状态、端口探测、清理结果 |
| `save` 失败 | `prepared` 可撤销，领域写入回到原状态 | core 持久化记录、回滚核对 |
| `commit` 丢 ack | 查询后只接受已核实的 active，未知则 pending | operation/status 对照、重连核对 |
| core EOF | owner 失效、拒绝新请求、排空并释放资源 | epoch/term、请求计数、端口释放 |
| core 重启而 runner 仍存活 | 旧 term 不可用；核对排空与屏障，基于保留版本准备新前向 revision，commit 后才服务 | 新旧 term、active/prepared、迟到指令拒绝与重新激活结果 |
| runner 崩溃 | 无自动重放，确认失败进程资源释放后再恢复 | 崩溃日志、恢复 epoch/revision、清理结果 |
| Key 迟到回复 | 丢弃旧 generation，保持撤销 watermark | generation、watermark、请求拒绝结果 |
| 升级失败 | 新 runner 释放后旧包新 epoch 恢复 | pending upgrade、包身份、回退核对 |

### 10.3 隔离路径

C/D 若将来运行，必须满足：

| 项 | 约定 |
|---|---|
| 应用数据 | 绝对临时 `AGENTHUB_HOME`，与宿主 `~/.agenthub` 分离 |
| 端口 | 临时端口，非默认网关口 |
| 上游 | 受控 loopback 上游 |
| 范围 | `AGENTHUB_HOME` 只隔离应用数据，不隔离 Agent 家目录 |
| 启动前清单 | 必须列出实际所有数据、Agent 配置及日志读写路径，规范化后逐一验证处于 scratch 范围内 |
| 执行文件与系统库 | 另设只读允许清单；不因此放开宿主登录文件 |
| 越界 | 发现路径越界或没有路径隔离能力，则不得运行 D，改用独立测试用户或其他隔离环境 |
| 自动 probe | 禁止未列出的外网访问、真实登录和真实 Agent 用户目录 |
| 测试 Agent | D 只验证测试 Agent；不写真实用户配置、不接管默认网关 |

### 10.4 证据格式

每次运行记录（隔离 Messages probe 写入其 scratch `evidence.json`；其余 A 证据工具尚未创建）：

| 字段 | 要求 |
|---|---|
| `build_sha` | 实际构建提交 SHA，不能把基线 tag 当作新版测试结果 |
| `dirty_fingerprint` | dirty 补丁指纹 |
| `platform` / `arch` | 目标平台与架构 |
| `run_id` | 本次运行 id |
| `fixture_version` | fixture 版本 |
| `active` / `prepared` | 运行时核对到的版本与 hash |
| `instance_epoch` / `owner_term` | 当时的实例与 term |
| `injection` | 注入点名 |
| `result` / `cleanup` | 结果与清理情况 |
| `stdout` / `stderr` | 写入前先脱敏 |

日志、status 和控制记录扫描 Key 与正文，扫描失败即阻断；不得记录真实 prompt 或工具参数。单平台通过不能代表整体通过。

遵守根 [AGENTS.md](../../AGENTS.md)：不编写或执行 Rust 测试。本页作为纯方案修改，验证只运行 `pnpm check:docs` 和 diff 检查。

## 相关页面

- [路由官方扩展与独立运行方案](adapter-sidecar.md)：本契约所对齐的提案。隔离 scratch 下的 desired-config 控制已经落地；默认网关切换、真实配置写入、插件商店和 E/F 仍不在本页授权。2026-10-03 的 no-go 原文见 [归档评估](../archive/official-extension-go-slice-review-2026-10-03.md)。
- [模块化与边界收紧](modularity.md#8-功能模块与官方扩展)：功能模块与官方扩展的产品对齐。
- [本机路由 API](../reference/local-route-api.md)与[路由兼容性](../reference/route-compatibility.md)：现行 HTTP 契约，对照时的裁决真源。
- [产品边界](../decisions/product-boundaries.md)：本页不改变的产品规则。
