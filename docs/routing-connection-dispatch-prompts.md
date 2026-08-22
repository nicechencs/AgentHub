# 路由 × 连接重构：派工提示词（worktree 多 agent 并行）

> 一次性派工文件，任务全部完成后删除（[docs/README.md](README.md) 管理规则）。
> 任务卡与依据见 [routing-connection-refactor-plan.md](routing-connection-refactor-plan.md)；本文把 12 张任务卡按**文件不相交**原则合并成 4 个可同时开工的 agent 包，每份提示词自包含、可直接粘贴。

## 0. 派工前准备（主线操作，一次性）

1. **先把计划文档提交到 `dev`**（`docs/routing-connection-refactor-plan.md` + `docs/README.md` + `docs/provider-api-oauth-adaptation.md` 的指向改动），否则新 worktree 里读不到任务卡。
2. 为每个 agent 建 worktree（PowerShell，在 `d:\demo_chen\2026\AgentHub` 执行）：

```powershell
git worktree add ..\worktrees\agenthub-bridge  -b refactor/bridge-gateway      dev
git worktree add ..\worktrees\agenthub-edges   -b refactor/edge-rules          dev
git worktree add ..\worktrees\agenthub-members -b refactor/ticket-members      dev
git worktree add ..\worktrees\agenthub-cleanup -b refactor/connection-cleanup  dev
```

3. 每个前端相关 worktree 内单独 `pnpm install`（node_modules 不共享）；Rust 首次 `cargo test` 会全量编译，属正常。
4. **合并顺序**（主 Agent 负责，agent 不互相合并）：P4、P2 小件先合 → P1 rebase 后合（P1 与 P2 在 `adapter_bridge_service` 有小面积交叠，P1 负责 rebase 消解）→ P3。
5. 汇报模板（四份提示词共用，写在各提示词末尾）。

---

## P1 · 路由网关分层重构（泳道 A 全量）→ `refactor/bridge-gateway`

```text
【角色】你是 AgentHub 的 Rust 重构工程师，在独立 git worktree 中工作，分支 refactor/bridge-gateway（基于 dev）。工作目录：d:\demo_chen\2026\worktrees\agenthub-bridge。只在本分支提交，不合并、不碰 dev/release，不改 worktree 外的任何文件。

【必读】开工前通读：仓库根 AGENTS.md；docs/provider-api-oauth-adaptation.md §5.2–§5.5（拍板真源）；docs/routing-connection-refactor-plan.md 任务卡 A1–A4；docs/adapter-design.md §5–§6；docs/logging.md；docs/testing.md。

【目标】把本机路由运行时从「胖 dispatch」重构为拍板的分层结构，最终形态：

  统一 loopback 网关（本轮仍进程内 BridgeRuntimeHost，为未来 agenthub-adapterd sidecar 做准备）
    → local bearer 鉴权（唯一 middleware；bearer 识别「边」）
    → DownstreamSurface 分派（/v1/messages | /v1/responses | /v1/chat/completions | /v1/models）
    → ProtocolKernel IR（纯映射；Responses↔Responses passthrough 显式声明，不再隐式绕过）
    → UpstreamTransport（按边封装 auth、上游 path、body 塑形、恢复策略、流编解码；Grok 身份头/encrypted-reasoning/会话 seed 全部收进 Grok transport）

现状问题（已审查确认）：crates/agenthub-core/src/bridge/host/dispatch.rs ~1573 行集中了鉴权、admission、协议分支、上游 HTTP、Grok 特例、三套流编解码；三个 handler 模板三份复制；一 profile 一 listener 一 surface；上游 path 字符串与常量散落；OpenAI→Codex 复用 KimiChatCompletions 枚举名语义误导（可改名，序列化格式不变）。

【重设计自由度】bridge/ 内部结构你可以完全重排：模块划分、内部类型（含 ListenerState、BridgeStartSpec 的内部组成）、host 测试的组织方式都不必迁就现状，按最优方案设计。鼓励借鉴成熟网关的分层：LiteLLM proxy 的 Router/provider-adapter 分层、one-api / new-api 的「渠道」抽象（上游=可插拔渠道）、claude-code-router 的 Claude 下游网关形态。只借设计思想，不引入新依赖、不复制代码。

【分阶段交付】按可独立验收的提交推进，每步测试全绿再进下一步：
  1. 统一 handler 模板 + 抽 DownstreamSurface（行为完全不变，错 surface 404 契约保持）；
  2. UpstreamTransport trait（dispatch 内不再有按协议的 match 塑形/auth 分支；send_upstream_with_grok_recovery 下沉为 Grok transport 恢复策略）；
  3. passthrough 显式化（transport 声明能力，代码只有一个声明点）；
  4. 统一网关 listener：多 profile 共享 listener、bearer→边识别、端点→surface 分派。第 4 步动手前先在分支内写一页设计稿（docs/ 不动，放 PR 描述或分支内 DESIGN.md），必须回答：存量 profile 已写进目标 Agent 配置的各端口 loopback URL 如何兼容迁移（迁移或兼容期双听二选一）、/v1/models 按 bearer 合成、health 语义、admission 从 per-listener 改 per-edge。设计稿未想清楚就停在第 3 步交付，不要硬上。

【不变式（外部契约，违反即返工）】
  - 仅监听 127.0.0.1/::1；有绑定才起；对外只发 local bearer。
  - 鉴权语义与错误码族不变：本机 401 invalid_api_key、错边 404、过载 429 bridge_overloaded + Retry-After、上游失败 502 upstream_error。
  - /v1/models 本机合成 + fail-closed（§5.1.3），不透传上游。
  - RetryGate 语义不变：首个有效流事件前最多一次上游 401 换 token 重试；换上游 auth 时 local bearer 不变（既有锚点测试 ensure_listener_replaces_upstream_auth_while_keeping_local_bearer 的语义必须保留，测试可搬家不可删义）。
  - refresh token 不进 bridge；token/正文不进日志；日志字段口径按 docs/logging.md（target=core.adapter，code/op/profile_id/request_id/elapsed_ms 等）。
  - bridge/protocol/ 内核与 fixtures 是安全网：API 可调整，fixtures 断言的 wire 行为不得改变，protocol 测试必须全绿。
  - Tauri 命令签名、adapter_profiles 持久化 schema 不破坏（如第 4 步确需 schema 迁移，写进设计稿等主 Agent 拍板，不先动手）。
  - host 保持 Tauri-neutral（不新增 tauri 依赖），控制面继续走 adapter_control 契约——这是未来 sidecar 迁移的前提。

【范围外（一律不做）】负载均衡；多账号轮询（另一 agent 负责设计）；sidecar 二进制/IPC；国产 OAuth 开边；凭据落盘加密；/v1/embeddings、/v1/images/*、/v1/realtime；打开任何边的 canApply；公网监听。

【测试与提交】测试与生产分文件（docs/testing.md）。每阶段提交前跑：cargo test -p agenthub-core bridge 与 cargo test -p agenthub-core protocol，全绿才提交；第 4 步另跑 pnpm test -- bridges 确认前端状态页不回归。commit message 用英文、按阶段一条，说明「why」。

【汇报】完成或受阻时回报：分支名与提交列表；每阶段测试结果原文（含用例数）；关键设计决策与放弃的备选；dispatch.rs 重构后行数与模块清单；第 4 步设计稿全文；未尽事项。
```

---

## P2 · 规则单一真源 + Claude 订阅 → Codex 新边（泳道 B）→ `refactor/edge-rules`

```text
【角色】你是 AgentHub 的 Rust 工程师，在独立 git worktree 中工作，分支 refactor/edge-rules（基于 dev）。工作目录：d:\demo_chen\2026\worktrees\agenthub-edges。只在本分支提交，不合并、不碰 dev/release。

【必读】仓库根 AGENTS.md；docs/provider-api-oauth-adaptation.md §4、§5.1–§5.4、§7（规则与门禁真源）；docs/routing-connection-refactor-plan.md 任务卡 B1–B2；docs/testing.md。

【背景】当前 local_bridge 的边有两处真源要人工对齐：domain/protocol_graph/adapter_capability_matrix.rs 的矩阵 cell（transport/protocol）与 services/adapter_bridge_service 的 LIVE_BRIDGE_RULES（local_surface/upstream protocol/默认模型）。新边要改两处 + dispatch match 臂，漂移即「analyze 可写、runtime 错 surface」。另外 Claude 订阅 → Codex 已于 2026-08-21 从「产品不做」改判为 ③ 可路由（方向开放），当前矩阵无 cell，decide_adapter_capability 仅硬编码特判 reason，无 transport、无 fixtures。

【任务 1 · 规则单一真源（B1）】消除双真源。允许重设计声明方式，不必迁就现状——推荐方向：一张「边登记表」作为唯一声明点，矩阵 cell 与 LIVE_BRIDGE_RULES 都由它派生或用契约测试逐项对账（rule_id、上游协议、local_surface、默认模型）。若派生改造波及面过大，本轮退守为完备的契约测试 + 登记表设计稿。约束：不改任何边的开放状态与 plan 对外结果；既有防漂移测试 open_matrix_cells_have_bind_and_apply_arms 保持通过。

【任务 2 · Claude 订阅 → Codex 边落地（B2，先做 kernel/fixtures 腿）】
  - 方向：下游 OpenAI Responses（Codex 读 loopback）→ IR → 上游 Anthropic Messages OAuth（Claude 订阅 access token 注入；refresh 按 §5.1.2 owner 分治，本任务不实现自动 refresh）。
  - 交付：矩阵新增 experimental cell（gates 初始全关、canApply=false，reason 保持「规则与 fixtures 未落地」口径）；LIVE_BRIDGE_RULES（或新登记表）新行；bridge/protocol/fixtures/ 补 Responses↔IR↔Messages 该方向的正反例 fixtures（文本、多轮、tool call/result、usage、stop reason、错误、SSE 分片、Unicode）；decide_adapter_capability 的硬编码特判改为查表；src/dev/mocks/adapter/ 的 reason 与 adapter-capability-contract.json 锁步更新。
  - 明确不做：实机取证与打开 canApply（取证是后续独立步骤，需要真实 Claude 订阅账号）；thinking 签名块不伪造，上游无可验证签名时 fixtures 按降级关闭建模。

【范围外】国产 OAuth；负载均衡；凭据加密；App Server 边保持关闭；不动 bridge/host 运行时（另一 agent 在重构，避免冲突——你只允许改 adapter_bridge_service 的规则声明区，不改其 saga/materialize 逻辑）。

【测试与提交】提交前跑：cargo test -p agenthub-core adapter、cargo test -p agenthub-core protocol、pnpm test -- adapter，全绿才提交。B1 的契约测试要在 PR 描述里演示「故意改错一侧能红」后还原。

【汇报】分支与提交列表；测试结果原文；登记表/派生方案的设计说明；B2 新 cell 与 fixtures 清单；plan 对该边的 reason/maturity 展示截图或测试断言；未尽事项。
```

---

## P3 · 票面成员集 + 多账号轮询设计（泳道 C + D3）→ `refactor/ticket-members`

```text
【角色】你是 AgentHub 的 Rust + TypeScript 工程师，在独立 git worktree 中工作，分支 refactor/ticket-members（基于 dev）。工作目录：d:\demo_chen\2026\worktrees\agenthub-members。只在本分支提交，不合并、不碰 dev/release。

【必读】仓库根 AGENTS.md；docs/provider-api-oauth-adaptation.md §5.5（多账号轮询拍板）与 §5.1.2（refresh owner 分治）；docs/connection-binding-model.md；docs/account-authorization-pool.md；docs/routing-connection-refactor-plan.md 任务卡 C1、C2、D3。

【背景】账号池已支持同人多授权并存（去重只按 authorization_key）；但「同票面多账号」今天只是钱包里多张独立票，无聚合实体。§5.5 拍板：同票面可挂多个本人账号，网关固定顺序轮询 + 故障切换（切换仅限请求边界/首个有效流事件前、单请求最多一次），负载均衡不做。另外「谁在用」有三处真相：TicketBinding（is_current + adapter_profiles 派生）、agent_active_bindings（ActiveBinding）、前端 connection-pool-store 缓存，list_wallet 不读 agent_active_bindings，存在漂移风险。

【任务 1 · 票面成员集读模型（C1，落代码）】按 surface + credentialClass 聚合同票面多条 account/provider 行，产出读模型（如 TicketSurfaceGroup { surface, members[] }）。只做读模型：不加新表、不改去重语义、投影 Provider 与 unknown surface 不入组。聚合规则允许你重新设计（例如是否把 Provider 与 Account 混组、成员排序依据），以「§5.5 运行时能直接消费成员列表」为设计目标。如需暴露给前端，同步 contracts/ticket.ts 的 wire 映射与 dev/mocks/ticket.ts。

【任务 2 · 多账号轮询与故障切换 RFC（C2，只写设计稿不落代码）】在分支内新建 docs/multi-account-routing-rfc.md，给出完整设计，供主 Agent 拍板后另行实施。必须覆盖：
  - 成员存储：profile 多 source_id vs 新成员表 vs 运行时纯读模型，三选一并论证；
  - AccountPicker：固定顺序游标、成员健康态（Renewable/NeedsLogin）、失效隔离与恢复；
  - 请求边界 FSM：与既有同账号 401 reload（RetryGate）如何正交合入；首个有效事件后禁切；单请求一次上限；
  - 审计：每请求记录实际承接账号；上游身份头/会话 seed 按实际承接账号生成，不串号；
  - refresh：每成员独立 single-flight，owner 分治不变；
  - 门禁：每条边的轮询支持如何随 fixtures 取证开放（矩阵加维度 vs 按边白名单）；
  - 与「统一网关 listener」重构（另一分支进行中）的两种对接形态：统一 listener 下游标归属；若统一 listener 延期，per-profile listener 的等价实现。
  可借鉴：one-api / new-api 的渠道轮询与自动禁用/恢复、nginx upstream 的健康剔除思想、LiteLLM router 的 fallback 策略。只借设计，明确排除其中的负载均衡与权重机制（本产品拍板不做）。

【任务 3 · 绑定真相一致性契约（D3，C1 完成后做）】① 契约测试：同一 DB 状态下钱包派生绑定与 agent_active_bindings 指针一致，故意只改一侧时测试能红（PR 描述演示后还原）；② 写面盘点：列出仍绕过 bind_ticket 的写入口（AccountService.switch、import activate、apply_adapter 兼容口）及各自是否维持派生一致性，盘点表写进 RFC 附录。只加测试与盘点，不改任何写入行为。

【范围外】负载均衡/权重/冷却池；国产 OAuth；凭据加密；改 bridge/host 运行时（另一 agent 负责）；改 derive_bindings 语义；pool_crud/connection_service 拆文件（归 modularity-improvement 管辖）；公网/多人共享。

【测试与提交】提交前跑：cargo test -p agenthub-core ticket、cargo test -p agenthub-core "account_ connection_"、pnpm test -- ticket，全绿才提交。RFC 是文档提交，不需测试但需自查与 §5.5 六条不变式逐条对照。

【汇报】分支与提交列表；测试结果原文；C1 聚合规则的设计决策；RFC 全文链接；D3 盘点表；未尽事项。
```

---

## P4 · 连接域收口（泳道 D1 + D2）→ `refactor/connection-cleanup`

```text
【角色】你是 AgentHub 的工程师，在独立 git worktree 中工作，分支 refactor/connection-cleanup（基于 dev）。工作目录：d:\demo_chen\2026\worktrees\agenthub-cleanup。只在本分支提交，不合并、不碰 dev/release。这是小而严谨的收口包，禁止顺手重构其它区域。

【必读】仓库根 AGENTS.md；docs/connection-binding-model.md；docs/account-authorization-pool.md；docs/modularity-improvement.md P1-5；docs/routing-connection-refactor-plan.md 任务卡 D1、D2。

【任务 1 · 文档矛盾修复（D1）】三处，只对齐事实、不新增决策，改动处标注核对日期 2026-08-22：
  ① docs/account-authorization-pool.md：§6/§9 仍写「按 identity 分组 UI」验收，与 §8「Connections 已是登录列表、勿再验收分组」矛盾——以 §8 为准改写 §6/§9，历史项标注（历史）；
  ② docs/connection-binding-model.md §4：「refresh single-flight 发生在票这一层」与实现不符，改为按 account 行（授权）single-flight，与 account_service/oauth_owner.rs 的 owner 分治实现对齐；
  ③ docs/connection-binding-model.md §2.4 对照表补一行：前端 connection-pool-store（src/app/runtime/connection-pool-store.ts）= accounts+providers 列表缓存，与 ConnectionService（ActiveBinding 事务 owner）同名不同物，禁止混称。
  完成后自查全部改动处的交叉引用不断链。

【任务 2 · 双 AccountService 实例收口（D2）】crates/agenthub-core/src/lib.rs 的 AgentHub 装配中，ticket_bind 另构了一份 AccountService::with_live（与 hub.accounts 分离实例，仅共享 DB）。改为经 from_parts 注入共享实例（延续 modularity P1-5 方向），确认 switch saga 锁与缓存单实例。公开 API 签名不变；new() 兼容构造保留给测试。

【范围外】不动 bridge/、adapter_route_service/、ticket_read_service（其它 agent 在改）；不做文档以外的行为变更；不拆上帝文件；凭据加密与国产 OAuth 话题不出现在任何改动里。

【测试与提交】D1 纯文档无需跑测试，但需通读自查；D2 提交前跑 cargo test -p agenthub-core "account_ ticket_"，全绿才提交，并确认 AgentHub::open 后无第二套 AccountService 实例（测试构造除外）。

【汇报】分支与提交列表；D1 三处改动的前后对照摘要；D2 测试结果原文；未尽事项。
```

---

## 附：汇报模板（四个 agent 通用）

```text
分支：refactor/xxx
提交：<oneline 列表>
测试：<每条过滤命令 + 结果原文（通过/失败数）>
设计决策：<关键取舍与放弃的备选，3 条以内>
未尽事项 / 需主 Agent 拍板项：<列表，可为空>
```
