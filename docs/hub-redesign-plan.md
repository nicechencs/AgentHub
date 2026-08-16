# Hub 重构 Phase 1 实施方案（Agent 优先信息架构）v2

> 状态：**Phase 1 已实施**（2026-08-14），本文保留为当时的实施记录。  
> **§3.2 过渡冻结已解除**（2026-08-15）：终态 IA 见 [bridges-page-redesign.md](bridges-page-redesign.md)。现行表面是 **Routes / 本机路由**（`/routes`）；`/adapter`、`/router`、`/bridges` 永久跳过来。页目录仍为 `src/pages/bridges/`。下文 §3.2「不移除 `/adapter`、不改路由结构、侧栏改名『桥与适配』」是当时护栏，不是现行约束。  
> **2026-08-15 起的领域与 UI 目标**改以 [connection-binding-model.md](connection-binding-model.md) 为准：票 / 绑定 / 协议图；Connections 改为全局钱包；真票常驻「接到…」；生成投影退出列表。**产品方向**以 [product-decisions.md](product-decisions.md) 为准（① API 直连 / ② 原生订阅 / ③ 本机路由）。下文「不改 OAuth 门禁」只约束当时 Phase 1 实施范围，不是「订阅一律不跨 Agent」。Phase 1 的对话框外壳仍可复用，**按 Agent tab 分页、行按钮白名单、诊断只放 Dashboard 不再是终态**，UI 允许按目标文档重做。
> 验收：pnpm typecheck / typecheck:test / test（627 用例，含集成 bug 防回归）/ build 全绿；cargo test 79 用例全绿（Rust 未改动）；dev:mock 冒烟通过（空态引导、非空可行性置灰+原因、无控制台错误）。
> 关联文档同步：docs/ui-design.md、docs/adapter-design.md 正文定位、docs/architecture.md §4.1 目录树（lib/connect-flow、components/connect）与 §4.6、README.md、docs/README.md、docs/agenthub-plan.md、docs/testing.md、docs/adapter-kimi-codex-dogfood.md。
> v2 修订要点：plan.canApply 为可执行权威；补同 Agent 原生切换分流；用途/徽标改用 profile 联结（不读 provider.meta）；apply 自动切换语义如实；排除 adapter 生成 Provider 作为来源；两层 OAuth 门禁；可注入 helper 保证 Node 环境可测。

## 1. 背景与问题

当前凭据相关功能按"机制"分成两个页面：

- `/connections`：账号池（accounts）+ 供应商池（providers）的聚合列表，负责凭据生命周期（OAuth 授权/导入、API key、刷新、删除、当前生效绑定）。
- `/adapter`：跨 Agent 投影（把 Connections 中已有连接接到另一个 Agent），含 analyze/plan/apply、能力矩阵、本地桥。

问题：用户的真实任务是"让 Agent X 用起来"（Agent 视角），而不是"先判断我的凭据是什么协议、再决定去哪个页面"（机制视角）。跨服务复用（如 Kimi 会员 → Claude Code）需要专门跑一趟 Adapter 页，"分析"是用户手动动作而非系统预计算。

## 2. 设计结论（已在前期讨论定稿）

领域三分层，UI 按用户目标组织：

```text
凭据域（credential store）   = accounts + providers（现状保留）
路由引擎（routing engine）    = 能力矩阵/桥/密钥解析（现状保留，不动）
绑定域（bindings）           = "谁用谁"（Phase 1 仅做读模型聚合，不做物理合并）
```

Phase 1 当时的 UI 形态：Dashboard 卡片发起连接/切换；Connections 仍按 Agent tab，行按钮只给可 apply 的 Provider。  
**「Agent tab + 行按钮白名单」已被后续 Connections 全局钱包取代**（真票常驻「接到…」，不可行在对话框置灰 + 原因，不再靠行上藏按钮）。  
**此后的目标形态**见 [connection-binding-model.md](connection-binding-model.md) / [ui-design.md](ui-design.md)：全局钱包、真票常驻「接到…」、不可行在同一对话框说明。下文 §3 是 Phase 1 冻结范围，不是下一轮 UI 约束。

## 3. Phase 1 范围

### 3.1 目标（本期交付）

#### A. 统一连接流程 `ConnectFlowDialog`（新组件）

进入方向（判别联合，见 §6 契约）：

- Agent 侧（Dashboard 卡片"连接/切换"）：固定 target Agent，选来源。
- 凭据侧（Connections 行"用于其他 Agent"）：固定来源，选 target Agent。

**来源列表分为两组**（Agent 侧进入时）：

1. **本 Agent 自有凭据（原生切换组）**：目标 Agent 自己的 accounts/providers。
   - `isCurrent=true` 的项：标注"当前使用"，禁用重复提交。
   - 未生效项：走**既有切换 API**（复用 Connections 页现有 switch preview + switch 调用链，不走 adapter）。
   - **原生切换同样有 capability 门禁**：复用 Connections 页现有判定（account switch 能力与 `providerCapabilityGate.canSwitch`），不可切换项状态为 `blocked_native(reason)`，置灰显示既有原因文本，不进入 preview/switch。
   - adapter 生成的 Provider（经 `AdapterProfile.generatedProviderId` 识别）在本组显示并标注"经兼容路由 · 来源 X"，切换到它同样走原生 provider 切换。
2. **其他服务凭据（跨服务复用组，本期核心）**：钱包中排除以下项后的全部凭据：
   - adapter 生成的 Provider（沿用 `src/lib/connect-flow/eligibility.ts` 的 `excludeAdapterGeneratedSources` 规则，避免二次投影链；历史 Phase 1 文件名：`src/pages/adapter/adapter-sources.ts`）；
   - 目标 Agent 自有凭据（已在组 1）。
   - **可行性权威：对每个候选 fan-out `planAdapter`（只读 Phase 0 预览），以 `plan.canApply` 决定可选/置灰**；路线摘要与原因取 `plan.analysis`。禁止以 `analysis.support` 推断可执行。
   - **两层门禁分离**：(a) 来源 OAuth 未完成 → 本地预检（沿用 adapter-sources.ts 既有识别），**不发起 fan-out**，该项显示"去 Connections 完成登录"；(b) OAuth 完成但能力矩阵关闭 → plan 返回的原因文本原样透传，置灰。
   - 选中可行项 → 预览步（plan 结果人话化：写哪些配置/服务影响/是否起桥/端口/模型映射）→ 确认 → `bindTicket` / `bindViaTicket`（`src/lib/connect-flow/default-deps.ts`；当时方案写 `applyAdapter`）。
   - **apply 成功语义（如实）**：后端 apply 会自动使生成/更新的 Provider 成为目标 Agent 当前连接（直接路由与首次桥 apply 均如此）。成功态以 `result.provider.isCurrent` 为权威展示"已生效"；不存在"需手动切换"的常规成功分支。
   - 失败态：错误原文 + 保留选择与预览、可重试；busy 期间禁止重复提交与关闭。
   - apply 成功后由对话框触发页面级刷新（见 §6 集成契约 `onApplied`）；刷新失败显示"已应用，但列表刷新失败"，不得误报未生效。

其他连接方式为**引导入口**（Phase 1 只跳转；自动弹窗与回跳已在后续落地，见 §10）：

- ① 导入已有登录态（OAuth）：跳 `/connections?agent=X&intent=import-login&resume=X`，Connections 打开导入确认（不静默写入）。**能力边界如实**：入口是"导入当前登录态"（读取官方 CLI 已完成的登录），不能发起全新 OAuth 授权。
- ② 新 API Key：跳 `/connections?agent=X&mode=providers&intent=add-key&resume=X`，自动打开添加对话框。成功后回 `/?connect=X` 重开 ConnectFlow。

**空态定义**：钱包为空 → 提示并引导去 Connections 添加；有凭据但全部不可行 → 全部置灰保留原因 + 新增凭据入口；资源部分加载失败 → 显示加载错误与重试，不得把缺失数据当空池。

#### B. Dashboard Agent 卡片增强

- 维持现状：**只渲染已安装 Agent**（不新增未安装卡片；整体空态行为保留）。
- 卡片动作模型从字符串 `target` 改为判别联合 `action: { kind: 'connect' } | { kind: 'navigate'; to: string }`，键盘（Enter/Space）行为与点击一致。已安装 Agent 主动作 = 打开 ConnectFlowDialog。
- 徽标（均基于 **profile 联结**，不读 provider.meta——前端 Provider 类型未映射 meta 字段）：
  - "经兼容路由"：当前生效 provider 的 id 命中某 `AdapterProfile.generatedProviderId`。
  - 桥状态：命中的 profile 为 bridge 型时显示；**沿用 `use-adapter-resources.ts` 的既有轮询模式**（运行/降级态轮询 + generation 防竞态），查询失败显示"状态不可用"，不得静默隐藏。
- profiles 由页面挂载时一次 `listAdapterProfiles()` 全量拉取后前端归并；桥状态仅对命中的 profile 查询。

#### C. Connections 钱包化增量（Phase 1 已做；全局钱包与常驻「接到…」见目标文档）

- 每行增加"用途"：该凭据正被哪些 Agent 使用。算法（纯函数）：
  - 直接用途：该 account/provider 自身 `isCurrent=true` → 用于其 agentId。
  - 兼容路由用途：存在 profile 满足 `profile.sourceKind/sourceId` 指向该凭据（按 `(kind, id)` 匹配，防 account/provider id 碰撞）**且** `generatedProviderId` 对应的 Provider 当前 `isCurrent=true` → 用于 `profile.targetAgentId`。
  - 同一 Agent 同时命中直接与兼容用途时去重（显示一次，直接用途优先）。
  - profile/生成 Provider/来源缺失或数据部分加载失败 → 该行用途显示"未知/不完整"，不得显示为"未使用"。
- 行动作增加"用于其他 Agent"→ 打开 ConnectFlowDialog（来源预选）。入口仅对存在后端可 apply 路径的 Provider 显示：Kimi Code 会员 Provider、Claude 的 Anthropic Provider；account 来源（含 apikey）与 adapter 生成 Provider、无规则来源一律不显示。行按钮是可行动作入口；不可行诊断由 Dashboard「连接/切换」承担。**此白名单已被后续 Connections 全局钱包取代**（真票常驻「接到…」）。
- **不改动现有 agent tab 过滤结构**（当时把跨 Agent 全局钱包视图标为 Phase 2；**现已被全局钱包取代**，本期只做行级增强是当时回归面控制，不是现行约束）。

### 3.2 非目标（明确不做，Review 请勿要求扩入）

- **不动 Rust 后端**：analyze/plan/apply/bridge/OAuth/switch 命令与能力矩阵原样使用。若实施中发现必须改后端才能达成目标，停下上报，不得绕过。
- **不做 AdapterProfile 与 agent_active_bindings 的物理合并**（推迟 Phase 2；本期只做前端读模型聚合）。
- **不改 OAuth 门禁**：`canApply=false` 的路线保持不可用，UI 呈现为置灰+原因。
- **不移除 `/adapter` 页与侧栏入口**（**2026-08-15 已解除**：现行规范路由 `/routes`，侧栏英文 Routes 有本机路由才出现；见 [bridges-page-redesign.md](bridges-page-redesign.md)）。过渡期职责定位：Dashboard/Connections 为推荐入口，本页只管理桥 runtime；两处 apply 行为同源（同一 lib/api 门面），不允许行为分叉。侧栏文案当时改为「桥与适配」，创建区已收掉。
- **不重做 OAuth 授权 UI**、不做 ①② 引导跳转的自动弹窗与回跳闭环（Phase 2）。
- **不重构 Connections 页 tab 信息架构**（全局钱包视图属 Phase 2）。
- **不修改 `src/lib/api/adapter.ts` 既有行为**（含 apply 后连接池刷新异常被吞的既有语义——对话框通过 `onApplied` 自行补偿刷新并呈现刷新失败）。
- **凭据落盘加密：无必要、项目范围外**（既定产品决策，不作为风险项或待办提出）。
- 不做 i18n 重构、不引入新 npm 依赖、不改路由结构（无新路由）。

## 4. 现状锚点（实施者必读，v2 已按评审核实修正）

- 路由与页面：`src/App.tsx`（`/` Dashboard、`/connections`、`/routes`；`/adapter`、`/router`、`/bridges` 永久跳到 `/routes`）。侧栏 `src/components/layout/Sidebar.tsx`（英文 Routes，有本机路由才出现）。
- Dashboard 卡片：`src/pages/dashboard/AgentOverview.tsx` + `agentOverviewModel.ts`。**注意：AgentOverview 只渲染已安装 Agent**；`buildAgentCardView.target` 现为 URL 字符串且被测试断言（改动作模型时同步改测试）。
- 生效连接解析：`src/lib/api/agent-connection.ts`（基于 accounts/providers 的 isCurrent）。**Dashboard 的 agents 状态来自页面自身的 `listAgents()` 加载，不随连接池自动刷新**——apply 后需显式触发重载。
- 连接池 store：`src/app/runtime/connection-pool-store.ts`（accounts+providers 缓存与 `notifyConnectionPoolChanged`）。
- Connections 行模型：`src/pages/connections/connection-model.ts`（`ConnectionEntry`）；切换调用链在 `src/pages/connections/ConnectionList.tsx`（switch preview + switch）。
- Adapter 门面：`src/lib/api/adapter.ts`（analyze/plan/apply/remove/listProfiles/bridge 全套；**apply 内部的连接池刷新失败会被吞掉**，页面不得直接 invoke）。
- 可执行权威：`src/lib/backend/contracts/adapter.ts` 中 `AdapterApplyPlan.canApply`；`analysis.support` 仅表示兼容性。存在 `support='stable'` 但 `canApply=false` 的真实路径。
- **前端 `Provider` 类型无 `meta` 字段**（`src/lib/types.ts`；`provider-map.ts` 仅映射 preset/official）。adapter 生成 Provider 的唯一前端识别方式：`AdapterProfile.generatedProviderId ↔ Provider.id`。
- 既有 fan-out 参照：`src/lib/connect-flow/`（`plan-fanout.ts` / `eligibility.ts`；缓存、generation 防竞态、单项 retry）。历史 Phase 1 文件名：`src/pages/adapter/use-adapter-target-analyses.ts`。**已知局限需在新实现中修复：无并发上限、缓存无失效机制**。
- 来源排除与 OAuth 预检参照：`src/lib/connect-flow/eligibility.ts`（`excludeAdapterGeneratedSources`、OAuth 未完成识别）。历史 Phase 1 文件名：`src/pages/adapter/adapter-sources.ts`。
- 桥状态轮询参照：`src/pages/bridges/use-bridge-resources.ts`（4s 轮询、失败映射为不可用而非隐藏）与 `src/pages/bridges/index.test.tsx` 中可注入 helper 的测试模式。历史 Phase 1 文件名：`src/pages/adapter/use-adapter-resources.ts`。
- **后端 apply 语义**：直接路由与首次桥 apply 均会把生成/更新的 Provider 设为目标 Agent 当前连接（`adapter_apply_service.rs`、`adapter_bridge_controller.rs`）。
- 测试环境：**vitest Node 环境，无 jsdom/RTL，不能直接渲染 hook**。逻辑必须抽成可注入的命令式 helper/controller，hook 只做薄封装（参照 `startAdapterBridgeStatusPoll` 的既有模式）。
- Mock：`src/dev/mocks/` 已覆盖 adapter/account/provider 域门面；**初始 fixture 为空，backend factory 未重置 account 域**——本期需要的 fixture/reset 补在 dev/mocks（不得往生产 façade 加测试钩子）。

## 5. 交互流程定义（v2）

### 5.1 Agent 侧进入（Dashboard 卡片）

1. 已安装 Agent 卡片"连接/切换"→ ConnectFlowDialog（target 固定）。
2. 顶部：当前生效连接摘要（含"经兼容路由"/桥状态徽标同款信息）。
3. 来源区两组：本 Agent 凭据（原生切换）/ 其他服务凭据（fan-out plan，canApply 定可选性，置灰项带原因；OAuth 未完成项不 fan-out、引导登录）。①② 为次要引导按钮（跳转并关闭对话框）。
4. 原生组选择未生效项 → 既有切换链（preview→confirm）→ 成功后走 `onConnectionChanged` 页面刷新。
5. 跨服务组选择可行项 → plan 预览步 → 确认 → apply → 成功态（以 `result.provider.isCurrent` 展示"已生效"）→ `onConnectionChanged` 页面刷新；刷新失败如实提示。
6. 失败态：错误原文、保留现场、可重试；busy 禁止重复提交/关闭。

### 5.2 凭据侧进入（Connections 行）

1. 行按钮「用于其他 Agent」（仅白名单 Provider 显示：Kimi Code 会员、Claude Anthropic）→ ConnectFlowDialog（source 固定）。
2. 目标 Agent 网格：**直接排除来源自身所属的 Agent**（入口语义即"用于其他 Agent"；本 Agent 内的切换由 Connections 页既有交互承担）；其余目标按 canApply 可选/置灰+原因。
3. 选中可行目标后与 5.1 步骤 5 相同。

### 5.3 可行性呈现规则（v2）

- `plan.canApply=true` → 可选，显示路线摘要（取 plan.analysis：如"直连端点映射"/"本地桥"）。
- `plan.canApply=false` → 置灰 + plan/analysis 原因原文，不改写、不隐藏。
- plan 进行中 → 骨架态；plan 失败 → 该项错误态 + 单项重试（generation 防竞态，切换选择后旧响应必须丢弃、旧 plan 不得用于 apply）。
- fan-out 约束：并发上限（建议 3）+ 相同 route key 去重 + 缓存按 `(sourceKind, sourceId, targetAgentId)` 键控（含 kind 防 id 碰撞）+ **对话框每次打开时失效缓存**（防陈旧结论参与 apply）。

## 6. 技术方案（文件级，v2）

### 共享契约（类型骨架先行，实施者只依赖此契约与既有 lib/api）

`src/lib/connect-flow/types.ts` 至少定义：

- `ConnectFlowEntry`（判别联合）：`{ mode: 'for-agent'; targetAgentId } | { mode: 'for-source'; source: ConnectSourceRef }`。
- `ConnectSourceRef = { kind: 'account' | 'provider'; id: string }`（全流程以 `(kind,id)` 为身份）。
- `SourceOption`（原生组/跨服务组条目：ref、展示字段、组别、`state: current | switchable | blocked_native(reason) | plannable`；blocked_native 复用 Connections 既有 capability 判定与原因文本）。
- `PlanEligibility`（判别联合）：`loading | blocked_oauth | ready(plan, canApply, routeSummary, reason?) | error(message, retry)`。
- `ConnectFlowResult`、集成回调契约：**统一 `onConnectionChanged(outcome)`**——同时覆盖 adapter apply 成功与原生切换成功两种出口，页面收到后重载 agents+profiles+连接池，刷新失败提示"已应用/已切换，但列表刷新失败"；`onNavigate(to)`（引导跳转）。
- fan-out 内核契约：`createPlanFanout(deps)` 可注入 `planAdapter`、并发上限、缓存；返回命令式 controller（`start/cancel/retry/subscribe`）。hook `usePlanFanout` 仅为薄封装。

### 新增

| 文件 | 内容 | 所有权 |
|---|---|---|
| `src/lib/connect-flow/types.ts` | 上述共享契约 | 类型骨架 |
| `src/lib/connect-flow/eligibility.ts` | 纯函数：plan → `PlanEligibility` 映射；来源分组/排除（生成 Provider、目标自有）；OAuth 未完成预检 | C1 |
| `src/lib/connect-flow/plan-fanout.ts` | 可注入命令式 fan-out controller（并发上限/去重/缓存失效/generation）+ 薄 hook | C1 |
| `src/lib/connect-flow/connection-usage.ts` | 纯函数：用途反查（§3.1C 算法，含"未知/不完整"态） | C1 |
| `src/lib/connect-flow/*.test.ts` | 上述全部单测（Node 环境、mock backend、领域 reset 用 dev/mocks） | C1 |
| `src/dev/mocks/` 中本期所需 fixture/reset 补齐（account 域 reset、连接流程 fixture） | 仅服务测试与 dev:mock | C1 |
| `src/components/connect/ConnectFlowDialog.tsx`（可拆子文件） | 对话框 UI + 状态机：两组来源/目标网格/plan 预览/结果态/空态/失败态 | C2 |
| `src/components/connect/connect-flow-state.ts` + `.test.ts` | 对话框状态机抽成纯逻辑（Node 可测）：进入模式矩阵、busy 锁、stale 响应丢弃、成功/失败/刷新失败态 | C2 |

### 修改

| 文件 | 变更 | 所有权 |
|---|---|---|
| `src/pages/dashboard/agentOverviewModel.ts` + `agentOverviewModel.test.ts` | `action` 判别联合替代 `target` 字符串；徽标字段（经兼容路由/桥状态，输入为 profiles+providers 联结结果）；测试同步更新 | C3 |
| `src/pages/dashboard/AgentOverview.tsx` | 徽标渲染；点击/键盘统一走 `action`；`onConnectRequest(agentId)` 回调（不 import 对话框） | C3 |
| `src/pages/dashboard/index.tsx` | 挂载 ConnectFlowDialog、接线 `onConnectRequest`/`onConnectionChanged`（重载 agents+profiles+连接池）、桥状态轮询接入 | 页面集成 |
| `src/pages/connections/connection-model.ts` + `connection-model.test.ts` | 行模型增加用途字段（消费 C1 的 connection-usage 输出） | C4 |
| `src/pages/connections/ConnectionList.tsx`、`ConnectionCard.tsx` | 用途展示；"用于其他 Agent"动作 `onReuseRequest(entry)` 回调（生成 Provider 不显示入口） | C4 |
| `src/pages/connections/index.tsx` | 挂载 ConnectFlowDialog、接线回调与刷新 | 页面集成 |

### 依赖与并行规则（v2）

- 依赖顺序：`types.ts`（类型骨架）→ C1（实现契约）与 C2/C3/C4（按契约并行，测试注入 fake 实现）→ 页面集成接线。
- C2/C3/C4 不 import 彼此与 C1 的实现文件，只 import `types.ts` 与既有 `lib/api`（C4 例外：可 import `connection-usage` 的**函数签名**，测试用 fake）。
- 不新增依赖；不改 `package.json`、路由、侧栏；不改 `src/lib/api/adapter.ts`。

## 7. 风险与回滚（v2）

| 风险 | 缓解 |
|---|---|
| 桥状态过期/掩盖故障 | 沿用既有轮询模式（运行/降级态 4s 轮询 + generation）；失败显示"状态不可用"，禁止静默隐藏；仅对生效 provider 命中的 bridge 型 profile 查询 |
| fan-out 请求放大与陈旧缓存 | 并发上限 3、同 key 去重、对话框每次打开失效缓存、generation 丢弃旧响应且旧 plan 不得 apply |
| apply 副作用（写用户机器上的 agent 配置、切换当前连接、启动本地桥监听） | 预览步如实披露将发生的写入；结果态以 `result.provider.isCurrent` 为权威；操作回滚路径=切回原连接/停桥/移除非当前 profile（既有能力），文档明确"前端代码回退不恢复已写入的机器状态" |
| apply/切换后页面不一致（Dashboard agents 不随连接池自动刷新） | `onConnectionChanged` 集成契约强制页面重载 agents+profiles+连接池（apply 与原生切换共用）；刷新失败显示"已应用/已切换，但列表刷新失败" |
| Dashboard `target`→`action` 语义变更回归 | 判别联合 + 键盘行为保持；`agentOverviewModel.test.ts` 同步更新（C3 责任） |
| Connections 回归 | C4 负责 `connection-model.test.ts` 更新与 `connection-trash-lock.test.ts` 回归确认；集成后全量 `pnpm test` |
| 并行产出风格不一致 | 契约先行 + 集成阶段统一走查；UI 复用现有 `components/ui/*` 原语 |

代码回滚：全部为前端增量，git revert 即可、无 schema 迁移；**但用户已通过对话框触发的 apply 所写入的本机配置/桥/当前连接切换不会随代码回退恢复**（此为既有 apply 行为，本期仅改变入口可达性）。

## 8. 测试计划（v2）

- eligibility：`canApply` 真/假映射（含 `support='stable'` 但 `canApply=false`）、门禁原因逐字透传、OAuth 未完成预检（不发起 fan-out）、analyze/plan 错误对象（AdapterCommandError/Error/string）的原文展示、来源排除规则（生成 Provider、目标自有凭据）。
- plan-fanout：缓存命中、`(kind,id,target)` 键控防碰撞、并发上限与去重、对话框重开失效、generation 竞态丢弃、单项重试。
- connection-usage：直接绑定、经 profile 联结的兼容用途（要求生成 Provider isCurrent）、直接+兼容去重、profile/生成 Provider/来源缺失 → "未知/不完整"、数据部分加载失败不得显示"未使用"、profile 各状态（applying/active/needs_attention）计入规则固化。
- connect-flow-state：两种进入模式 × 预选参数矩阵（含非法/已删除来源）、busy 锁、stale plan 不得 apply、apply 成功（isCurrent 权威）/失败/刷新失败三态、原生切换分支（含 blocked_native 置灰）、凭据侧排除自身 Agent。
- agentOverviewModel：`action` 判别联合、徽标字段、既有断言迁移。
- 回归：全量 `pnpm test`、`pnpm typecheck`、**`pnpm typecheck:test`**、`pnpm build`。Rust 无改动，`cargo test` 一次性确认。
- 手动：`pnpm dev:mock` 走通 5.1/5.2（含空态与置灰原因呈现）。

## 9. 实施记录与偏差（2026-08-14）

实施与 §6 计划的差异、以及集成期发现并修复的问题，如实记录：

1. **新增 `src/lib/connect-flow/default-deps.ts`（+ 测试）**：§6 表未列；`createDefaultConnectFlowDeps()` 组装 lib/api 真实现（含 switchNative 复用 Connections 切换链：account 直接 `switchAccount`，provider 先 `switchPreview` 再 `switchProvider`），是页面接线的唯一入口。
2. **原生切换门禁的数据源偏差**：`SourceOptionsInput` 未含实时 agentStatuses，账号侧 capability 门禁以 agent catalog 的 capabilities 回退判定（与 Connections 页实时 detect 结果可能存在时差）。Phase 2 如需准实时门禁需扩契约。
3. **集成缺陷修复（两个单测测不到的接缝 bug）**：`plan-fanout` 的 `getState()` 曾返回同一可变 Map，`useSyncExternalStore` 的 `Object.is` 快照比较吞掉全部更新，可行性 UI 永远停留初始态；叠加对话框 effect 依赖了逐渲染新建的数组，反复 `start()` 使在途请求被 generation 作废。修复：状态改不可变快照（每次变更换新 Map 引用）+ `start()` 同签名幂等；已补两个防回归用例（快照引用契约、幂等保活）。
4. **dev:mock 手动验证结论**：空态、来源/目标呈现、排除规则（生成 Provider、来源自身 Agent）、置灰+原因原文（"当前不支持不等于连接失效"话术）均走通，控制台无错误。apply 正向 UI 链路（预览→确认→已生效→徽标/用途更新）在 mock 下不可达——手工建的 provider 不带 mock 分类可识别的 preset 语义，全部目标 `canApply=false` 属预期；该链路状态机已由单测覆盖，**留待真机（`pnpm tauri:dev` + 真实 Kimi 会员 preset）验收**。
5. mock 域增补 `resetMockAccounts`/`upsertMockAccount` 且 backend factory 补 account 域 reset（服务测试；dev:mock 界面初始仍为空池，属既有设计）。
6. **原生切换预览为简化版**：对话框内原生切换（`switchNative`）的 provider 路径按既有链路先 `switchPreview` 再 `switchProvider`，流程安全等价，但 preview 返回的细节（备份/回填提示等）未在对话框呈现，仅显示确认文案；Connections 页的完整切换预览对话框不受影响。如需对齐信息量，Phase 2 扩 `ConnectFlowDeps` 暴露 preview DTO。
7. **代码评审轮（2026-08-14 下午）**：4 维度独立代码评审（逻辑层/对话框/页面集成/合规测试）经两轮修复后全部收敛（终核 PASS）。修复项：刷新失败契约被内部 catch 吞掉（改为返回成败 + store 快照判定，含 pool `ready+errors` 双侧失败与 statuses reload 的 promise 结果判定）、profiles 未就绪窗口内生成 Provider 复用入口 fail-open（页面与对话框双侧 fail-closed）、对话框关闭/卸载时在途会话未失效（session 于 effect cleanup 同步作废）、首帧旧状态闪现（entry 同步 guard）、preview 失效的确认窗口（`isPreviewInvalid` 同步禁用 + 确认原子锁 + 采用 `beginConfirm().next`）、fan-out 幂等签名未纳入 OAuth 门禁变化（签名含 blocked 集合）、跨 start/retry 在途请求去重（per-key token 语义）、桥轮询重叠（改链式）、profiles 并发加载竞态（generation）。测试由 627 增至 642。依赖说明：`lib/connect-flow` 除 contracts/lib/api/types 外还引用 `config/agents`（静态元数据）与 `lib/capability`（判定函数），组件直接使用 runtime 连接池 hook——与仓库既有页面惯例一致，视为允许。

## 10. 后续：票 / 绑定（取代原 Phase 2 展望）

原「Phase 2 再合并 binding / 再做全局钱包」已升格为已决策的目标架构，细节见 [connection-binding-model.md](connection-binding-model.md)。不再把全局钱包和常驻「接到…」当成可选项。

实施顺序（与目标文档 §6、§8 一致）：

1. 读模型：accounts+providers 聚成票；`is_current`+profile 聚成绑定；钱包不展示生成投影。
2. 进口打 `surface`；`plan(ticket, agent)` 收口，废掉前端第二份白名单。
3. UI 重做：Connections 默认跨 Agent 钱包；行上真票都有「接到…」；不可行在对话框置灰 + 原因；Dashboard 展示当前绑定而非「当前 Provider 行」。
4. 写入收成 `bind` / `unbind`；现有四条 apply 路径先改写成绑定实现。
5. 再按协议图加边（Anthropic→Codex、Kimi→Grok、新 surface……）。

仍有效、且已落地的 Phase 1 资产：ConnectFlow 双入口外壳、①② 深链、本机路由页不做日常创建。现行表面是 `/routes`（侧栏 Routes，有本机路由才出现），只管理 ③ 运行时。

OAuthFlowDialog 收编进 Connections 仍未做，可并进钱包「添加票」。
