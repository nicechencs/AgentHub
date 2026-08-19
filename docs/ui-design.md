# AgentHub 前端 UI 设计

> 对应《架构拆分》§4。技术：React 18 + TS + Vite + Tailwind + shadcn/Radix（唯一 UI 体系）+ CodeMirror 6 + recharts + react-router。  
> **实际依赖**以根目录 `package.json` 为准：**未**引入 TanStack Query / i18next / react-hook-form / zod；页面用本地 state + `lib/api`。GUI 语言为轻量自研字典（`src/lib/i18n/`），不引入 i18next。  
> 范围：**八家** Agent（Claude / Codex / Kimi / Grok / Pi / WorkBuddy / **Cursor Agent** 半套 CLI / **DeepSeek Harness**）；**不支持基于 Cursor IDE 私有库的账号池**。Dashboard 与侧栏按 `AGENTS` 自适应，不写死数量。  
> v1.1：Usage 模型筛选语义、Backups 流程、Dashboard/侧栏与当前 agent 集合对齐。  
> v1.3：Agents / 首次引导增加 **「环境未就绪」** 态；安装链路先 Runtime 再 Agent。  
> v1.4：环境条/安装预览按宿主平台分流——macOS/Linux 不展示 PowerShell；native 命令预览 Windows=`irm|iex`、macOS/Linux=`curl|bash`；Runtime 修复默认 Windows=`winget`、macOS=`brew`、Linux=`manual`。  
> 2026-08-14 Hub Phase 1：推荐入口为 Dashboard「连接/切换」与 Connections「接到…」，统一 `ConnectFlowDialog`。  
> 2026-08-15：Connections 全局钱包与真票「接到…」、Dashboard 当前绑定读模型已落地（见 [connection-binding-model.md](connection-binding-model.md) §5–§6 第 1 步）；ConnectFlow 确认步走 `bind`，本机路由解绑走 `unbind`。用户表面是 **Routes / 本机路由**（`/routes`）；内部模块仍叫 Adapter。下文 §4.1 / §4.3 为目标线框。  
> 把已有登录接到另一个工具的产品文案按三种做法（① 直接改配置 / ② 写进对方认的登录 / ③ 本机转发），见 [product-decisions.md](product-decisions.md)。预览不得把 ①② 写成「需要本机服务」。

## 1. 设计原则

1. **以 Agent 为筛选维度，以功能为导航维度**：侧边导航分为 Workspace（Chat / Agents / Skills / MCP / Projects）与 Manage（Dashboard / Connections / **Routes**（有本机路由才出现） / Settings）；备份在 Settings `?tab=backups`，不占侧栏。用量合并进 Dashboard。功能页内部用 AgentTabStrip（随 `AGENTS`）过滤，而不是「先选 app 再选功能」的两层切换。**例外：Connections 目标态是跨工具钱包**（一份份登录），Agent 只作筛选/高亮，不作第一导航；见 §4.3 与 [connection-binding-model.md](connection-binding-model.md)。底层 accounts/providers 可继续分表，UI 与规划器谈的是登录和绑定。连接从 Agent 卡片或钱包「接到…」发起；`/routes` 只做本机转发运行时（旧 `/adapter`、`/router`、`/bridges` 永久跳过来）。
2. **危险操作必有前置信息**：切换供应商/账号前展示 backfill 摘要、备份位置、运行中进程警告。
3. **凭据永不明文回显**：`SecretInput` 统一脱敏回显（`sk-••••3f2a` 一类掩码）；点眼睛切换明文。现行实现无二次确认、无自动再遮蔽。聚焦已遮蔽值会清空以便重新输入。
4. **空状态给动作**：每个空列表都有明确的下一步按钮（添加供应商/导入账号/安装 Agent / 安装运行环境）。**例外：Routes 健康空态没有按钮**——多数连接不需要本机转发，空是常态，不是待转化漏斗。
5. **能力不齐是常态**：Kimi 无技能目录、部分账号切换受限等，用 Tab 置灰 / 单元格 `—` / partial 态表达，禁止整页白屏。
6. **先环境后 Agent**：未满足渠道前置（如缺 Node）时，主按钮是「安装环境 / 查看修复步骤」，不是假装可装 Agent；装完环境后自动「重新检测」再解锁 Agent 安装。

## 2. 设计 Token

> **色值真源**：`src/styles/tokens.ts`（一处修改，处处生效）。  
> 运行时以 CSS 变量暴露；`tailwind.config.ts` 只映射 `var(--…)`；`globals.css` 只放结构样式。  
> Vite 插件把 token 注入 `virtual:agenthub-design-tokens.css`，并把 boot 子集写入 `index.html` 标记区。  
> 浅色优先（Cursor 桌面风），深色备选；靠明度分层，不靠边框堆叠。  
> **体验与视觉执行细化**（对标 Cursor/Codex 的层级、提示、预览、分期改造）见 [ui-experience-alignment.md](ui-experience-alignment.md)。  
> **组件用法、决策树与现行清单**见 [ui-component-standard.md](ui-component-standard.md)；本文 §5 只保留索引，细节以标准为准。  
> 冲突时：业务规则以本文为准，token 收敛以对标文档为准，组件选用以标准为准。

```
主题:浅色优先,深色备选(.dark) —— hex 以 tokens.ts 为准，下列为语义名
画布:  --bg-canvas / 面板 --bg-panel / 弱底 --bg-subtle / 悬停 --bg-hover / 激活 --bg-active
边框:  --border、--border-strong
文字:  --text-primary / --text-secondary / --text-muted / --text-disabled
强调:  --accent（focus ring / 链接 / checked / 每页至多一个 accent 主按钮；Phase 0 保留 indigo，只降暴露）
状态:  --success / --warning / --danger / --info
Agent 品牌色（logo 点、图表系列；改 tokens.ts 的 AGENT_COLORS）:
  --agent-claude / --agent-codex / --agent-kimi / --agent-grok
  --agent-pi / --agent-workbuddy / --agent-cursor / --agent-dsh
  TS 取 hex：agentHex(id) ；样式绑定：agentCssVar(id) 或 AGENT_MAP[id].color
字号:  仅三档，真源 `TYPE_SCALE`（`src/styles/tokens.ts`）— 16 `text-title` 页标题/空态主句/指标 · 13 `text-body` 正文/按钮/列表名/段标题 · 12 `text-meta` 次级/表头/路径/角标。`text-lg`/`text-xl`=`title`，`text-sm`/`text-base`=`body`，`text-xs`/`text-2xs`=`meta`（同像素别名，禁止再长出第四档）
圆角:  6px `rounded-btn` 控件 / 8px `rounded-card` 卡片·弹层 / 12px `rounded-composer` 输入壳·聊天气泡；chip 与头像用 `rounded-full`（禁止魔法 `rounded-[Npx]`）
阴影:  `shadow-xs` 卡片 / `shadow-sm` 轻浮层(Tooltip、分段选中) / `shadow-md` popover·菜单·Toast / `shadow-lg` Dialog
间距:  4 / 8 / 12 / 16 / 24 / 32（优先整数阶梯；列表卡 `p-3`、组合 Card 水平 `px-4`、Dialog `p-6`）
边缘:  常规页 / TopBar / Skills 工作台水平 **24**（`pageRhythm.pageShell` / `workbenchX` / `pageEdgePx.x`）；Chat 主区 chrome **16**（`chatChromeX`）；Skills 预览距右缘 **24**、上下 **12**
表格:  `TableShell variant` 决定密度（Context 自动套表头/行/单元格）。全站管理表（含 Skills 三 Tab、Dashboard 用量）统一 `default`=Card 壳；`workbench`/`flush` 仅留给无 Card 的特例贴边场景。禁止业务侧手写 `*Workbench` class
焦点:  输入类 `focus:ring-2 focus:ring-accent/60`（Input / Select 一致）
控件: Switch 选中态 = accent（`data-[state=checked]:bg-accent`）
```

**主按钮（accent / `variant="default"`）**：一页至多一个主 CTA；卡内安装/次级行动用 `secondary` / `ghost`（需边界时用 outline），避免多枚 accent 并排抢视线。Phase 0 保留 indigo 色相，只降低暴露；是否换色见 `ui-experience-alignment.md` Phase 5。

**Button（`components/ui/button.tsx`，节奏真源 `BUTTON` in `tokens.ts`）**：

| 轴 | 标准 |
|---|---|
| 角色（6） | `default` 页级 CTA（≤1）· `secondary` 实心底 · `outline` 有边 · `ghost` 无铬 · `danger` / `dangerOutline` 破坏 |
| 高度（2） | `sm` / `default` / `icon` = **28**（`h-7`）· `lg` = **32**（`h-8`）。水平 pad **8 / 12 / 16**（`px-2` / `px-3` / `px-4`） |
| 悬停 / 按下 | **只改底色与字色**。原语锁 `shadow-none hover:shadow-none active:shadow-none`。禁止 `hover:shadow-*`、禁止 `px-2.5` / `px-3.5` |
| 焦点 | `focus-visible:ring-2 ring-accent/60`（与 Input / Select 一致） |
| 禁用 | `opacity-50` + 无指针 |

不是 Button 的表面（不要为了「统一阴影」去改）：

- **分段选中抬起**：`SegmentedControl` / `Tabs` / `AgentTabStrip` 的 *active* 项用 `shadow-sm`，hover 未选项只洗底。
- **浮层**：Tooltip `shadow-sm`、菜单 `shadow-md`、Dialog `shadow-lg`——不是 hover 才出现。
- **回收站 FAB**：始终 `shadow-md`（浮在画布上的 overlay），hover 不加减阴影。
- **手写 `<button>`**：链接字、矩阵格、预览 chrome 保持原样；看起来像动作按钮的应改走 `Button` / `buttonVariants`。

**列表选中态**：`ListRow`（`components/shared`）= `bg-active` + 可选左边条；表格预览行用 `TableRow active`。勿把 checkbox 多选画成整行 accent。

**分段控件族**：`SegmentedControl` / `Tabs` / `AgentTabStrip` 共用 `segmented-styles`（灰轨 + 白底抬起）。

**Card 变体**：`default` 有边框阴影；`plain` / `subtle` 用于嵌套与工具条，避免双重描边。

**PageHeader**：标题一律 `text-title`；全高页（Skills）用 `size="compact"` 只收底边距。

**页面区块节奏**（`pageRhythm` / `PageSection` / `pageEdgePx`，`components/layout`）：
- **边缘**：常规页 `pageShell`（`px-6 py-6`）；TopBar `px-6`；Skills `workbenchX` + 预览 `pageEdgePx.x=24`；Chat 主 chrome `px-4`
- Header 自带 `mb-4`（compact `mb-2`）
- 其下工具带 / Agent 条：`chrome` / `chromeRow`（`mb-3`）
- 引导块（环境条、Notice 组）：`lead`（`mb-4 space-y-3`）
- 主列表：`stack`（`gap-3`）或 `stackDense`（`gap-2`）
- 同段卡片块：`blocks`（`space-y-4`）
- 二级段：`PageSection` → `mt-6`；大段分割：`ruled` → `mt-8` + 顶部分割线
- Chat / Skills 全高特例自管布局，但水平 inset 须对齐上表

导航保留英文专有名词（Dashboard / Agents / Skills…），页面标题与正文用中文。

## 3. 全局布局

```
┌──────────────────────────────────────────────────────────────┐
│ Sidebar ~224px       │  TopBar:右侧操作(通知)               │
│ ┌────────────────┐   ├──────────────────────────────────────┤
│ │ ◆ AgentHub     │   │                                      │
│ ├────────────────┤   │                                      │
│ │ Workspace      │   │           Page Content               │
│ │ 💬 Chat        │   │   内容区 max-w-content（1200px）居中   │
│ │ ▦ Agents       │   │   （Chat / Skills 全高特例）          │
│ │ ✦ Skills       │   │                                      │
│ │ ◉ MCP          │   │                                      │
│ │ ◫ Projects     │   │                                      │
│ ├────────────────┤   │                                      │
│ │ Manage         │   │                                      │
│ │ ▣ Dashboard    │   │                                      │
│ │ ⇄ Connections  │   │                                      │
│ │ ▦ Routes       │   │  （有本机路由才出现；Settings 本机区常驻入口）│
│ │ ⚙ Settings     │   │                                      │
│ ├────────────────┤   │                                      │
│ │ ● N/M agents   │   │   (侧栏底部:agent 在线状态迷你条)      │
│ └────────────────┘   │                                      │
└──────────────────────────────────────────────────────────────┘
```

- 常规页：主区 `mx-auto max-w-content`（theme token = **1200px**）+ 内边距；含 TopBar。
- **Chat 特例**：无 TopBar / 无 PageHeader，主区 `h-full` 全高铺满，不受 `max-w-content` 限制。
- **Skills 特例**：保留页内 Header/Tabs，但与 Chat 一样走 `fullBleed`、主区 `h-full overflow-hidden`；列表与右侧预览在页内独立滚动，不受 `max-w-content` 限制。
- 侧栏底部迷你状态条：品牌色圆点（数量随 `AGENTS`）+ 已安装计数，hover 显示各 agent 版本；有更新时圆点带黄环。折叠/展开态均允许换行，避免 agent 增多时溢出。

## 4. 页面设计

### 4.1 Dashboard（总览 + 用量）

上半 Agent 总览 + 共享筛选下的一套指标/趋势/分布；下半用量明细（模型筛选、明细表、UsageParserHealth）。`/usage` 永久重定向到 `/?section=usage` 并滚动到用量段。

Agent 总览区（`AgentOverview`）使用 `auto-fit + minmax(190px, 1fr)` 自适应网格，**支持任意数量 agent**，不写死列数；骨架屏复用同一网格定义，loading → 内容无跳动。

```
┌─ Dashboard ────────────────────────────────────────────────┐
│ Agent 总览  3/4 就绪 · 1 项待处理              [管理]      │
│ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐       │
│ │● Claude  ●│ │● Codex   ●│ │● Kimi    ●│ │● Grok    ●│       │
│ │官方 · v2… │ │xx云·兼容路由│ │官方 · v0… │ │官方 · v0… │       │
│ └──────────┘ └──────────┘ └──────────┘ └──────────┘       │
│  （只渲染已安装；点击卡片=连接/切换；N>4 折行，列宽≥190px）│
│ 时间 [近 7 天 ▾] Agent [全部 ▾]              [⟳ 立即采集] │
│ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐               │
│ │ 输入    │ │ 输出    │ │ 缓存命中│ │ 估算成本│               │
│ └────────┘ └────────┘ └────────┘ └────────┘               │
│ ┌─ 近 7 天 Token 用量(堆叠面积) ─────┐ ┌─ 快捷操作 ────────┐│
│ │  [按 agent 分品牌色]               │ │ ⇄ 切换连接         ││
│ │  合计:1.2M in / 380K out / ≈$2.0  │ │ ⛁ 立即备份         ││
│ └────────────────────────────────────┘ └───────────────────┘│
│ ┌─ Agent/模型 用量分布条 ──────────────────────────────────┐│
│ └──────────────────────────────────────────────────────────┘│
│ ── 用量明细 ──────────────── 模型 [全部 ▾] ──────────────── │
│ ┌─ 明细表(最多 50 条) ─────────────────────────────────────┐│
│ │ 时间 │ Agent │ 模型 │ 输入 │ 输出 │ 缓存读 │ 成本 │ 会话   ││
│ └──────────────────────────────────────────────────────────┘│
│ 数据源状态:claude ✓ · codex ✓ · kimi ⚠ · grok ✓             │
└──────────────────────────────────────────────────────────────┘
```

- Agent 卡片**只渲染已安装 Agent**（未安装去 Agents 页；未装卡不出「连接/切换」）。两行紧凑：第一行 logo + 名称 + 版本 + 右侧认证状态点（绿=已认证/黄=即将过期/红=失效/灰=未配置）；第二行当前连接 meta。顺序固定为 `AGENTS` 定义序，不按状态重排。
- **主动作「连接/切换」**（点击卡片）：打开统一绑定对话框（现名 `ConnectFlowDialog`），目标固定为该编程工具，选一份登录。目标语义是 `bind(这份登录, 此工具)`，不是「在两套池里挑一行」。
- 徽标（目标态按**当前绑定**展示，过渡期仍可用 profile 联结）：
  - **① 直接改配置 / ② 写进对方认的登录**：`route=reshape`（或命中生成投影）。文案用「只改配置」或「写进对方认的登录」，不要显示转发。
  - **③ 本机转发**：仅当前生效的 ③ 显示桥徽标；查询失败显示「状态不可用」，不得静默隐藏。点击徽标进入 `/routes?profile=`（无 id 则 `/routes`），tip「管理本机路由」（`stopPropagation`）。孤立 / 非当前桥没有徽标。
- **ConnectFlow（工具侧）**：两组来源——**本来就是给它的登录**（走切换）+ **其他登录**（`plan(ticket, agent)`）。可执行权威是 `plan()` 的 route / maturity / canApply / reason（`canApply` 表示现在能写入）。预览必须标出三种做法之一；② 不得出现「需要本机服务」或转发启停。接不上的登录**留在列表**，置灰 + 原因原文，不从 Connections 藏起来再让本页单独诊断。OAuth 未完成：引导去钱包补登录。空态与「导入登录态 / 新 API Key」走深链 `intent=import-login|add-key`；成功后回 `/?connect=` 重开。导入仍是读官方 CLI 已完成的登录。
- **共享筛选**（时间 + Agent）驱动一套指标卡与趋势图；筛选变更时 `queryUsage` / `usageTrend` 各请求一次，上下共用 records。
- 用量图：堆叠 Area，按 agent 分色。选中单 agent 时分布条下钻到**按模型**拆分。
- Agent 总览与用量分区处理 loading/error：用量失败不白屏上半。

### 4.2 Agents（安装管理）

```
┌─ Agents ───────────────────────────────────────────────────┐
│ 环境条(可折叠): Node v20.11 ✓  npm ✓  [Windows: PS ✓] Git ✓  [重新检测] │
│ ┌────────────────────────────────────────────────────────┐ │
│ │ [logo] Claude Code      v2.1.218  最新 v2.2.0 ↗        │ │
│ │ 路径:（detect 结果）   渠道:native                     │ │
│ │ [升级] [打开配置目录] [卸载 ▾(含"同时删除配置")]        │ │
│ ├────────────────────────────────────────────────────────┤ │
│ │ [logo] Codex            未安装 · 环境未就绪 ⚠          │ │
│ │ 渠道:npm  缺少: Node.js (≥18)                          │ │
│ │ [安装 Node.js ▾] [复制修复命令] [安装 Agent(禁用)]     │ │
│ ├────────────────────────────────────────────────────────┤ │
│ │ [logo] Grok             未安装 · 环境就绪              │ │
│ │ [安装 ▾] 渠道:官方脚本                                  │ │
│ └────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────┘
```

卡片状态机（每 agent × 所选渠道）：

| 态 | 展示 | 主行动 |
|---|---|---|
| `installed` | 版本 / 路径 / 可升级 | 升级 / 卸载 / 打开配置目录 |
| `ready_to_install` | 未安装 · 环境就绪 | **安装 Agent**（展开 InlineTerminal） |
| `env_missing` | 未安装 · 环境未就绪 ⚠ | **安装环境** / 复制命令；「安装 Agent」禁用并 tooltip 说明缺什么 |
| `env_installing` | 安装环境中… | InlineTerminal 流式输出；可取消（尽力） |
| `agent_installing` | 安装 Agent 中… | 同流式面板 |
| `env_outdated` / `broken_path` | 警告条 | 升级 Node / 修复 PATH / 重启 AgentHub 提示 |

- 页顶 **环境条**：共享 Runtime 总览（与 doctor 同源，**仅宿主相关**）；Windows 可显示 PowerShell 5.1/7 芯片，**macOS/Linux 不展示 PowerShell**。任一 Agent 的 npm 渠道依赖 Node，装一次全局受益。
- 每 agent 一张宽卡片：版本、渠道（npm/native）、二进制路径、可升级标记、**渠道前置状态**（native 在 Unix 上不得要求 PowerShell）。
- **安装 Agent**：仅 `ready_to_install` 可点；core 仍会二次 `ensure_env`，防止 UI 竞态。安装/升级预览可附带平台底层命令（Windows `irm…|iex` / macOS·Linux `curl…|bash`）。
- **安装环境**：展开 InlineTerminal；默认包管理器 **Windows=`winget`、macOS=`brew`、Linux=`manual`（无自动 sudo；`apt` 等渠道同样只给可复制命令）**，失败则展示官方下载链接 + 可复制命令 +「我已装好，重新检测」。禁止跨平台展示错误的包管理器命令；Linux 未知发行版不猜测 `apt-get`。
- PATH 刚刷新场景：检测仍失败时 toast/横幅提示「请完全退出并重启 AgentHub 后再检测」（GUI 进程继承旧 PATH）。
- 卸载：DropdownMenu 二级——「仅卸载程序」/「卸载并删除配置」（后者红字 + 输入 agent 名确认 + **自动 pre-uninstall 备份**）。**不提供「卸载 Node」**（共享运行时）。

### 4.3 Connections（连接 = 钱包）

侧栏只保留一个入口。目标态：**跨工具的登录列表**，不是按工具切开的两套池。底层 accounts/providers 可继续分表；UI 谈一份登录和「正用于」哪些绑定。生成投影不进本页。完整规则见 [connection-binding-model.md §5](connection-binding-model.md#51-两个入口一个对象)。

路由：`/connections`；`/connections?agent=codex` 高亮该 Agent 的 active 绑定（**不**再把整页收成该 Agent 私有列表）；`?mode=` 仍可筛到 API Key。旧 `/providers`、`/accounts` 重定向至此。

**目标线框：**

```
┌─ 连接 ─────────────────────────────────────────────────────┐
│ 钱包 · n 份登录                               [+ 添加]     │
│ 筛选 [全部] [官方登录] [API Key] [未识别]                   │
│ ● Kimi 会员     [API Key] [会员]                           │
│   正用于：Claude（只改配置）· Codex（本机转发 · 运行中）     │
│   [接到…] [详情]                                           │
│ ○ Anthropic     [API Key] [官方]                           │
│   正用于：Pi（改配置）                                     │
│   [接到…] [详情]                                           │
│ ○ me@…          [官方登录]                                 │
│   正用于：Claude（切换）                                   │
│   [接到…]  → 不可行目标在对话框置灰 + 原因                 │
└────────────────────────────────────────────────────────────┘
```

- 「正用于」来自绑定：native / reshape / bridge，不是手写 account/provider 出身。
- **每一份真登录都有「接到…」**，打开同一绑定对话框（登录固定，选工具）。接不上、工具不能写入、未识别：对话框内置灰 + 原因，不在行上隐藏动作。选目标后预览标 ① 只改配置 / ② 写进对方认的登录 / ③ 本机转发，不要把订阅默认写成转发。
- 「切换」只用于这份登录对它本来所属工具的 native 绑定。接到其他工具一律走 `bind`。
- 添加登录时写下它是哪一家。API Key 默认勾选官方端点 → 带出官方 URL + 模型；取消后可填自定义（未识别则标 `unknown`，不假装可接到任意工具）。**Pi 无单一官方 URL**：弹窗选厂商槽，官方槽（anthropic / openai / …）只写 `~/.pi/agent/auth.json`，自定义 URL 写 `models.json`。
- **已落地（读模型 + 写入）**：跨工具钱包列表 + 真登录常驻「接到…」+ Dashboard 当前绑定；生成投影不进钱包。确认步走 `bind`，成功以该工具的当前绑定为准，见 [connection-binding-model.md](connection-binding-model.md) §6。
- 导入当前登录时若本机同时有 Key 与官方登录，对话框警告条说明当前会收入哪一份。
- 实现落点：`TicketWalletList` / `ticket-wallet-model` / `lib/api/tickets`；`reuse-offer` 为登录常驻「接到…」语义。

#### 4.3.1 mode=providers — API 配置（历史线框 / 过渡形态）

> 当时按 Agent tab + `mode=providers` 的左右栏编辑器。**现行是全局钱包**（见 §4.3 目标线框），不再以本线框为产品契约。

```
┌─ API 配置 ─────────────────────────────────────────────────┐
│ ┌──────────────┐  ┌──────────────────────────────────────┐ │
│ │ ● 官方(当前)  │  │ 供应商编辑 / 详情                     │ │
│ │ ○ xx云中转    │  │ 名称 / 预设 / ConfigEditor            │ │
│ │ ○ 自部署      │  │ [保存] [测速] [切换到此供应商 →]       │ │
│ │ [+ 添加][导入]│  │                                      │ │
│ └──────────────┘  └──────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────┘
```

- 左列供应商列表（当前项带品牌色左边条 + ●），右列编辑器。预设下拉来自 `src/config/presets/`（模板内可含默认 `model` 字段，属**配置编辑**，不是 Usage 的模型筛选列表）。
- 敏感字段在编辑器内也脱敏显示，编辑时替换；保存走 `MergePreservingSensitiveCreds` 语义（未改动的密钥不被覆盖）。
- **切换确认对话框**（核心交互）：

```
┌─ 切换到 "xx云中转"? ─────────────────────────┐
│ ✓ 当前 live 配置将回存到 "官方"(backfill)     │
│ ✓ 当前 Agent 的 live 配置将先备份            │
│ ⚠ 检测到 claude 进程正在运行,切换后需重启生效 │
│                              [取消] [确认切换] │
└───────────────────────────────────────────────┘
```

#### 4.3.2 mode=accounts — 账号与密钥（历史线框 / 过渡形态）

> 当时按 Agent tab + `mode=accounts` 的账号卡列表。**现行是全局钱包**（见 §4.3 目标线框），OAuth / API Key 仍是来源类型，但不再以本线框为第一导航。

```
┌─ 账号与密钥 ───────────────────────────────────────────────┐
│ 筛选: [全部] [官方登录] [API Key]          [+ 添加账号/密钥]│
│ ┌────────────────────────────────────────────────────────┐ │
│ │ ● user@example.com [官方登录] ChatGPT Plus 5h ▓▓▓░░ 62%│ │
│ │   token 有效(剩余 4h12m)                  [详情 ▾]      │ │
│ ├────────────────────────────────────────────────────────┤ │
│ │ ○ work@example.com [官方登录] Team        [切换][详情]  │ │
│ ├────────────────────────────────────────────────────────┤ │
│ │ ○ sk-••••9f2a   [API Key]                 [切换][详情]  │ │
│ └────────────────────────────────────────────────────────┘ │
│ 添加 ▾: OAuth 登录 / API Key / 导入当前登录态              │
└────────────────────────────────────────────────────────────┘
```

- 账号卡：类型徽章（官方登录 / API Key）、邮箱/标签、订阅等级、配额条、token 有效期、状态点。
- 筛选：全部 / 官方登录 / API Key；换 agent 时重置为全部。
- 切换：同 Providers 的确认对话框（backfill 当前凭据 + 备份 + 进程警告）；跨池 demote 供应商 current。
- OAuth 添加：对话框展示进度三步——① 打开浏览器授权 ② **等待回调**（loopback 倒计时；**复制授权链接** + **手动粘贴回调 URL** 双降级） ③ 成功显示邮箱 + 订阅等级。
- 不支持账号切换的 agent 在 mode=accounts 时 Tab 置灰，提示改用「API 配置」。

#### 4.3.3 Routes（本机路由）

用户表面是 **本机路由运行时**：协议对不上时在这台电脑上开的一层转发。登录在 Connections，绑定在 Dashboard / ConnectFlow；本页只服务 ③。内部模块仍叫 Adapter（`lib/api/adapter`），不得漏进侧栏、页标题、空态、确认框、徽标、托盘。

规范路由 `/routes`。`/adapter`、`/router`、`/bridges` 永久 `replace` 过来（丢弃遗留 `?tab=`）。侧栏英文 **Routes**，仅当本机确有 `local_bridge`（含孤立）或钱包仍有 `route=bridge` 时出现；Settings → 本机有一条永远在的「本机路由运行时」回收链。页头无「去 Dashboard / 去 Connections」。创建区不在本页。

列出全部 `route=local_bridge`：来源仍在或 last-known binding 命中的进主列表；其余非空 `sourceId` 进「孤立本机路由」。行与详情都是**单层**进程健康 + 端口，不画「配置已生效 / 桥接运行中」。解绑只走 `unbindTicket`，不提供 `removeAdapter`。健康空态（profile 与钱包均已结算且 `bound+orphan===0` 且 last-known 钱包桥数为 0）标题「没有本机路由」，**无按钮**——这是对 §1.4 的显式例外。

- 与 ConnectFlow 两处 bind 同源（`lib/api/adapter` / `lib/api/tickets`）；可执行权威是 `plan.canApply`，禁止以 `analysis.support` 推断可执行。
- 规则名称、route 和可执行状态以[当前实现矩阵](provider-api-oauth-adaptation.md#4-当前实现矩阵)为准。
- 厂商/API/OAuth 规则见 [provider-api-oauth-adaptation.md](provider-api-oauth-adaptation.md)；页面与 Bridge 设计见 [adapter-design.md](adapter-design.md)、[bridges-page-redesign.md](bridges-page-redesign.md)。

### 4.4 Chat（会话）

全高特例布局（`App` 中 `pathname === '/chat'`）：**无 TopBar / 无 PageHeader**，主区 `overflow-hidden` + 子树 `h-full`，会话列表与消息区自行分配高度；**不**套用 `max-w-content` 居中内容壳。其余功能页保持标准壳（TopBar + max-w-content）。本节为产品契约摘要；完整方案（已落地，见 [chat-page-redesign.md](chat-page-redesign.md) Implemented 2026-08）。

**目标线框：**

```
┌───────────────────────┬─────────────────────────────────────────────────┐
│ [◧] [＋ 新建对话]      │ 标题（就地编辑）✎ [◉ Agent] [cwd] [自动批准] ⚙   │
│ [🔍 搜索对话        ]  ├─────────────────────────────────────────────────┤
│ 今天                   │            （消息列 max-w-3xl 居中）             │
│ ▮当前会话（bg-active） │                        ┌ user 气泡（bg-subtle）┐ │
│ ▮ ● cwd 短名          │ 本轮 2 个 Agent：[◉ …] [◆ …]   ← ≥2 时对比条    │
│  其他会话             │ ◉ Claude  12.4s                                 │
│   ● cwd 短名          │ ▸ 已完成 · 6 步 · 12.4s   ← 过程摘要行           │
│ 昨天 / 近 7 天 / 更早  │ 正文（Markdown）                        [复制]   │
│  …                    ├─────────────────────────────────────────────────┤
│                       │ （blocker 引导行：无 cwd / 隐藏 Agent / 他处发送中）│
│                       │ ┌ composer（rounded-composer）─────────────────┐ │
│                       │ │ textarea 自动增高 56–240px                    │ │
│                       │ │ [Agent 多选 ▾] [连接 ▾（仅首位 Agent）]  [➤] │ │
│                       │ └───────────────────────────────────────────────┘ │
│                       │          Agent 可能修改工作目录中的文件           │
└───────────────────────┴─────────────────────────────────────────────────┘
```

**会话 rail**（240px，可收起为 `w-0`，header 左端出现展开按钮）：

- 顶部「新建对话」（secondary）+ `SearchField`（匹配标题与 cwd）。
- 按相对日分组：今天 / 昨天 / 近 7 天 / 更早；组内 `updatedAt` 倒序。
- 行两行结构：标题（空标题显示「新对话」，发送中带状态点）；meta = `AgentDot` 品牌点列（>3 折叠 +N）+ cwd 末段目录名（无 cwd 显示「未设目录」）。完整 cwd 与更新时间进行级 Hint，不常驻。
- 选中 `bg-active`、hover `bg-hover`（与 `ListRow` 语义一致；**禁止**用 `bg-hover` 表示选中）。
- 删除 hover 显示，必经二次确认；发送中的会话删除先取消；删除最后一个自动补建（有可用 Agent 时）。

**会话 header**：标题就地编辑（Enter/blur 保存、Esc 取消、空值回退「新对话」；自动标题仅在 title 为空时由首条 prompt 生成）+ Agent 只读芯片（含「已隐藏」标记）+ cwd 芯片（Hint 完整路径，点击开设置；未设置为 warning 态）+ 自动批准芯片（**仅开启时显示**，warning 态）+ 设置按钮。修改 Agent 只在 composer picker。

**Composer 与发送前置校验**：自动增高 textarea + 底栏（Agent 多选 / 连接切换 / 发送）。Agent picker 只让已安装、未隐藏且已配置授权的项可选；已隐藏 / 未配置授权的项置底（标「已隐藏」/「未配置授权」），不可新增，已在会话内可取消勾选移出。发送前置条件统一为 `sendBlockers` 纯函数，composer 上方渲染第一个 blocker 引导行（含修复动作），优先级：含隐藏 Agent > 未配置授权 > 无 cwd > 他会话发送中；空草稿只禁发送不出引导行。composer 下方仅一行安全提示（批准关：「Agent 可能修改工作目录中的文件」；批准开：warning「自动批准已开启 · Agent 将不经确认修改文件」）。连接切换仅作用于 `agentIds[0]`，多选时固定标注；无连接深链 `/connections?agent=`。发送按钮是页内唯一 accent 主 CTA；发送中变「停止」。

**多 Agent 一轮**：默认纵向堆叠；同轮 ≥2 个 agent 消息时 user 气泡下出「本轮 N 个 Agent」对比条（logo + 状态点 + 耗时，点击定位），**不做左右分栏**。

**过程面板**（展示层规则；协议见 [chat-process-streaming.md](chat-process-streaming.md)）：摘要行 = `阶段 · N 步 · 耗时`（不含命令）；展开 = 无边框步骤时间线（tool/thinking/status/error）；命令 / stderr / exit 收进「运行详情」次级折叠。进行中/失败/超时默认展开，成功/取消默认折叠，用户手动开合记忆到阶段变化。

**消息轻操作**：user / agent 气泡 hover 复制（运行中不显示）；最后一轮失败/取消/超时气泡可「重试」（同一 prompt 作为新 turn 重发给会话全部 Agent，不新增 API）。流式仅在贴近底部时跟随滚动。

**空态矩阵**：无已装未隐藏 Agent 且无会话 → 页级 EmptyState「还没有可对话的 Agent」+「去 Agents 页」；空转录 → 「开始对话」居中引导；无 cwd / 含隐藏 Agent / 他处发送中 → composer 引导行（不再只靠 toast）；Projects bootstrap 无 cwd 不自动弹 Dialog，由引导行接管。

**密度与组件约束**：rail `bg-canvas`，主列 `bg-panel`；header / composer chrome 水平 inset 统一 `pageRhythm.chatChromeX`（禁止硬编码 `px-4`）；消息列 `max-w-3xl`；圆角只用 `rounded-btn` / `rounded-card` / `rounded-composer`；过程仅内存、切会话清空；页面层不 `invoke`。文件拆分按 [chat-page-redesign.md §9](chat-page-redesign.md)（`chat-model` 纯函数 + `use-chat-page` hook + 组件，`index.tsx` 只编排）。

### 4.5 Skills（技能管理）

```
┌─ Skills (fullBleed) ───────────────────────────────────────┐
│ 技能 · 共享库 N · 本工具 M                    [安装]      │
├──────────────────────────┬─┬──────────────────────────────┤
│ Tabs + 过滤（紧）         │┆│ 预览 chrome（单行）           │
│ 列表/矩阵（canvas）       │┆│ 预览|源码 · 打开目录 · 收起  │
│ active 行: bg-active     │┆│ Markdown 文档区              │
│ checkbox 仅多选          │┆│ footer: 路径 meta            │
└──────────────────────────┴─┴──────────────────────────────┘
```

- **真源**在 `~/.agents/skills/`；列为各 Agent 投影状态（由 `skill_service` 维护，Adapter 只提供目录/是否支持）。
- 矩阵：行=技能，列=agent，单元格 ✓ 已同步 / ○ 未同步 / — 该 agent 无技能目录（Kimi）。
- 单元格切换：Windows 优先**复制**；目标已存在且内容不同 → 冲突对话框（覆盖/跳过），默认不静默覆盖。
- 批量：勾选行 + 顶部工具条同步。
- **预览**：单击技能名或 Enter 打开右侧预览；复合 `activeKey`（`shared:id` / `agent:agentId:id`）与 checkbox selected 分离；筛选隐藏目标行时预览仍保留。
- 列表行固定「名称 + 最多一行描述」；绝对路径不进主列（见预览 footer）。
- 插件（Claude `plugins/`、Grok `installed-plugins/`）若展示，与技能矩阵分区或单独只读区，**不混进同步矩阵**。
- 视觉执行细节见 [ui-experience-alignment.md](ui-experience-alignment.md)。

### 4.5.1 MCP（只读清单）

路由 `/mcp`。页面只读扫描各 Agent 已知的 MCP 配置文件，按 Agent 汇总 server、transport、来源文件和启用状态，并允许打开配置目录。

- 当前不创建、编辑、删除或注入 MCP server；页面需明确显示“管理/注入仍为规划能力”。
- 配置缺失、解析失败和空清单分别显示可恢复状态，不得把空结果伪装成“不支持”。
- 只读 inventory 是独立诊断能力，不代表能力矩阵中的 `Mcp` 已从 Planned 变为 Full。

### 4.5.2 Projects（Agent 会话 / 工程记录）

侧栏在 Skills 下方；路由 `/projects`（可 `?agent=claude`）。

导航：**项目树可展开会话**（同页内联，无需下钻）。删除 / 总结作用在会话上；Cursor 为 Partial（仅工作区目录，无 transcript，隐藏删除与总结）。

```
┌─ Projects ─────────────────────────────────────────────────┐
│ 树状展开本地会话                  [总结] [删除] [刷新]      │
│ AgentTabStrip …   Claude · N 个项目                        │
│ 搜索 [____________]                                         │
│ ▸ app  2h前 · 2 会话 · 128KB   actualPath …               │
│ ▼ other …                                                 │
│     ☐ 会话标题  继续 / 删除                                │
└────────────────────────────────────────────────────────────┘
```

- **数据源**：只读扫描各 Agent 本地项目/会话布局（Adapter + `project_service`）；删除仅限 agent home 下已声明安全路径。各家目录与字段映射**不在 UI 文档展开**，以 core 实现为准。
- Cursor 为 Partial：仅工作区目录列表，无会话 transcript；隐藏删除与总结。
- 顶部 **AgentTabStrip**（已安装优先）；搜索可过滤项目或会话。
- **隐藏 / 别名**：写在 AgentHub `data_dir/project_metadata.json`，不改原生日志；「显示隐藏项」开关可找回。
- **删除**：二次确认；按 capability 隐藏不支持的 Agent。
- **打开目录**：优先打开项目工作区路径；无工作区时打开存储路径；会话行可打开 cwd。复用 `open_path_in_file_manager`。
- **继续 Chat**：sessionStorage bootstrap → `/chat?from=projects`。
- **多选总结**：读取会话摘录 → bootstrap（无 transcript 的 Agent 不提供）。
- **不**调用各 CLI 原生续会话能力。
- **性能**：对解析成本高的 Agent 在 AgentHub `data_dir` 做 mtime 索引，避免重复解析会话头。

### 4.6 用量明细（并入 Dashboard）

独立 Usage 路由已取消；能力落在 Dashboard 下半段（见 §4.1）。深链 `/usage` → `/?section=usage`。

- **数据来源**：只读解析各 Agent 本地日志/会话（零侵入），经 `usage_service.collect` 增量入库；非代理流量。
- **「模型」下拉**：`listModels` = 已入库 `usage_records` 的 **DISTINCT model**（实际用过的模型）。  
  - **默认只筛选明细表**，不必扩展 `usageTrend` API。  
  - 不是官方可购模型目录，也不是独立「模型管理」页。  
  - 空库时下拉仅「全部」，引导先「手动采集」。
- 时间 + Agent 为页面级共享筛选；指标卡与趋势图只保留一套。
- 成本为 `pricing` 估价（**与价表同单位，当前为 USD / 1M tokens**；不做汇率换算；价表为离线快照，由 LiteLLM 日更 CI / `pnpm pricing:update` 刷新，App 运行时不拉价），标注「估算」以免误解为账单原件。
- 底部 **UsageParserHealth**（`variant="dashboard"`；兼容旧名 `ParserHealthBar`）：各 parser 采集条数与失败率；失败率高时提示「日志格式可能已变更」。
- **「手动采集」**：与筛选同行；进度条；首次引导用 `StorageKey.usageGuideDismissed`。
- **同步状态文案**（同筛选项右侧）：`上次同步：… · 还有 x 分 y 秒 自动同步` / `仅手动采集`（`UsageSyncProvider` + `usage-sync` 纯函数）。
- **前台自动采集**：`usageCollectIntervalMin`（默认 30；`0` = 仅手动）。App 在前台且 `document.visibilityState === 'visible'` 时按间隔调用 `collectUsage`；切后台暂停；回到前台若已到期则 grace 后补采。有新增条数时 toast「自动同步完成」。
- 设置中可改采集间隔（偏好页变更后立即写入，`notifyUsageSettingsChanged` 立即重排程），并可跳转 Dashboard 用量段。

#### 用量同步 — 已做 / 以后要做

| 状态 | 项 |
|---|---|
| **已做** | 手动采集；前台定时自动采集；上次/下次倒计时；间隔设置生效；增量游标 + raw_hash 去重 |
| **以后** | 系统托盘/开机后台守护（App 未开也能采）；日志目录 **文件监听** 近实时触发；OS 级定时任务；采集失败重试与失败计数 UI；跨设备同步；自动采集静音策略可配置 |

### 4.7 Backups（安全备份）

Settings 子页（`?tab=backups`），**不**占侧栏。页内 tab 中文 **备份**，英文 **Backups**。页内不再重复「已启用 / 自动快照 / 用途说明」；Settings 页头已覆盖分区语义。

独立路由 `/backups` 永久重定向到 `/settings?tab=backups`。旧深链 `/settings#backups`、`/settings?tab=local#backups` replace 到 `?tab=backups`。

```
┌─ 备份 ─────────────────────────────────────────────────────┐
│ [Claude] [Codex] [Kimi] [Grok]   Claude · 3 条记录  [备份] │
│                                                            │
│ ┌ 卡片 ──────────────────────────────── [恢复] [删除] ───┐ │
│ │ 切换前自动  2h前  绝对时间  · 0.4KB                     │ │
│ │ 备注 / 文件路径（最多 3 行）                            │ │
│ └────────────────────────────────────────────────────────┘ │
│ …（当前 Agent 的全部记录平铺，不折叠）                       │
└────────────────────────────────────────────────────────────┘
```

- 顶部一行：AgentTabStrip + 记录数 + **备份**（当前 Tab 的 Agent；未安装禁用）。不另写说明段。
- **AgentTabStrip** = **已安装 ∪ 有备份记录**（装/卸或备份变化会更新 Tab；保持产品序；隐藏 Agent 不占 Tab）。
- 列表**平铺不折叠**；已卸载但有历史备份的 Agent 仍可切换查看/恢复。
- 条目中等密度卡片：左信息（类型/时间/备注/文件）、**右侧** [恢复] [删除]。
- **恢复**：二次确认 + **恢复前对当前 live 再备份一次**。
- 对象仅为各 Adapter 的 live 配置与凭据快照，不含会话日志/整库换机包。
- `autoBackup` 兼容字段已不展示开关；live 快照由核心服务在切换/导入/更新后自动创建。换机整库导出未实现（`Backend.features.backupExport=false`），无 UI 入口。

### 4.8 Settings

四个分区（侧栏英文 **Settings**；页内中文 **偏好 / 本机 / 备份 / 关于**，英文 **Preferences / This device / Backups / About**）：

1. **偏好**（`?tab=preferences`）：语言、主题、开机自启、关闭到托盘、技能市场源、用量采集间隔。
2. **本机**（`?tab=local`）：数据目录（只读 + 打开）、日志级别 / 保留天数、打开日志目录、本机路由回收链。
3. **备份**（`?tab=backups`）：各 Agent live 配置快照的查看 / 手动备份 / 恢复 / 删除，见 §4.7。
4. **关于**（`?tab=about`）：版本、检查/安装更新、GitHub 仓库、标语，以及原「安全」页的两条只读凭据说明（界面脱敏；存储不加密。**不**提供主密码 / keyring UI）。

Chat 会话设置（`ChatSettingsDialog`：cwd / 自动批准）不进 Settings。

**L1 SQLite 白名单**（`SETTINGS_WHITELIST`，与 CLI `config get/set` 共用）：`theme`、`language`、`log_level`、`log_retention_days`、`skill_market_source`、`close_to_tray`、`usage_collect_interval_min`。

- **保存模型**：无页级「保存」条。偏好控件变更即 `updateSettings`；主题/语言先预览再落盘，离开页面不回退。日志级别与保留天数同样立即写入；级别变更保留「需重启才完全生效」提示。关于页的检查/安装更新即时生效。
- **主题**：core 为权威。Settings 页 Select 预览并立即 `set_setting`。启动时 ThemeProvider 用 localStorage 做首屏缓存，再 `getSettings` 对账。
- **用量采集间隔**：在偏好页。已写入 SQLite，**不是**仅 localStorage。`None`=从未写入（前端默认 30）；`0`=仅手动；上限 1440。变更后 `notifyUsageSettingsChanged` 立即重排程（见 §4.6）。
- **开机自启**（`autoStart`）：OS 登录项（Windows 启动项 / macOS Login Item），不进 L1 白名单。
- **关闭到托盘**（`closeToTray`）：写 core，并同步 Tauri `AppState`。
- **语言**：core L1 为权威（`zh-CN` / `en`）。Settings Select 预览并立即 `set_setting`。启动时 `LanguageProvider` 用 localStorage 做首屏缓存，再 `getSettings` 对账；同步 `<html lang>`。**首次启动**（无语言缓存且尚未 seed）按 `navigator.languages` / `navigator.language` 选 zh/en，回落 zh，并一次性写入 core；已有用户选择不覆盖。不引入 i18next；字典在 `src/lib/i18n/locales/{zh,en}.ts`，第一期覆盖 Settings 四面板与侧栏 chrome。导航专有名（Chat / Agents / Skills / MCP / Projects / Dashboard / Connections / Routes / Settings）两种语言同值。业务页分期迁移。

Tab 与 URL `?tab=` 同步。规范 slug：`preferences` / `local` / `backups` / `about`（解析集中在 `src/pages/settings/settings-format.ts` 的 `SETTINGS_TABS` / `parseSettingsTab` / `resolveSettingsLocation`）。非法或缺省值 fallback 到 Preferences。切换使用 `replace`，避免污染浏览器历史。

旧 slug **replace 重定向**（不 404、不落空白面板）：

| 旧 `?tab=` | 去向 |
|---|---|
| `general` | Preferences |
| `security` | About（凭据说明现居于此） |
| `data` | Local（页顶：数据 / 日志） |
| `backups` | Backups（规范 tab） |
| `about` | About |

`/backups` → `/settings?tab=backups`。`/settings#backups` 与 `/settings?tab=local#backups` → `/settings?tab=backups`。

## 5. 组件清单

完整现行清单、决策树与禁止项见 [ui-component-standard.md](ui-component-standard.md)。下面只列产品契约仍要点名的复合件；**不要**把本表当实现目录。

| 组件 | 职责 |
|---|---|
| `AgentTabStrip` | 页内 agent 切换条，能力位置灰（如 Kimi 不支持账号切换/技能） |
| `ConnectFlowDialog` | 绑定对话框：Dashboard（工具固定，选一份登录）与 Connections「接到…」（登录固定，选工具）；目标语义 `bind`；预览标 ① 只改配置 / ② 写进对方认的登录 / ③ 本机转发；`plan.canApply` 只表示现在能写入 |
| `AgentDot` | Agent 品牌色圆点（侧栏/列表等轻量标识） |
| `StatusDot` | 四态认证状态（有效/临期/失效/未配置） |
| `SearchField` | 统一搜索输入（左图标 + `h-7`）。列表筛选禁止手写搜索框 |
| `SegmentedControl` | 页内列表筛选（Connections 登录类型等）；页级导航用 `Tabs` |
| `SecretInput` | 脱敏回显 + 眼睛切换明文。无二次确认、无自动再遮蔽 |
| `ConfigEditor` | CodeMirror 封装：JSON/TOML 高亮、敏感键自动脱敏层 |
| `QuotaBar` | 5h/7d 配额窗口进度条 + reset 倒计时 |
| `OAuthFlowDialog` | 三步授权：等待态含**复制授权链接** + **手动粘贴回调 URL**。实现在 `components/connect/` |
| `Notice` | 统一信息条；tone：`neutral` / `info` / `warning` / `danger` / `success` |
| `EmptyState` / `ErrorState` / `ListSkeleton` / `TableSkeleton` | 四态载体；见本文 §6 |
| `ListRow` | 管理列表行选中态（`bg-active` + 可选左边条） |
| `InlineTerminal` | 安装/升级 **环境或 Agent** 的流式输出面板 |
| `EnvStatusBar` | 页顶/Doctor 共享的 Runtime 状态条（ok/missing/outdated） |
| `EnvRemediationPanel` | 缺失 Runtime 的修复步骤：自动装 / 复制命令 / 打开官方页 / 重新检测 |
| `UsageParserHealth` | 用量解析健康：主用于 Dashboard（`dashboard`）；`compact` 仅兼容 re-export |
| `TableShell` | 全站管理表默认 Card 壳（含 Skills）。`workbench`/`flush` 业务侧基本不用 |

页面本地件（不进 shared）：Dashboard/Agents 的 Agent 卡、`SkillMatrix`、`UsageDetailsTable`。危险确认走各页 `Dialog` + `busy-confirmation`，**没有** `SwitchConfirmDialog`。

## 6. 状态覆盖清单

每页必须实现四态：**loading**（骨架屏，表格行/卡片轮廓）、**empty**（图标 + 一句话 + 主行动按钮）、**error**（错误摘要 + 重试 + 复制诊断信息）、**partial**（部分 agent 数据缺失时页面其余部分正常渲染，缺失区块单独置灰标注原因——多 Agent 能力不齐是常态，不能一错整页白屏）。

## 7. 关键交互流

1. **首次启动引导**（仅一次）：
   - Step A：**检测共享环境**（Node/npm 等）→ 缺失则展示「稍后安装 / 现在修复」（可跳过，不阻塞进入 App）。
   - Step B：检测已安装 agent → 扫描现有配置/凭据 → 询问「导入现有配置为供应商/账号？」→ 进入 Dashboard。
   - 未装任何 agent 且环境缺失时，Dashboard 空状态主 CTA 指向 **Agents 页环境修复**，而非直接「安装 Agent」。
2. **安装 Agent（含环境）**：选渠道 → 若 `env_missing` 则先 EnvRemediationPanel → 成功/重新检测通过 → 确认安装 Agent → InlineTerminal → detect 刷新卡片。
3. **切换供应商/账号**：选目标 → 确认对话框（backfill + 备份 + 进程警告）→ 执行 → toast 成功（含「撤销」按钮，5s 内回滚到刚写入的 auto-switch 备份）。
4. **OAuth 添加账号**：选平台 → 打开浏览器 → 等待回调（**复制授权链接** / **手动粘贴回调 URL** 降级）→ 展示账号信息 → 按当前账号存储方案入库 → 询问「立即切换？」。**已接线** Claude / Codex / Grok；其余 Agent 对话框展示 unsupported，引导导入/API Key。
5. **Token 统计首用**：Dashboard 用量段引导条 → 触发采集（后台任务 + 进度）→ 完成后模型下拉与图表有数据 → 之后以增量为主。
6. **技能同步**：矩阵点击 → 若冲突则对话框 → 复制/移除 → 刷新单元格；不修改真源除非用户显式「安装到真源」。
7. **恢复备份**：选条目 → 确认（将先备份当前）→ 写回 live → 提示可能需重启 Agent。
