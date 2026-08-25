# 多账号轮询与故障切换 RFC（C2 设计稿）

> **Archived / 已归档**: Historical record. Do not use as current implementation contract or TODO list.
> **Status**: archived historical record
>
> **归档（2026-08-24）**。产品方向仍以 [../reference/route-compatibility.md](../reference/route-compatibility.md) 的兼容矩阵为准（历史稿 §5.5）。内核（AccountPicker / 请求边界 FSM）已在 `bridge/account.rs`；`multi_account` 门默认关，不是「零实现」，也还没对用户开闸。
>
> 原状态：**设计稿（2026-08-22）**。
> 产品真源：[../reference/route-compatibility.md](../reference/route-compatibility.md)；原文 §5.1.2、§5.3、§5.5 的历史上下文保留在本记录中。
> 任务卡（历史）：[routing-connection-refactor-plan.md](routing-connection-refactor-plan.md) C2。读模型已落地（C1 `TicketSurfaceGroup`）。
> 借鉴：one-api / new-api 渠道轮询与自动禁用恢复、nginx upstream 健康剔除、LiteLLM router fallback。**只借故障隔离与有序回退；明确排除权重、压力分配、least-conn、余额调度。**

## 0. 范围

适用：③ `local_bridge` 各边。① 直连与 ② 写原生槽不涉及。

做：同票面有序成员、固定顺序轮询、请求边界 / 首个有效流事件前故障切换（单请求最多一次）、成员健康隔离与恢复、按实际承接账号审计与身份头。

不做：负载均衡 / 权重 / 冷却池；国产 OAuth；凭据加密；改 `derive_bindings`；公网 / 多人共享；sidecar 迁移。

## 1. §5.5 六条不变式（实施对照）

| # | 不变式 | 本设计如何守 |
|---|---|---|
| 1 | 仅本人账号、仅本机 loopback；不对外、不转售 | 成员只来自本机钱包 C1 组；listener 仍只绑 127.0.0.1 |
| 2 | 切换只在请求边界或首事件前；单请求最多一次 | `AccountSwitchGate` 与 `RetryGate` 正交；`EmissionState::Emitted` 后两闸都关 |
| 3 | 每次请求记录实际承接账号；身份头 / 会话 seed 按该账号，不串号 | 日志 `account_id`；failover 后重算 auth、Grok replay、session hash（混入 `account_id`） |
| 4 | refresh owner 分治不变（§5.1.2） | 每成员独立 `oauth_refresh_lock(account_id)`；CLI 跟随 vs Hub PKCE 按该行 `extra.source` |
| 5 | 失败只标该成员，不向调用方暴露其余账号 / token | 下游仍是稳定 502/`NeedsLogin` 文案；隔离态写在该 account 行健康 |
| 6 | 每条边随 fixtures 取证后才开轮询 | 矩阵 cell 增加 `multi_account` 门，默认关；开边 ≠ 开轮询 |

## 2. 成员存储：三选一

| 方案 | 做法 | 优点 | 代价 |
|---|---|---|---|
| A. profile 多 `source_id` | `adapter_profiles.source_ids: Vec`（或 JSON），`source_id` 保留 lead | 绑定快照稳定；重启不丢池 | schema 变更；后导入的同票面账号**不会**自动入池，需 rebind |
| B. 新成员表 | `bridge_pool_members(profile_id, ticket_id, ord, isolated)` | 游标 / 隔离可落盘；多对多干净 | 新表 + 新写口；C1 已禁止为读模型建表，C2 再开表偏重 |
| **C. 运行时纯读模型（选定）** | bind 仍挂一张 lead 票；启动 / 每请求从 C1 `TicketSurfaceGroup` 取同 `(surface, credentialClass)` 成员 | 无新表、不去重语义不动；新导入的同票面账号自动入池；C1 可直接喂给 AccountPicker | 游标不落盘（重启从 0）；不能把池缩成子集 |

**选定 C**，理由：

1. §5.5 的「同一票面可挂多个自己的账号」按钱包聚合理解，而不是再做一次显式 attach UI（C3 只展示，不管理大盘）。
2. 现有 `AdapterProfile.source_id` 继续表示 **bind lead**（`plan`/`unbind`/投影仍指向它）。运行时把 lead 映射到其所在 C1 组，组内全员可被 picker 选中。
3. 成员健康已有 account 行 `AuthHealth`（`Renewable` / `NeedsLogin`），隔离不必新列。
4. 若日后需要「这条路由只用其中两号、第三号永不进池」，再升级为 A（在 profile 上钉 `source_ids`）。升级路径：C 的 lead + 组 → A 的显式列表，不改下游 FSM。

**不选 A/B 的否决点**：A 让后导入账号进不了已绑定的桥，和「同票面自动轮询」相反；B 为小 N（个位数账号）引入表与 CRUD，且与 C1「不加新表」同轮冲突。

绑定语义（C2 落地时）：

```text
bind(ticket, agent) 仍是单票写入（lead）
  local_bridge 启动时：
    group = wallet.surface_groups.find(ticket.surface, ticket.credentialClass)
    members = group.members   // 若 multi_account 门关：仅 lead 自己
    BridgeStartSpec.members = resolve_auth(members)
```

① / ② 路径忽略组。`unknown` 与投影 Provider 已在 C1 排除。

## 3. AccountPicker

挂在 **边**（profile / 未来 unified-gateway 的 edge），不是挂在全局钱包。

```text
AccountPicker {
  members: Vec<PickedMember>   // C1 顺序：ticket_id 字典序
  cursor: AtomicUsize          // 下一发新请求从这里取
}

PickedMember {
  ticket_id, source_kind, source_id, label
  auth: ResolvedAuth           // 每成员一份，可 in-place replace
  reload: Option<UpstreamAuthReload>  // 闭包钉死该 account_id
  health: AuthHealth           // 启动时快照；失败路径回写 account 行
}
```

### 3.1 固定顺序游标

- 新请求：从 `cursor` 起向后找第一个 **可接单** 成员（健康 ∈ {Renewable, Unknown 且非 NeedsLogin}，且有可解析 secret），取到后 `cursor = (idx+1) % n`。
- 不是加权、不是随机、不是 least-conn。one-api 的 priority 渠道表只取其「有序列表 + 跳过禁用」，不取其权重。
- 重启：cursor=0。可接受：本产品不做均衡，从组头再走一圈不影响正确性。

### 3.2 健康态

沿用现有 `AuthHealth`，不新增枚举：

| 态 | picker |
|---|---|
| `Renewable` | 可接单 |
| `NeedsLogin` | **隔离**：跳过，直到用户同步 / 重登 / live reconcile 把该行拉回 |
| 其它（Unknown / NeedsAttention） | 可试一次；该请求内 401 且 refresh 无效 → 标 `NeedsLogin` 并隔离 |

隔离 = 写该 account 的健康，**不是**从 C1 组删行（C3 置灰可见）。恢复 = 现有 list reconcile / 「同步当前登录」/ Hub PKCE refresh 成功。nginx `max_fails` + `fail_timeout` 只借「剔除不健康、健康后再进组」；**不做时间窗冷却池**（范围外）。

API Key 成员没有 OAuth refresh：上游持续 401 → 标 `NeedsLogin`（或等价「Key 失效」），同样隔离。

### 3.3 失效隔离与恢复

```text
成员 M 在本请求被判失效
  → 只更新 M 的 AuthHealth = NeedsLogin
  → 不改其它成员 token / 健康
  → 不把 M 的 id 以外的账号写进下游错误体
  → 下一新请求自然跳过 M
M 恢复（reconcile 后 health ≠ NeedsLogin）
  → 重新可接单；不必重 bind、不必重启 listener
```

LiteLLM 的 fallback 列表只借「按序试下一个」；不借 cooldown / routing strategy。

## 4. 请求边界 FSM（与 RetryGate 正交）

现状（单账号）：`RetryGate { max_attempts: 2 }` + `EmissionState`。首事件前上游 401 且 OAuth 协议：`try_reload_upstream_auth`（§5.1.2 owner 分治）成功换 bearer 则再 POST 一次。Grok 400 encrypted-reasoning strip 是**同账号**恢复，不算切号。

C2 增加平行的 **账号闸**，不得把切号塞进 `RetryGate.max_attempts`。

```text
RequestOpen
  member = picker.pick_new()           // 游标前进
  identity = build_identity(member)    // 头 / seed 钉死该账号
  switch_used = false
  retry_used  = false                  // 同账号 401 reload

loop:
  POST upstream(member.auth, identity)
  if success → 进入流；observe EmissionState
  if EmissionState == Emitted:
      禁止 RetryGate、禁止切号、禁止重放
  if 401 && OAuth && !retry_used && RetryGate.can_retry(Idle, Transient, 0):
      retry_used = true
      按该成员 owner 分治 reload；token 变了则 continue
      token 未变 → 视为该成员失效，落入切号判定
  if 可切号 && !switch_used && EmissionState == Idle:
      switch_used = true
      标当前成员 NeedsLogin
      next = picker.failover(from=member)   // 组内下一可接单，不再前进「新请求游标」
      若 next 为空 → 502，不泄漏其它账号
      member = next
      identity = build_identity(member)     // 必须重算，禁止沿用上一号 session/replay
      retry_used = false                    // 新账号允许自己的一次 401 reload
      continue
  其它错误 → 映射现有 upstream_error；不切号
```

**可切号** 条件（全部成立）：

1. `EmissionState::Idle`（尚未向下游写出有效事件）
2. `!switch_used`（本请求还没切过）
3. 该边 `multi_account == true` 且组内还有另一可接单成员
4. 当前失败属于账号级：NeedsLogin / 持续 401（reload 无效）/ 健康探针失败。**不是** 429、5xx、协议 400、Grok reasoning decode

429 / 过载走现有 `Retry-After`，留在当前账号。Grok strip 重试留在当前账号，不占切号次数。

单请求上限：**切号 0 或 1**。同账号 401 reload 仍最多 1。最坏路径：A reload 一次 → 切 B → B reload 一次。再失败则 502。

首事件后：流中取消只取消当前上游；不断流换号（§5.3 工具闭环与用量归属）。

## 5. 审计与身份（不串号）

### 5.1 日志

每请求结束一条（失败必打，成功按 [logging.md](../reference/logging.md) 现口径）：

| 字段 | 值 |
|---|---|
| `profile_id` | 现有 |
| `request_id` | 现有 |
| `account_id` | **实际承接**的 `source_id`（切号后是 B，不是 lead） |
| `ticket_id` | `account:<id>` / `provider:<id>` |
| `failover` | bool |
| `failover_from` | 仅 failover 时写被切走的 `account_id` |

禁止：token、refresh、完整 credentials。`account_id` 是行 id，可检索。

### 5.2 上游身份头 / 会话 seed

今日 Grok 身份头是 CLI 静态对（`x-xai-token-auth` 等），`session_id` 从客户端 cache seed 哈希，**未混入账号**。多账号下同一 Claude 会话切到另一 Grok 号会把 A 的 prompt cache / `encrypted_content` 打到 B —— 这就是串号。

C2 规则：

1. `ResolvedAuth` 必须是当前成员的；failover 后 `replace_token` 只换 B，A 的 cell 不动。
2. `grok_session_id(seed)` 改为 `grok_session_id(seed, account_id)`（或等价 mix）。客户端 seed 仍用于「同一用户会话」，账号维保证缓存分区。
3. `GrokReasoningReplay` 按 `(account_id, seed)` 分区。failover 发生在首事件前：丢弃本请求已按 A 准备的 replay，按 B 重建（通常为空）。
4. 其它协议的上游 header（Anthropic `x-api-key`、Codex bearer）一律从当前成员解析，不得缓存跨成员。

## 6. refresh：每成员独立 single-flight

现状：`oauth_refresh_lock(account_id)` + `live_reconcile_lock(agent)`；CLI-owned 不调 token 端点，Hub-owned 只写账户池（§5.1.2）。

C2：

- 每个 `PickedMember.reload` 闭包捕获**自己的** `account_id`，内部仍走 `AccountService` 现有 owner 分治。
- A 的 refresh 不持有 B 的 lock，不写 B 的行，不碰 B 的 CLI `auth.json`。
- 同账号并发请求共享该账号 single-flight（今日行为），跨账号并发各飞。
- refresh token 仍不进 bridge / IPC / 日志。listener 换上游 auth 时 **local bearer 不变**（锚点 `ensure_listener_replaces_upstream_auth_while_keeping_local_bearer` 继续有效；切号也不换 local bearer）。

## 7. 门禁：矩阵加维度，不靠 rule_id 白名单

比较：

| 做法 | 优点 | 缺点 |
|---|---|---|
| **cell 增加 `multi_account: bool`（选定）** | 与现有 gates 一样 fail-closed；B1 防漂移可把该位置进对账；开 `can_apply` 不必同时开轮询 | 要改 `AdapterCapabilityCell` + fixtures 断言 |
| 按 `rule_id` 白名单 | 改动小 | 第三真源；新边容易忘 |

默认 `multi_account = false`：即使 C1 组有 3 个成员，picker 也只保留 lead。取证清单（与该边原 fixtures 一起）：

1. 两成员固定序轮流承接新请求
2. A `NeedsLogin` → 本请求（若仍在首事件前且未切过）切 B；A 隔离
3. 首事件后 401 不切号
4. 单请求第二次切号被拒
5. 日志 `account_id` 为实际承接号、无 token
6. Grok/Codex 边：failover 后 session/replay 不串号

未取证保持关。`can_apply` 与 `multi_account` 独立：边可以单账号绑，但不能轮询。

## 8. 与统一网关 listener（A4）的两种对接

C2 依赖 A2 Transport 注入点；A4 可能延期。FSM / Picker 必须能在两种宿主下等价。

### 8.1 A4 已落地：统一 listener，下游标归属

```text
loopback :port
  → local bearer 识别边（profile / edge_id）
  → DownstreamSurface
  → edge.account_picker.pick / failover
  → UpstreamTransport(member.auth, identity)
```

游标、成员列表、admission 都挂在 **edge** 上。同一 surface 组绑到 Claude 与 Codex 是两条边、两份 picker，互不借用游标（避免用量归属搅在一起）。

### 8.2 A4 延期：per-profile listener（等价实现）

今日：一 profile 一 `ListenerState` 一 `ResolvedAuth`。

等价：

- `BridgeStartSpec.upstream.auth` 扩成 `members: Vec<PickedMember>`（单成员时退化成现在）。
- `ListenerState` 持有该 profile 的 `AccountPicker`。
- `handle_*` 在现有 RetryGate 旁接入 §4 FSM。
- secret 解析（`src-tauri/adapter_bridge_controller.rs`）对组内每个 `source_id` 调现有 resolver，**一次启动解析多份 access，refresh 仍按成员闭包**。
- local bearer / port / `listed_models` 仍按 profile，不按成员。切号不换端口、不换下游 token。

A4 到来时：把 `ListenerState.picker` 整块挪到 edge map，FSM 单测不用改。

## 9. 对 C1 读模型的消费约定

C1 已定（本分支代码）：

- 键：`(surface, credentialClass)`
- Account 与 Provider **混组**（同一票面可来自两张表）
- `unknown` surface / `unknown` credential class / 投影 Provider **不入组**
- 成员序：`ticket_id` 字典序（`account:` < `provider:`）
- 单成员也出组（池大小 1）

C2 不得改这套聚合；只允许在 picker 里再过滤 `multi_account` 与健康态。若日后改为 created_at 序，属 C1 修订，需两端（Rust + `groupTicketSurfaceMembers`）锁步。

## 10. 实施切片（拍板后，不在本分支做）

1. `PickedMember` + `AccountPicker` 纯结构 + FSM 单测（不接 host）
2. `BridgeStartSpec` / `ListenerState` 多成员；secret 解析循环
3. dispatch 合入 §4；日志字段
4. Grok identity mix-in `account_id`；replay 分区
5. 矩阵 `multi_account` + 契约测试；默认全关
6. 首条取证边（建议已有 fixtures 的 Grok/Codex OAuth `local_bridge`）再开门

依赖：A2（auth 注入点）、C1（已做）。A4 非硬依赖（§8.2）。

## 11. 否决清单（防止实施跑偏）

- 按余额 / QPS / 权重选号
- 流中切号或「悄悄换号继续 SSE」
- 一个账号 refresh 写另一账号行或 CLI 文件
- 把国产 OAuth 成员拉进组
- 为轮询监听非 loopback
- 用 `RetryGate.max_attempts` 兼做切号次数

---

## 附录 A. D3 绑定真相写面盘点（只盘点，不改写入）

三处「谁在用」：

| 名字 | 是什么 | `list_wallet` 是否读 |
|---|---|---|
| `TicketBinding` | `is_current` + `adapter_profiles` 经 `derive_bindings` | 是（这就是钱包「正用于」） |
| `ActiveBinding` / `agent_active_bindings` | 每 Agent 一行：`account_id` XOR `provider_id` | **否** |
| 前端 `connection-pool-store` | `listAccounts` + `listProviders` 缓存 | 否；过期时 UI 与钱包可能短窗口不一致 |

映射（契约测试使用，**不是**改 `derive_bindings`）：

- 钱包该 Agent 的 **active native** 票 `account:X` / `provider:P` ↔ 指针 `(account_id=X)` / `(provider_id=P)`
- 钱包 **active reshape/bridge** ↔ 指针 `provider_id = profile.generated_provider_id`（live 当前行是投影，票仍是 source）

`ConnectionService.get_active` **会按指针回写 `is_current`**（指针赢）。因此契约比较必须读 **裸 `ActiveBindingRepo.get`**，不能先 `get_active`（会治好漂移，测试变假绿）。

### A.1 绕过 `bind_ticket` 的写入口

| 入口 | 是否走 `ConnectionService` 双写 | 钱包派生 vs 指针 | 后续是否收口为 `bind(native)` |
|---|---|---|---|
| `AccountService.switch` | 是（`activate_account`，先写 live 再双写） | 维持：native account 票 = 指针 | 产品切换仍是账号池能力，不必强收口；Dashboard「正用于」已能对上 |
| `import_live` / `upsert_live_account(make_current=true)` | 是（`commit_authorization_merge` → `create_and_activate` / `update_and_activate`） | 维持 | 进口不是 bind；保持 |
| `import_live` Pi 多 provider（`make_current=false`） | 不激活 | 无指针变化 | 保持 |
| 建账号 `is_current=true`（无 adapter 的测试/内部路径） | 是（`create_and_activate_account`） | 维持 | — |
| `ProviderService` 切当前 / 建当前投影 | 是（`activate_provider` / `create_and_activate_provider`） | 维持：投影 current 时钱包走 reshape/bridge 映射 | apply 生成投影必须继续走这条，不能只改 `is_current` |
| `TicketBindService.bind` reshape | 是（`apply_generated` → provider switch） | 维持 | 已是产品写口 |
| `TicketBindService.bind` Codex 官方自绑定 | 是（转 `accounts.switch`） | 维持 | 已是 bind |
| Tauri `apply_adapter` | 是（薄委托 `bind_ticket`） | 与 bind 相同 | 兼容口，不必再写一套 |
| Desktop `local_bridge` host saga | 是（起桥后切生成 provider） | 维持 | 保持 host 拥有 listener |
| `AccountRepo.update` / `ProviderRepo.update` 改 `is_current` | **否** | **会漂**（D3 探测器：`ticket_connection_wallet_vs_pointer_detects_one_sided_is_current_drift`） | 生产路径不应直接打 repo；测试与误用除外 |
| `ActiveBindingRepo.upsert` | 只写指针 | **会漂**（另一侧） | `pub(crate)`，生产应只经 `ConnectionService` |
| `ConnectionService.get_active` | 以指针为准 **修复** `is_current` | 读时愈合，掩盖钱包已展示的漂 | 不要把它当契约比较的读口 |
| 前端 `connection-pool-store` | 不写 DB | 缓存 stale ≠ DB 漂 | reload 队列已有；不在本轮改 |

结论（供主 Agent）：生产写面里，**switch / import activate / apply_adapter / bind / provider 切当前** 都已经双写，派生一致性在「经 ConnectionService」时成立。真正的漂来自 **只打 `is_current` 或只打指针表**。不建议本轮把 `switch` 收成 `bind(native)`——语义不同（换本 Agent live vs 把票接到另一 Agent）。建议后续只加：禁止业务路径直接 `AccountRepo.update(is_current)`；契约测试保持红灯能力。

漂移演示（PR 描述用，提交树里的测试是探测器本身为绿）：

```text
# 绿：官方 activate 后派生 == 指针
cargo test -p agenthub-core ticket_connection_active_binding_matches_wallet_native_account -- --exact

# 把 ticket_connection_wallet_vs_pointer_detects_one_sided_is_current_drift
# 末尾 assert_ne! 改成 assert_eq! 后应红：
#   left:  (Some("acc-b"), None)   // 钱包已跟 is_current
#   right: (Some("acc-a"), None)   // 指针未动
# 演示完还原 assert_ne!
```

### A.2 前端第三真相

`connection-pool-store` = accounts+providers 列表缓存，**不是** `ConnectionService`，也不是 `TicketBinding`。Connections 页「正用于」走 `list_wallet`；store 只给行编辑 / 徽章提供池数据。短窗口 stale 靠现有 reload，不在 D3 改写路径。
