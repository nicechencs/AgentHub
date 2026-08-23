# UI 组件与体验标准

> **定位**：组件用法标准。统一「用哪个件、什么层级、什么禁止」，减少方言，增强可预期性。  
> **不是**再写一版产品线框，也不是重写视觉 token / 对标执行。  
> **版本**：v1.0 · 2026-08-18（对照现行实现 + Fable 会审）

## 1. 定位与阅读顺序

| 文档 | 管什么 | 冲突时 |
|---|---|---|
| [ui-design.md](ui-design.md) | 布局、页面线框、业务交互、四态产品规则 | **业务规则**以它为准 |
| [ui-experience-alignment.md](ui-experience-alignment.md) | 对标 Cursor/Codex 的 token、表面分层、提示通道、分期改造 | **token / 视觉收敛**以它为准 |
| [chat-page-redesign.md](chat-page-redesign.md) / [bridges-page-redesign.md](bridges-page-redesign.md) | Chat / Routes 已落地的页面 IA | 本页特例以它们为准 |
| **本文** | 组件清单、决策树、信息通道、对照审计、分期落地 | 组件用法与检查表以本文为准 |

阅读顺序：先 `ui-design` 弄清页面在做什么 → 再看本文决定用哪个组件 → token 细节回 `tokens.ts` / 对标文档。

**范围外（不得据此派工）**：凭据落盘加密；国产 OAuth 开边 / 转 API。现有国产路由只认官方 API Key。

**技术栈（承认现状，不另起炉灶）**：React 18 + TS + Vite + Tailwind 3 + shadcn/Radix + lucide + CVA + CodeMirror 6 + recharts。未引入 TanStack Query / i18next / RHF / zod。GUI 语言走 `src/lib/i18n/`。禁止再引入第二套 UI 库。页面不得 `invoke`。

---

## 2. 设计原则

1. **浅色优先，克制**。靠明度分层，不靠多重边框和 accent 铺底。
2. **一页至多一个 accent 主 CTA**（`Button variant="default"`）。其余用 `secondary` / `outline` / `ghost`。
3. **四态必达**：loading / empty / error / partial。空态给下一步；**例外**：Routes 健康空态无按钮（空是常态）。
4. **提示分层**：控件释义走 `Hint`，截断/路径走 `Tip`，页内横幅走 `Notice`，短结果走 `Toast`，危险后果走 `Dialog`。禁止原生 `title` 当教学。
5. **不为空而空**。L3 教学不得默认占主列；L4 路径/ID 不得每行复读。
6. **危险先说清楚再执行**。确认用各页 `Dialog` + `busy-confirmation`，不再发明 `SwitchConfirmDialog`。
7. **凭据不明文常驻**。`SecretInput` 默认脱敏；眼睛切换明文。现行实现无二次确认、无自动再遮蔽——不要把未做的写成必须项。

---

## 3. Token 与焦点速查

真源：`src/styles/tokens.ts` → Vite 注入 CSS 变量。`tailwind.config.ts` 只映射 `var(--…)`。不要在本文复制大段色值。

| 约束 | 现行值 | 说明 |
|---|---|---|
| 字号 | 仅三档：`text-title` 16 / `text-body` 13 / `text-meta` 12 | `text-sm`/`text-xs` 等是同像素别名，**新代码写语义名** |
| 圆角 | `rounded-btn` 6 / `rounded-card` 8 / `rounded-composer` 12 / `rounded-mark` 产品标 | 禁止 `rounded-[Npx]`、`rounded-lg`、`rounded-2xl`；嵌套面板、Tooltip、代码井、应用壳列用 card |
| 阴影 | `shadow-xs` 卡 / `sm` 轻浮层 / `md` 菜单·Toast / `lg` Dialog | |
| 焦点环 | `focus:ring-2 focus:ring-accent/60` | Input / Button / Select 已对齐；**不要**再改成 1px 方言 |
| Accent | 浅 `#4F46E5`，深 `#6366F1` | Phase 0 已选：保留 indigo，只降暴露。换色相 = Phase 5，先不做 |
| 控件高 | 默认 `h-7` | `lg` 才 `h-8`。搜索框不要自创 `h-8` |

**已知文档/实现差（不在本阶段改色）**：

- 深色 `--bg-hover`：代码 `#1e1e22`，对标文档写 `#1C1C1F`。
- 深色 `--text-muted`：代码 `#8b8b96`；Phase 0 明确浅色 muted 不要弱到这个值。深色精修属 Phase 4–5。

**两套「L」不要混用**：

| 名称 | 出处 | 含义 |
|---|---|---|
| **信息分级 L0–L4** | 对标文档 §5.1 | L0 必需标签 · L1 行内状态 · L2 Hint 解释 · L3 教学 · L4 路径/调试 |
| **操作权重** | 本文 / `ui-design` 主 CTA | 每页 1 个 accent；次操作 ghost/secondary/outline；危险先 outline，确认后再 `danger` |

不要再发明第三套 L0–L4。

---

## 4. 现行组件清单

分层（已落地，必须承认）：

```
src/components/ui/        # 唯一基础件（shadcn/Radix）；仅此处可包 Radix
src/components/shared/    # 自研复合件
src/components/layout/    # 壳
src/components/connect/   # ConnectFlow / OAuth
```

页面本地件（`pages/*/…`）不是共享清单的一部分：`AgentCard`、`SkillMatrix`、`UsageDetailsTable`、`ParserHealthBar`（Dashboard 兼容 re-export）等。需要复用时先抽，再进 `shared/`。

### 4.1 `ui/` 基础件

| 组件 | 真实 API | 用法 |
|---|---|---|
| `Button` | `default` / `secondary` / `outline` / `ghost` / `danger` / `dangerOutline`；`default` `h-7` / `sm` / `lg` `h-8` / `icon` | `title` 自动转 `Hint`。作为 `DropdownMenuTrigger asChild` 时不要设 `title`，外层包 `Hint` |
| `Input` | `h-7`，`ring-2 ring-accent/60` | `title` → `Hint` |
| `Select` | Radix，Trigger `h-7` | 表单选择 |
| `Dialog` | 默认 `max-w-lg`、`rounded-card`、`p-6`、`shadow-lg` | Footer 右对齐；取消=`ghost`，主操作靠右。忙碌中走 `busy-confirmation` + `hideClose` |
| `DropdownMenu` | Radix | 添加菜单、行操作 |
| `ContextMenu` | **自研 portal，不是 Radix** | **仅** Skills 矩阵右键。Chat rail / 侧栏不用它 |
| `Tabs` | 与分段控件同灰轨 + 白底抬起 | 页级导航（Skills **两栏** library/market、Settings **四分区** 偏好 / 本机 / 备份 / 关于） |
| `Switch` | 选中 = accent | Settings 即时开关 |
| `Progress` | 细条 | 只经 `QuotaBar` 等复合件，页面少直接用 |
| `Toast` | `default` / `success` / `danger`；默认 5s | 标题 ≤16 字；可带撤销 |
| `Hint` / `Tip` | 默认 200ms（`main.tsx` TooltipProvider） | 见 §5.5 |
| `TableShell` | `default` = Card 壳（全站管理表，含 Skills）；`workbench` / `flush` **API 在、业务侧基本不用** | 业务表只选 variant，禁止手写 `*Workbench` class |
| `Card` | `default` 边框+`shadow-xs` / `plain` 无框 / `subtle` 弱底 | 嵌套用 `plain`/`subtle`，避免双重描边 |
| `Badge` | `default` / `accent` / `success` / `warning` / `danger` / `info` / `chip` / `chipActive` | 状态用无边框淡底；可点选项用 `chip` |
| `Skeleton` / `ListSkeleton` / `TableSkeleton` / `CardGridSkeleton` | | loading 用骨架，不用一句「正在加载…」。**没有** `ChatListSkeleton`。`CardGridSkeleton` 已导出、业务未用 |
| `segmented-styles.ts` | `segmentedTrackClass` / `segmentedItemClass` / `segmentedCountClass` / `actionCountClass` | 分段视觉真源 |

### 4.2 `shared/` 复合件

| 组件 | 职责 | 注意 |
|---|---|---|
| `EmptyState` | 图标 + 主句 + 可选行动 | 主句走 `text-title`。默认 `actionLabel` 按钮是 accent——仅当空态就是本页主行动时用；本页已有主 CTA 时传入 `action` 并降为 `secondary`/`outline` |
| `ErrorState` | 摘要 + 重试 + 复制诊断 | 分区内嵌用 `compact`（Dashboard 用量） |
| `Notice` | 页内横幅；`neutral` / `info` / `warning` / `danger` / `success` | 一屏建议最多一条 |
| `SearchField` | `h-7` + 左图标 | **禁止**手写 `relative + Search + pl-7/pl-8` |
| `SecretInput` | 脱敏回显 + 眼睛切换 | 无二次确认、无 10s 再遮蔽；聚焦已遮蔽值会清空以便重输 |
| `ListRow` | 管理列表行；`active` = `bg-active` + 可选左边条 | 仍带 border + `rounded-card`，偏管理后台。工作台会话行不要套它 |
| `SegmentedControl` | 页内筛选，默认 `sm` + `count` | 不要用来切页面 |
| `QuotaBar` | 5h/7d 配额 | |
| `StatusDot` | 认证四态 | |
| `StatusPin` | 更小的语义点（更新/生效/缺失） | 不要手写 `h-1.5 w-1.5 rounded-full bg-*` |
| `AgentDot` / `AgentLogo` / `AppLogo` | 品牌标识 | `AgentDot.title` → `Hint`；不要用 Agent 色铺大面 |
| `CurrentBadge` | 「当前」 | |
| `DetailRow` | 详情网格 label/value | |
| `ConfigEditor` | CodeMirror；敏感键脱敏层 | |
| `GenericConfigForm` | 通用字段表 | |
| `InlineTerminal` | 安装/升级流式输出 | |
| `EnvStatusBar` / `EnvRemediationPanel` | Runtime 状态与修复 | 环境条「一键修复」用 `secondary`；面板内「一键安装」仍是 accent，本页若已有主 CTA 应降级 |
| `MarkdownView` | Skills 预览等 | |
| `UsageParserHealth` | Dashboard 主用 `dashboard` | `UsageHealthStrip` / `ParserHealthBar` 是兼容 re-export，新代码不要用 |
| `OnboardingDialog` / `BootSplash` | 首次引导 / 启动 | |
| `ThemeProvider` / `LanguageProvider` | 主题 / 文案字典 | |
| `NotificationBell` / `UpdatePrompt` / `UsageSyncProvider` | 壳级服务 | |
| `busy-confirmation` | 忙碌中禁止关确认框 | 危险 Dialog 必用 |

`SwitchConfirmDialog`：**已删除**。`ui-design.md` 旧 §5 与 `cli-and-config.md` 的「同语义」表述不得再当成 GUI 组件名。

`OAuthFlowDialog`：实现在 `components/connect/`；`shared/OAuthFlowDialog.ts` 只再导出 token helper。

### 4.3 `layout/`

| 组件 | 职责 |
|---|---|
| `Sidebar` | Workspace：Chat / Agents / Skills / MCP / Projects。Manage：Dashboard / Connections / Routes（英文 Routes、中文「路由」，**永久显示**）/ Settings。备份在 Settings `?tab=backups`，不占侧栏 |
| `TopBar` | Chat 不渲染 |
| `PageHeader` | `default` / `compact`（全高页只收底距）。标题一律 `text-title` |
| `PageSection` | 段距 / 可选分割线 / 段标题（body + semibold） |
| `AgentTabStrip` | 页内 Agent 过滤，固定 md；普通数字走 `counts` |
| `page-rhythm.ts` | 边缘与区块节奏真源 |

### 4.4 `connect/`

| 组件 | 职责 |
|---|---|
| `ConnectFlowDialog` | 绑定。Dashboard 点工具 = 固定选登录；Connections「分享 / 路由」= 登录固定、按目的过滤工具。确认走 `bind` |
| `OAuthFlowDialog` | 仅已开边 OAuth（Claude / Codex / Gemini / Antigravity 等）。国产 OAuth **不开边** |

---

## 5. 组件决策树

### 5.1 按钮

| 页面角色 | variant | 例 |
|---|---|---|
| 该页唯一主行动 | `default`（accent） | Connections「添加」、**本页没有其他主 CTA 时的**空态按钮 |
| 卡内安装 / 次要提交 | `secondary` | Agents 卡「安装」 |
| 需要边框的次要 | `outline` | ErrorState「复制诊断」、Notice 行动 |
| 工具条、取消、图标 | `ghost` | Dialog 取消、行内「详情」 |
| 确认框里的破坏 | `danger` | 卸载、停止路由、删除会话 |
| 未确认前的破坏入口 | `dangerOutline` 或 `ghost` | 行内「移入回收站」 |

尺寸：默认 `h-7`。不要为了「更显眼」把搜索框做成 `h-8`。

### 5.2 表面：Card / 表 / ListRow

| 内容 | 用 | 不用 |
|---|---|---|
| 独立内容块（设置分区、指标、登录列表行外壳） | `Card default` 或 `ListRow` | 再外包一层 Card |
| 已在框内的工具条 / 嵌套 | `Card plain` / `subtle` | 再加 border |
| 管理表（用量、Skills 库/市场） | `TableShell default` | 手写 table class；不要为了对标 IDE 把 Skills 改成 `flush`（本阶段不做） |
| 工作台会话列表（Chat rail） | 页面自管行 + `bg-active` | `ListRow`（带卡边，会把 rail 做成后台） |
| 表格预览行 | `TableRow active` | 整行铺 accent |

`ListRow` 去边框、改成 IDE 树行：先不做。

### 5.3 Tabs vs Segmented vs AgentTabStrip

见 `segmented-styles.ts` 注释，这里是硬规则：

| 场景 | 组件 | 尺寸 |
|---|---|---|
| 页级导航（Skills 两栏、Settings 四分区：偏好 / 本机 / 备份 / 关于） | `Tabs` | md |
| 页内列表筛选（全部 / OAuth / API Key…） | `SegmentedControl` | sm + `count`。仍是需要类型芯片的页面的通用件 |
| 页内 Agent 过滤 | `AgentTabStrip` | md（不要再传 sm）。Connections 走此件，**不再**用 `SegmentedControl` 做「官方登录 / API Key / 未识别」 |
| 预览「预览 \| 源码」 | 允许手写扁段，`h-6` | 特例 |

普通数量用 `segmentedCountClass`。琥珀行动角标用 `actionCountClass`，不要再画一遍明文数字。凭据类型在 Connections 用行内 OAuth 人头 / API Key 钥匙，不另开类型芯片。

### 5.4 搜索

列表筛选 **必须** `SearchField`。现行已收口：Skills、Chat、Projects。**Connections 登录列表无搜索框。**

禁止再写 `relative + Search icon + Input`（旧方言：`h-8 w-44 pl-7 text-xs`）。

### 5.5 提示通道

| 要说的事 | 通道 | 长度 | 不要 |
|---|---|---|---|
| 图标按钮是什么 | `Button title` → `Hint` | ≤20 字 | 原生 `title` |
| 截断文本 / 路径全文 | `Tip` | 完整串或「名 · 一句」 | 行级 native title |
| 禁用原因 | `Hint` 包一层（disabled 子节点也能悬停） | 一句 | 把原因写进主列 |
| 页内需要行动的状态 | `Notice` | 一屏一条 | 多条堆叠 |
| 操作结果 | `Toast` | 标题 ≤16 字 | 把路径堆进标题 |
| 危险后果 / 不可逆 | `Dialog` + `busy-confirmation` | 结构化 | 只靠 tip |

测试锁：`src/components/ui/title-channel.test.ts` 禁止 `pages` / `layout` / `shared` 在 `p/span/div/button` 等节点写原生 `title`。`Button` / `Input` / `AgentDot` 的 `title` 不在扫描内。`components/ui/` 自己的裸 `<button title>` 也不准（Toast 复制钮已收）。

`Hint` 与 `Tip` **同一 200ms**（`main.tsx` TooltipProvider）。`Tip` 只是给非控件节点包一层 span，不是更长延迟。

### 5.6 四态

| 态 | 组件 | 规则 |
|---|---|---|
| loading | `ListSkeleton` / `TableSkeleton` / `CardGridSkeleton` / `Skeleton` | 与终态同密度，避免「一行字 → 卡片」跳动 |
| empty | `EmptyState` | 有明确下一步则给按钮；筛选无结果给「显示全部」 |
| error | `ErrorState` | 分区失败用 `compact`，不要白掉整页（Dashboard 用量） |
| partial | 缺块单独标注 | 多 Agent 能力不齐是常态 |

**产品例外**：Routes 健康空态无按钮（[bridges-page-redesign.md](bridges-page-redesign.md)）。

### 5.7 凭据输入

- 密钥 / token：`SecretInput`
- 普通文本：`Input`
- 不要把「10s 自动再遮蔽」写成验收项

### 5.8 危险确认

各页自己的 `Dialog`。忙碌中：

- `closeConfirmationOnOpenChange(open, busy, onClose)`
- `preventBusyConfirmationDismissal(busy, event)`
- 必要时 `DialogContent hideClose`

不要新建全局 `SwitchConfirmDialog`。三要素（backfill / 备份路径 / 进程警告）是**文案内容**，不是组件名。

---

## 6. 页面壳与节奏

内容宽度只分两套（产品契约 [ui-design.md §3.1](ui-design.md)）。新页先选套，再写布局。

| 套 | 何时用 | 必须引用 |
|---|---|---|
| **阅读列** | 对话 / 设置表单 / 长文 | `pageRhythm.readingColumn`（`mx-auto w-full max-w-3xl`）。Chat 与 Settings 必须同一条，禁止各写各的 max 宽。 |
| **贴边列**（默认） | 列表 / 表格 / 卡片墙 / 主从分栏 | 常规页走 `pageRhythm.pageShell`（`px-[18px]`）；Skills / Projects 走 `workbenchX` / `pageEdgePx.x`。不要再套 `mx-auto max-w-*`。 |

禁止第三套：不要恢复 `max-w-content`（1200），不要页面私有 `max-w-4xl` / `max-w-5xl`，不要阅读列左对齐。`fullBleed` 只管全高，不是第三套宽度。

`App.tsx`：

| 路由 | TopBar | 宽度套 | 外壳 |
|---|---|---|---|
| 常规页 | 有 | 贴边列 | `pageRhythm.pageShell` = `w-full min-w-0 px-[18px] py-[18px]` |
| `/chat` | 无 | 阅读列 | fullBleed，无 `PageHeader`；消息列 `pageRhythm.readingColumn` |
| `/skills` | 有 | 贴边列 | fullBleed；`workbenchHeader` + `PageHeader size="compact"` + Tabs |
| `/projects` | 有 | 贴边列 | fullBleed；`workbenchHeader` + `PageHeader size="compact"` + 左侧列表 / 右侧会话预览 |
| `/settings` | 有 | 阅读列 | 常规壳；页头贴边，表单 `pageRhythm.readingColumn` |

区块自上而下：`PageHeader` → `chrome` / `chromeRow` → `lead`（环境条、Notice）→ `stack` / `blocks` → `PageSection`。Chat / Skills / Projects 自管布局，但水平 inset 须走 `pageRhythm` / `pageEdgePx`，禁止硬编码 `px-4`（Chat chrome 除外，它的真源是 `chatChromeX`）。

导航专有名词保持英文（Dashboard / Agents / …）；页面标题与正文用中文。

---

## 7. 对照审计

### 7.1 文档偏差（已按现行实现纠正）

| 旧描述 | 现行 | 处理 |
|---|---|---|
| `SwitchConfirmDialog` 仍在 `ui-design` §5 | 代码已删（`modularity-improvement.md`） | 本文 + `ui-design` §5 删除该行 |
| `SecretInput`「二次确认 + 10s 再遮蔽」 | 眼睛切换；无自动遮蔽 | 产品原则改为按实现写 |
| 输入焦点「应是 ring-1」等口头差 | 实现与 `ui-design` 均为 `ring-2 / accent/60` | 以代码为准。会审稿若写 `ring-1` / Dialog `max-w-md` / Badge `outline` / `ChatListSkeleton`，一律以本表和源码为准 |
| Dialog「max-w-md / rounded-composer」 | 默认 `max-w-lg` / `rounded-card` | 以 `dialog.tsx` 为准 |
| `AgentCard` 列为 shared；Skills「三 Tab」 | Agent 卡在 `pages/agents` / `AgentOverview`；Skills 只有 library + market | 清单按实现写 |
| 对标 Phase 3「抽 ListRow / compact Header / 分段同一族」 | 这三项已落地 | Phase 3 剩余改为方言收口，见 §8 |
| Dashboard 快捷「切换供应商 / 切换账号」 | 两钮同一 `openForAgentConnect` | 已收成一枚「连接 / 切换」 |

### 7.2 实现方言（本 PR 已收一部分）

| 项 | 原状 | 本标准 |
|---|---|---|
| Connections 登录列表搜索 | 手写 `h-8 w-44 pl-7 text-xs`，后改 `SearchField` | **现行无搜索框** |
| 登录列表 loading | 「正在加载钱包…」（历史文案） | 已改为 `ListSkeleton` / 登录列表 loading |
| `EmptyState` / `ErrorState` / `Notice` / `QuotaBar` / `StatusDot` | `text-sm` / `text-xs` | 已改语义名；空态/错误主句升为 `text-title` |
| 全站仍大量 `text-sm` / `text-xs` | 别名，像素相同 | 新代码写语义名；存量不搞大扫除 |

### 7.3 仍存在的体验债（承认，不假装本周清完）

| 债 | 位置 | 本阶段 |
|---|---|---|
| 列表名仍写 `text-sm font-medium` | 多页 | 新代码改 `text-body`；不扫全站 |
| `ListRow` 带卡边，和 Chat 会话行是两套选中态 | `ListRow` vs Chat rail | 先保持；去边框重做 = 后做 |
| Card 密度不齐 | Dashboard 工具卡 / Agents 卡 / Settings | 新卡跟 `p-3` / Header `px-4`；不重做旧卡 |
| Accent 面积 | EmptyState 默认 accent；Skills 页头「安装」反而是 ghost；`EnvRemediationPanel`「一键安装」accent | 新空态看本页是否已有主 CTA；Skills 安装 / 环境面板降级随后续 PR |
| 选中态方言 | `ListRow` 带框；Chat rail / Sidebar 无框 `bg-active`；Agents 渠道 chip 铺 `accent/10` | 工作台无框、管理列表有框——先承认两套，不硬收 |
| 无 Checkbox / Textarea 基础件 | Skills / Projects / Provider 手写 `<input type="checkbox">`；Chat composer 无 ring | **先不抽**新 ui 件；新 checkbox 用 `accent-accent`，禁止 `--color-accent` / `sky.500` |
| 旧 class | 回收站曾用 `text-muted-foreground`、`rounded-lg` | 本 PR 已改 token class |
| `TableShell workbench/flush` 未用于业务 | Skills 仍是 default Card 壳 | **先不做** 大改表壳 |
| 动效 / 深色 hover·muted / accent 换色相 | 对标 Phase 4–5 | **先不做** |
| Backups「无可管理」空态无下一步 | `BackupsPanel` | 后做：补「去 Agents」 |
| Dashboard 骨架按全部 `AGENTS` 出卡 | `dashboard/index.tsx` | 后做：按已装数出骨架 |

---

## 8. 分期落地

### 当前（Phase 3 收口，可拆 PR）

1. **文档**：本文 + 回写 `ui-design.md` §1.3 / §5 + `docs/README.md` + 对标文档相关链接。  
2. **SearchField 收口** Skills / Chat / Projects（禁止手写搜索框）。Connections 登录列表**无搜索框**。  
3. **共享件字号**改语义 token；空态/错误主句用 `text-title`（本 PR 已做）。  
4. **登录列表 loading** 改 `ListSkeleton`（本 PR 已做）。  
5. **提示通道决策**以本文 §5.5 为准；后续 PR 按检查表自检。

### 先不做

- 动效、深色精修、accent 换色相（Phase 4–5）
- `TableShell` 改 workbench / 去掉 Skills Card 壳
- `ListRow` 去边框改成 IDE 树
- 全站 `text-sm` → `text-body` 机械替换
- 引入第二套 UI 库、i18next、RHF、zod；现抽 `Checkbox`/`Textarea`/`Popover`
- 凭据落盘加密
- 国产 OAuth 开边 / 转 API
- 恢复 `SwitchConfirmDialog`
- 给 `SecretInput` 加 10s 自动遮蔽（除非产品重新授权）
- 把 Chat rail / Sidebar 强行改成 `ListRow`

---

## 9. 禁止项（PR 检查表）

- [ ] 没有新增 UI 库或平行组件目录
- [ ] 页面没有直接 `invoke`
- [ ] 没有手写搜索框（必须 `SearchField`）
- [ ] 没有新增字号档或 `text-[Npx]` / `rounded-[Npx]`
- [ ] 新代码字号用 `text-title` / `text-body` / `text-meta`
- [ ] 一页没有两枚并排 `variant="default"`
- [ ] 没有在 `pages` / `layout` / `shared` / `ui` 的裸 button 上写原生 `title`
- [ ] 没有 `text-muted-foreground` 或 `accent-[var(--color-accent…)]`
- [ ] loading 不是一句「正在加载…」（用对应 Skeleton）
- [ ] 危险确认用 Dialog + `busy-confirmation`，没有新的全局确认件
- [ ] 没有把凭据加密或国产 OAuth 写成待办
- [ ] Chat / Skills 水平 inset 走 `pageRhythm`，没有随手 `px-4`（Chat chrome 除外）
- [ ] Agent 品牌色只用于点与图表，不铺按钮/大面

---

## 10. 与对标文档 Phase 3 的关系

对标文档 Phase 3 原列表里，下列**已经落地**，不必再派工：

- 选中态抽 `ListRow`
- Segmented / Tabs / AgentTabStrip 同一视觉族
- `PageHeader size="compact"`

对标文档仍标「计划」的剩余项，以**本文 §8** 为准：先收方言与文档，不动表壳/动效/换色。
