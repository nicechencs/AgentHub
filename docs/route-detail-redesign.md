# 本机路由详情面板重设计

| 字段 | 值 |
|---|---|
| 作者 | — |
| 日期 | 2026-08-24 |
| 状态 | **Implemented（2026-08-24）** |
| 类型 | 产品 / UX / 信息架构（无新后端能力、无 wire 变更） |
| 范围 | `/routes` 行内展开详情（`AdapterProfileDetailDialog`）：信息去重、路由关系示意、逐边 Agent 支持判断、Quick Apply 与边状态合体 |
| 非范围 | 列表行结构大改、创建路由对话框、后端协议矩阵 / `can_apply`、凭据落盘加密（**无必要 / 项目范围外**）、国产 OAuth 开边 / OAuth→API（产品不做）、新增后端字段 |

本文是路由**详情面板**本期重设计的真源，风格与 [bridges-page-redesign.md](bridges-page-redesign.md)、[chat-page-redesign.md](chat-page-redesign.md) 对齐。页面级 IA（Routes / `/routes`、单层健康、启停）以 [bridges-page-redesign.md](bridges-page-redesign.md) 为准；本机三条入口与协议转换以 [local-route-endpoints.md](local-route-endpoints.md) 为准；端点审计快照见 [archive/route-endpoint-audit-2026-08.md](archive/route-endpoint-audit-2026-08.md)。

> **现行状态**：详情为 `AdapterProfileDetailDialog` 的「来源登录 → 本机桥 → 客户端接入」关系图；边状态与 Quick Apply 合体。本文为已落地规格。

---

## Overview

`/routes` 列表行已能完成「扫一眼 + 启停 + 一键配置」。详情应解释 **为什么是这条链路、每条下游边处于什么支持状态、出了问题怎么办**，而不是把列表上的 URL、状态、capabilities 再抄一遍。

用户心智模型是：**外部登录 → 本机桥 → 客户端接入**。详情首屏应是这张关系图；其余信息挂在对应节点或边上。

---

## Background & Motivation

### 现状问题（对照代码）

| # | 问题 | 证据 |
|---|---|---|
| 1 | **信息重复** | 本机 URL 在列表行、详情 Header、`localEndpoint` 出现三次；`capabilities.endpoints` 区块在详情内出现两次（约 L226–251 与 L290–308） |
| 2 | **路由关系不清** | 只有「来源 → 本机 URL 列表」，看不到外部上游 URL，也无直通 / 转换标注 |
| 3 | **Agent 支持判断粗** | `targetHidden` 只按 `profile.targetAgentId` 一次判定；「已写入」只看当前 profile 的 `generatedProviderId`，无法表达同来源兄弟边 |
| 4 | **区块堆叠** | Header / 状态 / capabilities×2 / members / localEndpoint / quickApply / targetWrite / recovery / diagnostics / footer，正常态也难一屏读完 |

### 必须保留的语义

- 行内展开（不是独立 Dialog）；`data-route-detail={profile.id}` 测试锚点可保留或等价迁移。
- 本机下游 surface：`/v1/messages`、`/v1/responses`、（条件）`/v1/chat/completions`。
- 运行状态单层语义：`bridgeRuntimeStatusView`（状态读取失败 ≠ 启动失败）。
- autoStart 是桥级属性，详情内可改；解除绑定走页面级确认。
- Quick Apply 仍走 `planTicket` + `bindTicket`，目标集合仍以 Claude / Codex / Grok 为主产品边。
- 界面说「登录」，不说「票」。

---

## Goals & Non-Goals

### Goals

1. 详情内每类信息只出现一次。
2. 首屏用三节点关系图讲清「外部什么端点 → 本机桥 → 客户端什么端点」。
3. 每条下游边有互斥状态枚举；禁用必带中文原因；`hidden` / `applied` **逐边**判定。
4. Quick Apply 与右列勾选合体，无第二套勾选框。
5. 正常态尽量一屏；recovery / error / members / diagnostics 条件出现或默认收起。

### Non-Goals

1. 不改后端协议矩阵，不新开 `can_apply` 产品边。
2. 不做凭据落盘加密；不开国产 OAuth / OAuth→API。
3. 不改列表行主结构与页面级启停 / 删除流程；不改创建路由对话框。
4. 不新增后端字段（`applied` 用既有 profiles 在前端聚合）。

---

## 设计原则

1. **一条数据只出现一次。**
2. **以「链路」为叙事主轴**，状态挂在节点 / 边上。
3. **详情只放列表行没有的东西。**
4. **每条下游边有明确状态，禁用必带原因。**
5. **诊断默认收起，异常条件出现。**

---

## 信息优先级

| 层级 | 内容 |
|---|---|
| **必须首屏** | 关系图（来源 / 桥 / 下游边）；运行状态与上游状态；每条边的可复制本机 URL 与支持状态；Quick Apply |
| **次要，可收敛** | 模型名单（一行）；autoStart；host/port（已含在 URL 中则不单列 DetailRow）；members（仅 ≥2） |
| **仅诊断（默认收起）** | profileId、rule + 版本、lastError、创建/更新时间、打开日志；上游外部 URL 全文 |
| **条件出现** | 来源已删、targetHidden 提示、recovery、mutation error |
| **删除（详情内不再独立出现）** | capabilities 区块 ×2、独立 localEndpoint、独立 targetWrite —— 全部并入关系图 |

---

## 推荐布局

推荐 **横向三列关系图 + 底部条件区**。窄容器（约 &lt;720px）降级为纵向堆叠。备选纯纵向卡堆不推荐：会把「一桥多下游」的扇出拉成列表，与现状差异不大。

```
┌─ 详情（行内展开） ──────────────────────────────────────────────────────────┐
│                                                                              │
│  ┌ 来源 ──────────────┐      ┌ 本机桥 ─────────────┐     ┌ 客户端接入 ────────────────┐
│  │ ● OpenRouter 登录   │      │ 127.0.0.1:8317      │     │ ✅ Claude   /v1/messages  [复制]│
│  │ [API Key]          │ ───▶ │ ● 运行中 · 上游正常  │ ──▶ │    转换 → 上游 Chat        │
│  │ 上游: OpenAI Chat   │      │ 自动启动 [开关]      │     │ ✅ Codex    /v1/responses [复制]│
│  │ openrouter.ai/api…  │      │                     │     │    转换 → 上游 Chat        │
│  └────────────────────┘      └─────────────────────┘     │ ☐ Grok     /v1/responses [复制]│
│                                                           │    可一键接入               │
│                                                           └───────────────────────────┘
│  模型名单：仅放行 stealth/ox-alpha（未列出的模型将被拒绝）        [将勾选项写入客户端配置] │
│                                                                              │
│  ── 以下条件出现 ──                                                          │
│  ⚠ 来源已删除 / 需要处理（recovery） / 操作失败原文                            │
│  👥 同来源成员（仅多成员时）                                                  │
│  ▸ 诊断信息（默认收起）                                                       │
│                                                                              │
│  [解除绑定]                                                    [收起]        │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 左：来源节点

- 行 1：AgentDot + 登录名（`resolveAdapterProfileSource`）
- 行 2：凭证类型 Badge（`adapterCredentialKindLabel`）
- 行 3：上游协议标签 + 外部 endpoint URL（截断；全文进诊断）
- 多 endpoint（如 GLM per-target URL）时：左节点只显示 base URL，per-target URL 标在右列对应边上
- 来源已删：节点降灰 + 警告；右列写操作全禁

### 中：本机桥节点

- `host:port`（port pending 时显示「分配中」，不显示 `{port}` 字面量）
- 运行状态与上游状态合一行：「运行中 · 上游正常」
- autoStart 开关放在此节点（桥级属性）

### 右：下游边列表

每条边包含：

1. 状态符号（见下节枚举）
2. 客户端名（Claude / Codex / Grok；必要时 Kimi/DSH 只读行）
3. 本机 URL（可复制）
4. 转换标注：`直通` / `转换 → 上游 X` / 降级 `转发`
5. 边状态文案

### 直通 / 转换判定

- **上游协议推导**：openai-compat / OpenRouter 按 per-target endpoint URL（含 `/anthropic` 或 `api.anthropic.com` → Anthropic Messages，否则 OpenAI Chat，与 [local-route-endpoints.md](local-route-endpoints.md) 一致）；订阅来源按凭证种类映射固定通道。
- **直通**：`messages×Anthropic`、`responses×Codex/Grok Responses`、`chat_completions×OpenAI Chat`；其余为转换。
- **降级**：来源已删或 config 解析失败时只显示「转发」，不猜协议。

---

## Agent 支持判断矩阵

每条下游边状态互斥，按优先级从上到下判定：

| 状态 | 展示 | 交互 | 判定条件 |
|---|---|---|---|
| `source_missing` | 边降灰 +「来源登录已删除」 | 复制可用，写操作全禁 | `resolveAdapterProfileSource().missing` |
| `hidden` | 「该客户端已在设置中隐藏」 | 禁写，复制可用 | 该边 target ∈ `hiddenTargetIds`（**逐边**；现状只按 `profile.targetAgentId` 判一次） |
| `no_upstream` | 「来源未配置此客户端的上游端点」 | 全禁，不进 Quick Apply | `capabilities.endpoints` 非空且无该 target 的 enabled 行；capabilities 为空时按 surfaces 回退，不产生此态 |
| `applied` | ✅ +「已写入 Claude 配置」 | 复制可用；Apply 中可勾选重写 | 同来源存在 `targetAgentId === 该边` 且 `generatedProviderId` 非空的 profile（需兄弟 profiles 或 per-target applied map） |
| `ready` | ☐ +「可一键接入」 | 可勾选 Apply | 边在 surfaces 中、无上述禁用、且无已写入记录 |
| `runtime_only` | 只读 +「由后端路由支持，暂不提供界面配置」 | 仅复制 | kimi/dsh 等 **仅当已有绑定时**显示；无绑定不展示、不引导 |

### 判定数据源

| 数据 | 来源 |
|---|---|
| 配置声明的上游端点 | `sourceEntry.provider.configText` → `readCreateRouteCapabilities().endpoints` |
| 下游 surfaces | `listLocalRouteSurfacesFromConfig` |
| 隐藏判定 | 页面级 `hiddenTargetIds`（store-stamp，只影响界面） |
| 已写入判定 | 同 `sourceKind`+`sourceId` 的 profiles：`targetAgentId` + `generatedProviderId` |
| 运行 / 上游状态 | `bridgeStatus.state` / `upstreamStatus` |
| 成员健康 | `bridgeMemberRows` |
| 上游协议 | 凭证种类 + per-target URL 协议识别 + `ruleId` 佐证 |

**配置声明 + 写入情况在右列；listener 是否在服务只在中节点。** 桥停止时右列不降灰，中节点提示「已停止——客户端暂时无法使用以下地址」。

### 实现前数据流缺口

详情目前拿不到「同来源兄弟 profiles」，也只收到单个 `targetHidden` 布尔。落地时页面层需下传：

- profiles 集合，或预计算的 per-target `applied` map；
- 整套 `hiddenTargetIds`（逐边判定）。

---

## 现状区块处置（对照 A–L）

| 区块 | 处置 |
|---|---|
| A Header（来源+箭头+URL 列表+Badge+已删） | **拆解**：来源/Badge/已删 → 左节点；URL → 右列 |
| B 状态 | **并入中节点**，与 upstreamStatus 合一行 |
| C capabilities #1 | **删除**，上游信息并入左节点与边标注 |
| D members | **保留**，仅 ≥2 成员时出现，移至条件区 |
| E capabilities #2 | **删除** |
| F localEndpoint | **拆解**：URL → 右；host/port → 中；upstream → 中；autoStart → 中 |
| G quickApply | **与右列合体** |
| H targetWrite | **删除独立句**，升级为右列 `applied` |
| I recovery | 条件区保留 |
| J error | 条件区保留 |
| K diagnostics | 默认收起；增加上游 URL 全文；有 `lastErrorCode` 时 summary 提示 |
| L footer | 保留（解除绑定 / 收起） |

**列表行保留**：状态 + 来源名 + surfaces URL 摘要 + 启停 + 一键配置 + 详情按钮。

模型名单收敛为关系图下方一行：「仅放行 …」或「跟随客户端请求的模型」。

---

## 交互细则

- **复制端点**：右列 URL 可点即复制（沿用 toast）；port pending 时复制禁用，悬停「端口分配后可复制」。列表行复制行为一致。
- **Quick Apply**：右列勾选框即目标选择（仅 `ready` / `applied` 可勾；`applied` 默认勾选表示可重写）；按钮「将勾选项写入客户端配置」；0 勾选禁用；成功后边即时转 ✅。
- **autoStart**：中节点内开关；`targetHidden` 或 busy 时禁用并带原因。
- **members**：仅 `bridgeMemberRows` ≥2 时出现。
- **recovery**：仅 `needs_attention`；条件区最上；列表行可保留一句摘要引导。
- **diagnostics**：始终存在但默认收起。

---

## 空态与异常

| 场景 | 表现 |
|---|---|
| 来源已删除 | 左降灰 +「来源登录已删除，路由仅可查看或解除绑定」；右禁写可复制；Apply 禁用；解除绑定可用 |
| 状态不可用 | 中节点「状态暂不可用」（中性灰，非错误红）；右列与转换标注照常 |
| 端口 pending | 「127.0.0.1 · 端口分配中」；右列路径 +「端口分配中」；复制禁用；无 `{port}` 字面量 |
| needs_attention | 条件区顶部 recovery；中节点警示色 |
| 无 surfaces | 按 `targetAgentId` 回退单边，不显示空列表 |
| targetHidden | 对应边说明已隐藏；当前 profile 自身 target 隐藏时 autoStart / 解除绑定同现状禁用 |

---

## 文案语气

统一说「登录」，不说「票」。示例：

- 边状态：「已写入 Claude 配置」「可一键接入」「来源未配置此客户端的上游端点」「该客户端已在设置中隐藏」
- 转换标注：「直通上游」「转换 → 上游 Chat 接口」「转发」
- 来源删除：「来源登录已删除，路由仅可查看或解除绑定」
- 模型名单：「仅放行：a, b（其余模型将被拒绝）」/「跟随客户端请求的模型」
- Quick Apply：「将勾选项写入客户端配置」
- runtime_only：「由后端路由支持，暂不提供界面配置」

用户可见文案不出现 profile / surface / edge / bridge 等内部术语（诊断区除外）。

---

## 验收标准

- [ ] 详情内每类信息只出现一次：原 C、E、F、H 独立区块不复存在
- [ ] 首屏为三节点关系图：左来源、中桥、右下游边
- [ ] 右列每条边含：状态符号、客户端名、可复制本机 URL、直通/转换标注、状态文案
- [ ] 直通/转换符合本文判定；推导失败降级为「转发」
- [ ] 六种边状态全部可达且互斥；禁用有中文原因；`hidden` 逐边判定
- [ ] `applied` 按同来源兄弟 profiles 逐边判定
- [ ] kimi/dsh 仅既有绑定时只读出现；无绑定不展示不引导
- [ ] Quick Apply 与右列合一；0 勾选禁用；写入后边状态即时更新
- [ ] 桥停止时右列不降灰；status 读取失败显示中性「状态暂不可用」
- [ ] port pending：占位文案 + 复制禁用；无 `{port}` 字面量
- [ ] recovery / error / members(≥2) / 来源已删仅条件出现；diagnostics 默认收起且含上游 URL 全文
- [ ] 文案说「登录」不说「票」；窄屏纵向降级不丢信息
- [ ] 未触碰后端协议矩阵、凭据加密、国产 OAuth

---

## 相关代码与文档

| 路径 | 角色 |
|---|---|
| `src/pages/bridges/AdapterProfileDetailDialog.tsx` | 现行详情实现（改造主战场） |
| `src/pages/bridges/AdapterProfilesList.tsx` | 列表行 + 展开入口；需下传 applied/hidden 集合 |
| `src/pages/bridges/create-route-flow.ts` | `readCreateRouteCapabilities` / `listLocalRouteSurfacesFromConfig` |
| `src/pages/bridges/adapter-view-model.ts` | 运行状态、来源解析、recovery |
| `src/pages/bridges/adapter-member-model.ts` | members 行 |
| `src/lib/route-endpoints.ts` | 下游 surface 路径与绑定映射 |
| [local-route-endpoints.md](local-route-endpoints.md) | 协议转换真源 |
| [archive/route-endpoint-audit-2026-08.md](archive/route-endpoint-audit-2026-08.md) | 审计快照 |
| [bridges-page-redesign.md](bridges-page-redesign.md) | 页面级 IA |

---

## 落地建议（实施轮次，本文不执行）

1. 纯函数 view-model：边状态枚举、直通/转换标注、per-target applied/hidden（并列 `*.test.ts`）。
2. 关系图 UI 组件替换详情主体；列表行暂不动。
3. 页面层下传 `hiddenTargetIds` + 同来源 profiles / applied map；收口 Quick Apply。
4. 回写本文状态为 Implemented，并同步 [ui-design.md](ui-design.md) 线框。
