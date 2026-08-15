# 本机桥页终态 — Cursor 多 Agent 实施提示词

> 一次性派工。做完后删除本文件，终态只留在稳定文档（先以 [bridges-page-redesign.md](bridges-page-redesign.md) 为准，PR 5 再回写 ui-design / adapter-design / connection-binding-model）。

**真源（先通读，再动手）：** [docs/bridges-page-redesign.md](bridges-page-redesign.md)

**仓库约定：** 根目录 `AGENTS.md`。测试由测试 Agent 跑，实现 Agent 不代跑完整套件。

---

## 0. 总控（Orchestrator 先贴这段）

你是主 Agent。按下面的波次派工，**不要自己改产品代码**，不要把 5 个 PR 揉成一次提交。

### 硬约束（每个子 Agent 提示词都要带上）

- 不把 bind / analyze / plan / apply UI 搬回本页。创建绑定只走 Dashboard / Connections 的 `ConnectFlowDialog`。
- 不做凭据落盘加密；不做国产 OAuth 开边 / OAuth→API。
- 不做 `agenthub-adapterd` sidecar。
- 页面不 `invoke`；只走 `lib/api/*`。
- 本轮 **不** 重命名 `lib/api/adapter`、`lib/backend/contracts/adapter.ts`、Rust `Adapter*`、mocks。
- 不提供 `removeAdapter` 当孤立强删。孤立与日常解绑都是 `unbindTicket`。
- ConnectFlow 字符串改动必须对 ①②③ 仍成立。禁止把共用错误改成「本机桥」。
- 测试不与生产写在同一文件。不往生产 façade 加 `__reset*ForTests`。
- **不要发明产品决策。** 文案、页态、路由、可见性以设计稿为准。

### 依赖与并行（不能 5 人同时开干）

```text
Wave 1  Agent A = PR 1     必须先完成
Wave 2  Agent B = PR 2     依赖 A
Wave 3  Agent C = PR 3     依赖 A + B
Wave 4  Agent D = PR 4     依赖 A + B（建议等 C，避免再改 Sidebar import）
        Agent E = PR 5     与 D 可并行（只改 docs）；最好等 C，才能写死条件侧栏
```

| 波次 | Agent | 独占文件（别人不准碰） |
|---|---|---|
| 1 | A | `src/pages/adapter/**`（不迁目录、不删 barrel）、`src-tauri/src/tray.rs`、`src-tauri/src/adapter_bridge_controller.rs` 启动错误串、对应 Rust/前端测试、ConnectFlow **仅 3 处字符串** |
| 2 | B | `src/App.tsx`、redirect、`Sidebar.tsx`（path+label+icon，仍常驻）、Dashboard 徽标深链、Connections 用途 parts |
| 3 | C | `src/app/runtime/bridge-presence-store.ts`（新建）、`tickets.ts` notify、`Sidebar.tsx` 显隐、Settings 数据 tab |
| 4 | D | `src/pages/adapter/**` → `src/pages/bridges/**`；删创建流残骸；`App.tsx` import |
| 4 | E | `docs/**`、`README.md`（不含本派工文件以外的代码） |

每波结束后：主 Agent 验收 diff 是否越权；测试 Agent 按该 PR 的测试清单跑过滤后的 `pnpm test` / `cargo test`。全绿再开下一波。

每波单独提交，标题用设计稿 PR Plan 里的标题。

---

## 1. Agent A — PR 1（Wave 1，先做）

```
你只做 PR 1。真源：docs/bridges-page-redesign.md（§4.1、§6、§6.3、§6.6、§6.7、§7、§9、PR Plan PR 1、实现备忘）。

标题：fix(ui): treat adapter page as local-bridge runtime, not a bind workbench

目标：让 /adapter 页变成「本机桥」运维台。路由和侧栏先不动。

必须做：
1. 页标题「本机桥」；description / tip 用设计稿常量；header 去掉「去 Dashboard / 去 Connections」。
2. 去掉「本机桥运行时」PageSection 道歉壳。不要再包一层 pageRhythm.pageShell。
3. 健康空态：「没有本机桥」+ 一段 description；不传 actionLabel/onAction。
4. partitionLocalBridgeRuntimes：bound / orphan。列出全部 route=local_bridge。源已删且 binding 未命中 → 孤立区，不要丢行。
5. 钱包 listTicketWallet 失败：保留上次 bindingProfileIds / lastWalletBridgeCount，禁止 catch → new Set() / count=0。
6. pageViewState 按设计稿：profile loading 或 wallet 未 settled → loading；list_error；list（含 only-orphan）；wallet_without_runtime；healthy_empty。
   - 健康空仅当 bound+orphan===0 且 lastWalletBridgeCount===0 且两边都 settled。
   - 仅孤立：不套 EmptyState，只显示「孤立本机桥」。
   - lastWalletBridgeCount>0 且列表空：非健康说明 + 重试，不是「没有本机桥」。
7. 行：单层健康 + 来源→目标 + 端口 + 主按钮。丢掉「配置已生效」和行上凭据/路线 Badge。
8. unavailableBridgeStatusForPoll：保留 previous.state 与 port。error 只当「从未观测过」占位。
9. adapterProfilePrimaryAction 增加 statusUnavailable：读失败 + last running/degraded → 「停止」，禁止「重试启动」。
10. 详情：单层运行时状态；无「配置已生效」块；目标写入纯文字，禁止链到 /connections?agent=（生成投影不进钱包）。
11. 确认框：「停止本机桥？」「解除本机桥绑定？」；解除只走 unbindTicket；禁止 removeAdapter。
12. 托盘 tray.rs 六条文案按 §4.1 改成本机桥。
13. adapter_bridge_controller 启动/停止错误：本机桥无法启动或停止（仅桥控制面）。
14. Tauri façade fallback 若改：用「操作失败」，禁止「本机桥操作失败」。
15. ConnectFlow 只改这三处，且必须对 ①②③ 仍成立：
    - ConnectFlowDialog 原生切换：「不会创建跨服务绑定。」
    - default-deps bindViaTicket：「未找到对应的绑定配置」（禁止「本机桥配置」）
    - connect-flow-state listProfiles 碎片：「绑定档案」（禁止「本机桥档案」）

禁止：
- 迁 src/pages/adapter/；删 index.tsx re-export barrel；删 AdapterSourceList / TargetGrid / RoutePipeline / Preview。
- 改 App.tsx 路由、Sidebar path/label（那是 PR 2）。
- 新建 src/pages/bridges/。
- 改规划器 / 矩阵 reason。

测试（写测试，跑测试另交测试 Agent）：
按设计稿「测试计划」表更新 index.test.tsx / adapter-view-model.test.ts 断言。不删 preview 用例。同步 tray 与 adapter_bridge_controller 错误串测试。ConnectFlow / lib/api/adapter / mocks 回归要绿。

验收：打开 /adapter，空态无「去 Dashboard」；有桥时单层「运行中」+端口；读失败+上次 running 显示「状态不可用」+「停止」。
```

---

## 2. Agent B — PR 2（Wave 2）

```
你只做 PR 2。真源：docs/bridges-page-redesign.md §5、§8、PR Plan PR 2。

标题：feat(ui): route local-bridge page at /bridges and rename nav to Bridges

依赖：PR 1 已合入。页仍是 AdapterPage default export。

必须做：
1. App.tsx：/bridges → 现有 import AdapterPage from '@/pages/adapter'。
2. /adapter 与 /router → LegacyBridgesRedirect（学 LegacyConnectionsRedirect）：replace 到 /bridges，保留非 tab 查询，丢弃 ?tab=api|oauth。
3. Sidebar：一次改成 { to: '/bridges', label: 'Bridges', icon: Cable }。本 PR 仍常驻，不要做条件显隐。
4. Dashboard 桥徽标：仅当前 ③（generatedProviderId === 当前 Provider）。navigate(`/bridges?profile=${id}`)；无 id 则 /bridges。tip「管理本机桥」。view.bridge 带 profileId。
5. Connections 用途从单字符串改成 parts。「本机桥」可点：有 profileId → /bridges?profile=；null → /bridges。点击不打开 ConnectFlow。搜索仍匹配「本机桥」。
6. 页内 ?profile=：列表 ready 后打开对应详情；未知/缺失不 toast。关详情 replace 清 query。PR 1 若还写死 /adapter，这里改成 /bridges。

禁止：
- 新建 src/pages/bridges/ 或 BridgesPage。
- 条件隐藏侧栏（PR 3）。
- 改 bind / unbind 语义。

测试：redirect 丢掉 tab；徽标带 profileId；wallet 用途 parts 搜索命中「本机桥」；profileId 空不崩。
```

---

## 3. Agent C — PR 3（Wave 3）

```
你只做 PR 3。真源：docs/bridges-page-redesign.md §3、K10–K13、PR Plan PR 3。

标题：feat(ui): show Bridges nav only when a local bridge exists

依赖：PR 1 的 partition 已在页上；PR 2 的 path/label 已是 Bridges。

必须做：
1. 新建 src/app/runtime/bridge-presence-store.ts（subscribe / getSnapshot / reset*Store，学现有 runtime store）。
   字段只有：status, hasLocalBridgeProfile, walletBridgeCount, lastNonZero。
   不算 bound/orphan，不订连接池，不 4s 轮询，不做 StatusPin。
2. shouldShowBridgesNav：hasLocalBridgeProfile || walletBridgeCount>0 || (error && lastNonZero)。
3. 启动 idle 拉一次 listAdapterProfiles + listTicketWallet。
4. notifyBridgePresenceChanged 只从 bindTicket / unbindTicket 调用（已有 notifyConnectionPoolChanged 之后）。不要改 ConnectFlow，不要订阅每一次 pool 变更。
5. 失败保留上次 hasLocalBridgeProfile / walletBridgeCount，禁止写成 0。
6. 首屏默认按空（不闪 Bridges），ready/error 后再插入。
7. Settings → 数据 tab 常驻一行「本机桥运行时」链到 /bridges，不看 presence。
8. Sidebar 按 shouldShowBridgesNav 显隐。仍停在 /bridges 且 count 到 0 时不踢走。

禁止：
- 再实现一套 partition。
- 侧栏健康钉。
- 删页面文件。
- 用 removeAdapter。

测试：空 ready 隐藏；有 local_bridge（含孤立）显示；wallet>0 显示；失败且从未非零隐藏；失败保留 last count。
```

---

## 4. Agent D — PR 4（Wave 4，可与 E 并行）

```
你只做 PR 4。真源：docs/bridges-page-redesign.md §4.2、§10、PR Plan PR 4。

标题：refactor(ui): move bridge page to pages/bridges and drop dead create-flow chrome

依赖：PR 1–2 已合。建议 PR 3 已合。

必须做：
1. 先改写 index.test.tsx / adapter-sources.test.ts / adapter-view-model.test.ts，去掉对 AdapterPreviewResult、AdapterSourceList、AdapterTargetGrid、AdapterRoutePipeline 的依赖。
2. 再把 src/pages/adapter/ 迁到 src/pages/bridges/，default export 改为 BridgesPage。
3. App.tsx 改 import BridgesPage from '@/pages/bridges'。
4. 删除创建流残骸与 index.tsx barrel。
5. eligibility.ts 顶部注释改为 canonical in this module（不再写 Copied from adapter-sources）。
6. 验收：代码树 rg pages/adapter 为零（docs 留给 Agent E）。ConnectFlow 与 mock 测试全绿。

禁止：
- 改 bind/plan/analyze 行为。
- 重命名 lib/api/adapter 或 Rust。
- 改文档（那是 Agent E）。
```

---

## 5. Agent E — PR 5（Wave 4，可与 D 并行）

```
你只做 PR 5。只改文档。真源：docs/bridges-page-redesign.md §11、PR Plan PR 5。

标题：docs: retire 桥与适配 IA in favor of Bridges

依赖：PR 2 路径已是 /bridges。最好 PR 3 已合（才能写死条件侧栏）。

必须改：
- docs/ui-design.md：导航、线框、§1.4 空态例外、徽标=当前③、重写 §4.3.3
- docs/adapter-design.md：§1 / §4 按本页终态；用户表面 Bridges，模块仍叫 Adapter
- docs/connection-binding-model.md：§5.3 徽标范围；§5.5 /bridges、条件侧栏、单层行、禁止链钱包投影
- docs/hub-redesign-plan.md：文首写 §3.2 冻结已解除；§4 锚点改到仍存在的文件（eligibility / connect-flow / pages/bridges），标历史 Phase 1 文件名
- docs/agenthub-plan.md、architecture.md、adapter-kimi-codex-dogfood.md、docs/README.md、根 README.md：/adapter→/bridges；模块行 Bridges
- 删除本派工文件 docs/bridges-page-redesign-exec.md
- 若 PR 已全部落地：把 bridges-page-redesign.md 标为已实施，或把仍有效的句子并进上述稳定文档后删本文（按仓库文档规则，一次性稿执行完应回收）

禁止：
- 改 product-decisions.md 的三路决策。
- 改代码、tray、规划器矩阵。
- 讨论凭据加密或国产 OAuth 开边。
```

---

## 6. 测试 Agent（每一波结束后）

```
不要改产品代码。按刚完成的 PR 跑过滤后的测试，只回报是否全绿和失败用例原文。

PR 1：pnpm test 过滤 adapter / connect-flow / ticket；cargo test 过滤 tray / adapter_bridge
PR 2：pnpm test 过滤 App 路由 / Sidebar / dashboard AgentOverview / ticket-wallet
PR 3：pnpm test 过滤 bridge-presence / Sidebar / settings / tickets
PR 4：pnpm test 过滤 pages/bridges + connect-flow + mocks/adapter；确认无 pages/adapter 引用
PR 5：不跑测试；检查文档互链是否 404
```

---

## 7. Cursor 怎么开

1. 新对话贴「§0 总控」+「§1 Agent A」，做完提交。
2. 新对话贴「§2 Agent B」（或同一对话开第二个 Agent，但文件表互斥）。
3. 再开 Agent C。
4. 最后并行 D + E。

不要让两个 Agent 同时改 `src/pages/adapter/` 或同时改 `Sidebar.tsx`。
