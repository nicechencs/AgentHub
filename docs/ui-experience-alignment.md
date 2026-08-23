# UI 风格与体验对标优化方案

> **定位**：对标 **Cursor** / **Codex**（及同类 IDE 工作台）的**视觉层级与交互质感**，不是抄皮肤、不是改产品信息架构。  
> **范围**：颜色层级、边框、字号/字体、表面分层、预览面板、提示体系、辅助信息；优先落在 **Skills** 与**全站 token/组件**，再推广到 Connections / Agents / Chat。  
> **关系**：本文件是体验与视觉执行方案；页面线框与业务交互仍以 [ui-design.md](ui-design.md) 为产品契约。冲突时：**业务规则以 ui-design 为准，视觉执行以本文件收敛 token 与组件为准**。  
> **版本**：v1.1 · 2026-08-06（实机核验版）  
> **落地状态（2026-08）**：**Phase 0–2 大体已落地**（token / Skills 静音与预览一体 / 文案包）。Phase 3 里 ListRow / 分段同一族 / `PageHeader compact` 已落地；剩余方言收口与组件决策见 [ui-component-standard.md](ui-component-standard.md)。Phase 4–5（动效、深色精修、accent 换色）仍是 backlog。  
> **截图**：仓库不存实机图，以 `pnpm dev:mock` 为准。当时核验条件见 §12.2 文字结论，不链本地 png。  
> **现行状态**：本文是视觉/体验对标，**不是**现行产品 IA。现行 Settings **四栏**（偏好 / 本机 / 备份 / 关于）、侧栏 **Routes / 本机路由**、picker 芯片 **直连 / 用这份登录 / 本机路由 / 当前不支持** 见 [ui-design.md](ui-design.md)。下文 Phase 写作为当时体验改造记录，勿当现行 IA。

---

## 0. 审查结论与已拍板决策

**结论（当时核验）**：实机对比支持原方案的主方向，但修正两点轻重：AgentHub 的全局框架与侧栏并不差，当时最大差距集中在 **Skills 内容面过度表格化** 与 **预览焦点叙事不完整**；indigo 色相本身不是 Phase 0 的首要问题。Phase 0–2 现已大体落地，不必再按「批准进入 Phase 0–1」排期。

已拍板：

1. **预览打开方式**：单击 skill 名称打开或切换预览；`Enter` 等价。矩阵格点击仍只执行启用/停用，不顺带切预览。
2. **Accent Phase 0**：采用方案 B——保留现有 indigo token，只降低暴露；一页最多一个 accent 主 CTA，其余降为 secondary / ghost。Phase 0 不换色相。
3. **预览跨筛选行为**：采用 B——筛选后即使目标行不在可见列表中，右侧预览仍保留；此时允许左侧没有 active 行，以预览 header 的名称与来源为准。

未拍板且不阻塞 Phase 0–1：是否在 Phase 5 微调 accent 色相。必须等 Phase 1 截图复核后再决定，不预埋主题设置。

---

## 1. 目标与非目标

### 1.1 目标

| 目标 | 含义 |
|------|------|
| **风格对齐** | 浅色优先、低对比边框、明度分层、13px 级桌面密度，体感接近 Cursor 工作台 / Codex 紧凑工具面 |
| **层级可读** | 一眼分清 canvas / panel / elevated / overlay；主文 / 次文 / 弱文 / 禁文 |
| **提示克制** | 默认界面安静；说明进 Hint / 状态栏 / 首次空状态，而不是主列长文 |
| **预览一体** | 列表 active + 右侧文档预览是同一工作流，而不是「表 + 弹窗/旁注」 |
| **可执行** | Token → 组件 → Skills 试点 → 全站推广，可分期落地 |

### 1.2 非目标

- 不复制 Cursor 橙 / Codex 绿作为**应用主色**（Agent 品牌色仅用于 agent 点与图表）。
- 不取消 Skills 的多 agent 矩阵能力（这是 AgentHub 差异化，不是对齐对象）。
- 不做大面积插画 / 营销风；不做重动效。
- 不引入第二套 UI 库；继续 Radix + Tailwind + 现有 `components/ui`。

### 1.3 成功标准（可感知）

1. **静音主界面**：Skills 默认首屏无超过一行的说明段落；图例默认折叠。  
2. **焦点可辨**：预览打开时，左侧对应行有明确 active（与 checkbox 多选分离）。  
3. **提示通道统一**：业务悬停全部走 `Hint`/`Tip`/`Button title→Hint`；禁止原生黄框 `title` 当教学文案。  
4. **Token 单真源**：字号 / 边框 / 表面色无「同义不同值」散落 class。  
5. **对照自评**：在浅色主题下，并排 Cursor 设置页 / 文件树 + AgentHub Skills，边框与灰阶不「更脏、更紫、更跳」。

---

## 2. 对标对象：各自强在哪

| | **Cursor** | **Codex（IDE/工作台式交互）** | **AgentHub 现状倾向** |
|--|------------|-------------------------------|------------------------|
| 气质 | 编辑器工作台：选中即工作 | 流程紧凑、少装饰、状态清楚 | 全局框架已接近桌面工具，**Skills 内容面仍像管理表格 + 说明书** |
| 颜色 | 近中性灰阶，accent 克制 | 中性 + 少量功能色 | 中性底色合格；**accent 曝光需要收敛，色相暂不构成主矛盾** |
| 边框 | 极弱分隔，靠底色差 | 细线、少双重描边 | **卡片/表格/Tab 多重 border 叠罗汉** |
| 字号 | 12–13 列表，UI 标签更小 | 偏紧 | 已收成 title 16 / body 13 / meta 12 |
| 辅助信息 | Tooltip / 状态栏 / 命令面板 | 短状态、少说教 | **主列长文 + 原生 title + Tip 混用** |
| 预览/详情 | 侧栏/编辑区与列表强绑定 | 输出区与输入一体 | 侧栏已有，**active 绑定与加载质感不足** |

**对齐策略一句话**：借 Cursor 的**表面分层与焦点叙事**，借 Codex 的**信息克制与密度**；保留 AgentHub 的**多 agent 管理语义**。

---

## 3. 现状深度审计（对照代码真源）

### 3.1 颜色与表面（`src/styles/tokens.ts` → CSS 变量）

| Token | 浅色现值 | 问题 |
|-------|----------|------|
| `--bg-canvas` | `#F7F7F8` | 合理，接近 Cursor 画布 |
| `--bg-panel` | `#FFFFFF` | 合理 |
| `--bg-subtle` / `--bg-hover` | `#F1F1F3` / `#EBEBED` | 与 border 接近，**hover/选中有时不够「进一层」** |
| `--border` / `--border-strong` | `#E6E6E9` / `#D6D6DA` | 单独可用；**到处加 border 后整体变脏** |
| `--text-primary` | `#18181B` | 可 |
| `--text-secondary` / `--muted` | `#55555D` / `#70707A` | 对比度可用，Phase 0 **不要把 muted 弱化到 `#8B8B96`**；层级问题主要来自角色混用与元信息过多 |
| `--accent` | `#4F46E5` indigo | Skills 实机中只零散出现在复选框、链接和分隔条；问题是**使用面积与语义**，不是 Phase 0 必须换色相 |

**深层问题**：文档写了「靠明度分层，不靠边框堆叠」，实现仍是 **panel + border + card + table header bg-subtle** 叠加，分层靠线不靠面。实机确认应先减框、减元信息，再讨论换 accent 色相。

### 3.2 字体与字号（`src/styles/tokens.ts` `TYPE_SCALE`）

| 标准 | class | 像素 / 行高 | 用途 |
|------|-------|-------------|------|
| title | `text-title` | 16 / 1.35 | 页标题、空态主句、指标数字 |
| body | `text-body` | 13 / 1.45 | 正文、按钮、列表名、段标题（加字重） |
| meta | `text-meta` | 12 / 1.4 | 次级说明、表头、路径、角标、眉题 |

旧名 `text-lg`/`text-xl`、`text-sm`/`text-base`、`text-xs`/`text-2xs` 是同像素别名，不是第四档。`body` 元素跟 body 档走。

**历史问题（已收敛）**：曾并行 11/12/13/14/16/20 六档；现只保留上表三档。

### 3.3 边框与圆角

| 元素 | 常见做法 | 观感 |
|------|----------|------|
| Card / TableShell | `border border-border` + `shadow-xs` | 管理后台 |
| 矩阵图例 | 再套一层 border 卡 | 说明区比数据区还显眼 |
| Tabs / Segmented | 各自容器 | 控件「方言」 |
| 分栏 separator | `w-1.5 bg-border` | 功能有，**视觉权重可再 IDE 化** |
| 圆角 | 6/8/12 | 略大于 Cursor 部分控件的「更方一点」 |

### 3.4 提示与辅助信息（Skills 重灾区）

| 通道 | 约定（tooltip.tsx） | 实际 |
|------|---------------------|------|
| `Hint` / `Tip` | 悬停唯一真源 | 部分遵守 |
| `Button title` | 自动转 Hint | 好 |
| 原生 `title=` | **禁止当提示** | Skills 仍大量：`title={rowHint}`、`title={\`${n} 个…\`}`、拖拽条 title 等 → **黄框 + 与 Tip 双通道** |
| 主列说明 | 应克制 | 「这里是本地共享库…单向投影…」整段常驻 |
| PageHeader `descriptionTip` | 悬停展开长说明 | 合理，但 description 本身仍可更短 |
| 图例 | 应折叠 | `SkillMatrixLegend` 默认展开，占纵向空间 |
| Toast | 标题 + 描述 | 部分文案过技术（路径、术语堆叠） |

**文案问题类型**：

1. **教学型**：解释产品模型（单向投影、共享库路径）→ 应进首次/帮助，不应每行 tip。  
2. **状态型**：已在共享库、冲突 → 应短（「已在库中」），细节进侧栏或确认框。  
3. **操作型**：现状「双击预览 · 右键菜单」→ 目标改为名称单击/Enter；首次提示一次即可，不要每行 title 复读。
4. **系统型**：`~/.agents/skills` → 专家向；默认可弱化，需要时在预览 header / 打开目录。

### 3.5 预览体验（`SkillMarkdownPreviewPanel` + Skills 布局）

**已有优点**：

- 右侧面板替代 Dialog，方向正确。  
- 全宽全高（App fullBleed）、可拖拽宽度 + localStorage。  
- Markdown / 源码切换、Esc 关闭、打开目录。

**仍弱于 Cursor/Codex 的点**：

| 缺口 | 表现 |
|------|------|
| 无列表 **activeId** | 预览与勾选 selected 混谈；看不出「正在预览谁」 |
| 加载闪白 | 换 skill 时空内容 + spinner，无「保留上篇 + 顶部细条」 |
| 预览 chrome 偏重 | 双行 header + 工具条，文档区被挤 |
| 路径展示 | mono 截断在 header，与 Cursor 面包屑/tab 名的简洁度比偏吵 |
| 无键盘闭环 | 无 ↑↓ 切换 skill、无 Enter 打开 |
| 分隔条 | 可拖但缺「拖动中全局态 / 双击重置」等 IDE 细节 |
| 表面 | 左右都偏同一 panel 感，**面差不足** |

### 3.6 控件方言（同页多套语言）

Skills 单页可同时出现：`PageHeader`、Tabs、SegmentedControl、AgentTabStrip、Table、图例 Card、ContextMenu、Toast、Dialog。  
对齐对象通常 **同一密度、同一边框哲学、同一选中态**。AgentHub 每套控件各自合理，合在一起像拼盘。

### 3.7 实机对比后的诊断校正

| 可见事实 | 判断 | 对方案的影响 |
|----------|------|----------------|
| AgentHub 与 Cursor 侧栏都使用安静灰底和弱选中面 | **全局框架不是主要问题** | Phase 0 不重做导航，不引入品牌色选中条 |
| AgentHub Skills 行同时展示名称、描述、绝对路径，并叠加 Card 外框与行分隔 | 首屏噪声主要来自**三行元信息 + 表格壳** | Phase 1 固定「名称 + 最多一行描述」，路径移到预览/footer；新增 flush/workbench 表壳变体 |
| 打开预览后，左侧没有可辨 active 行 | 预览与列表缺同一焦点叙事 | `activeKey` 成为 Phase 1 验收阻断项，不得只复用 checkbox selected |
| 预览 header 两层、路径常驻；Markdown 首屏标题和首段明显过大过重 | 右栏 chrome 与文档正文同时抢主视线 | `SkillMarkdownPreviewPanel` 与 `MarkdownView(document)` 必须一起收敛，不能只改容器 |
| Cursor Settings 主要靠宽松面差和组块底色分层，极少使用外包边框 | 原「边框堆叠」诊断成立 | `TableShell` 已有 workbench/flush 变体；**Skills 矩阵仍用默认 Card 壳，flush 未用** |
| ChatGPT/Codex 空白工作台大面积留白，只在任务卡与 composer 上给极弱轮廓 | Codex 的「克制」来自少量高价值表面，不是单纯缩小字号 | AgentHub 不照抄大留白；只借其辅助信息降噪和焦点单一性 |

**误判修正**：不再把 `#4F46E5` 视为「冲突最强的一点」，也不建议 Phase 0 把 `--text-muted` 改成更浅的 `#8B8B96`。这两项都可能掩盖真正的结构噪声，且后者会降低小字可读性。

---

## 4. 目标视觉系统（执行规格）

### 4.1 表面层级（Surface）— 优先于边框

定义 **5 层**，组件只通过 token 取色，禁止随意 `bg-white` / 硬编码灰。Phase 0 使用下列**确定值**，不以区间作为实现验收：

| 层级 | Token | 浅色 | 深色 | 用途 |
|------|-------|------|------|------|
| 0 Canvas | `--bg-canvas` | `#F7F7F8` | `#0A0A0B` | 主区底、Skills 左栏底 |
| 1 Panel | `--bg-panel` | `#FFFFFF` | `#121214` | 右预览、侧栏、浮层底 |
| 2 Subtle | `--bg-subtle` | `#F1F1F3` | `#1A1A1D` | 表头、工具条条带 |
| 3 Hover | `--bg-hover` | `#EBEBED` | `#1C1C1F` | 行 hover |
| 4 Active | `--bg-active` | `#E4E4E7` | `#2c2c31` | **当前预览行 / 当前连接** |
| Overlay | panel + shadow | 沿用 Panel | 沿用 Panel | Dialog / Menu / Toast |

**规则**：

- 同层相邻区域 **优先换底色，不双加 border**。  
- 仅在「可拖拽边界 / 浮层边缘」使用可见分隔；列表内用 `border-t` 极淡或只靠 hover 底。  
- Skills：**左 canvas，右 panel**；中间 separator 默认用 `--border`，仅 hover/focus 可短暂出现 accent，不给 active 行铺 accent 淡色。

### 4.2 边框

| 用途 | 规格 |
|------|------|
| 默认分隔 | `1px solid var(--border)`，透明度可 80% 于表行 |
| 强分隔（少用） | `--border-strong`，仅输入框 focus 外框、危险区 |
| 禁止 | Card 外包 Table 再外包 border 的三层框（TableShell 审视） |
| 圆角 | 控件 **5–6px**；卡片 **8px**；大面板 **0–6px**（全高侧栏建议 **无大圆角**，贴边更 IDE） |

### 4.3 文字色阶（四阶 + 禁用）

| 角色 | Token | 浅色 / 深色 | 用途 | 字重 |
|------|-------|-------------|------|------|
| Primary | `--text-primary` | `#18181B` / `#FAFAFA` | 名称、页标题、正文 | 500–600 标题 / 400 正文 |
| Secondary | `--text-secondary` | `#55555D` / `#A1A1AA` | 描述、次要标签 | 400 |
| Muted | `--text-muted` | `#70707A` / `#71717A` | 路径、时间、占位、表头 | 400 |
| Disabled | **新增** `--text-disabled` | `#A1A1AA` / `#52525B` | 不可用操作；不得承载必读信息 | 400 |
| Accent text | `--accent` | 保持现值 | 链接、可点文字按钮 | 500；一屏少量 |

Phase 0 保持现有 primary/secondary/muted 数值，只补 `--text-disabled`。小字号下不得用 `#8B8B96` 替代当前 muted。

**禁止**：主列表名称用 secondary；说明用 primary。

### 4.4 字号角色表（强制映射）

真源：`TYPE_SCALE`（`src/styles/tokens.ts`）。**只许三档**，禁止再加 11 / 14 / 20 或任意 `text-[Npx]`。

| 角色 | class | 字号 | 行高 | 使用场景 |
|------|-------|------|------|----------|
| Title | `text-title` | 16px | 1.35 | `PageHeader` h1、空态主句、Dashboard 指标、Markdown 文档 H1 |
| Body | `text-body` | 13px | 1.45 | 正文、按钮、列表名、`CardTitle` / `PageSection` / Dialog 标题（靠字重区分）、菜单项 |
| Meta | `text-meta` | 12px | 1.4 | 表头、路径、角标、眉题、Toast 描述、Tooltip、代码/mono |

**别名**（同像素，便于存量 class）：`text-lg`/`text-xl` → title；`text-sm`/`text-base` → body；`text-xs`/`text-2xs` → meta。新代码写语义名。

**字重，不是第四档**：段标题 / 卡标题用 body + `font-medium`/`semibold`，不要为了「看起来像标题」再开 14px。

### 4.5 字体

| 用途 | 建议 |
|------|------|
| UI | 保持 system-ui 栈（与 Cursor 桌面一致，加载快） |
| Mono | JetBrains Mono / Consolas；**仅路径、ID、源码、版本号** |
| 中文 | 不额外嵌中文字体文件（体积）；依赖系统 YaHei / PingFang |

可选后期：Inter 仅英文 UI（非必须）。

### 4.6 Accent 策略（对齐 IDE 气质的关键）

| 方案 | 做法 | 建议 |
|------|------|------|
| **A. 中性工作台** | accent 改为近中性蓝灰或锌色系可点色（如 `#2563EB` 仅链接级，或更淡的 blue） | 更贴 Cursor |
| **B. 保留 indigo，降暴露** | token 不变；一页最多一个 `default` 主 CTA，其余改 secondary / ghost；accent 保留在 focus ring、链接、checked 与少量状态 | **Phase 0 已选** |
| **C. 双 token** | `--accent` 操作色 + `--focus-ring` 中性 | 最灵活 |

**执行决策**：Phase 0 只做 B，且不新增「中性主按钮」变体；先用现有组件层级减少 `variant="default"` 数量。A 延后到 Phase 5，并以 Phase 1 实机截图为前置。Agent 品牌色继续只用于点与图表。

### 4.7 间距与密度

| 区域 | 目标 |
|------|------|
| 表行 | `py-1.5`–`py-2`（现 `py-2.5` 偏松） |
| 侧栏导航项 | 保持紧凑 h≈28–32 |
| 页头 | 减 `pt-5`/`mb-4` 空气；标题区 **单行标题 + 一行 meta** |
| 预览 header | 单行标题 + 可选次行路径（muted meta） |
| 分栏 | 左列表 padding `px-4` 级，避免 `px-6` 在全宽下仍「留白带」过宽 |

---

## 5. 提示与辅助信息架构

### 5.1 信息分级（写进组件用法）

| 级别 | 定义 | 载体 | 示例 |
|------|------|------|------|
| **L0 必需** | 没有就无法操作 | 主界面常驻短标签 | Tab 名、列名、主按钮 |
| **L1 状态** | 当前对象状态 | 行内短 badge / 点 | 冲突、仅本工具、已在库 |
| **L2 解释** | 为什么/怎么做 | `Hint` 悬停 ≤2 句 | 格子为何灰、按钮禁用原因 |
| **L3 教学** | 产品模型/路径哲学 | 空状态 / `?` 帮助 / 首次 callout（可关闭） | 单向投影、共享库路径 |
| **L4 系统** | 路径、ID、调试 | 预览 footer / 复制 / 打开目录 | `sourceDir` |

**铁律**：L3 不得默认占 Skills 主列；L4 不得做每行 `title` 复读。

### 5.2 提示组件规范

| 场景 | 组件 | 文案长度 | 延迟 |
|------|------|----------|------|
| 图标按钮 | `Button title` → Hint | ≤20 字 | 全局 **200ms** |
| 截断文本 | `Tip` | 完整字符串或「名 · 一句话」 | 全局 **200ms** |
| 禁用原因 | Hint on wrapper | 一句原因 | 全局 **200ms** |
| 行/单元格教学 | **禁止**整行 native title | — | — |
| 危险后果 | Dialog，不靠 tip | 结构化列表 | — |
| 成功反馈 | Toast：标题动作结果 + 可选一句下一步 | 标题 ≤16 字 | — |

`src/main.tsx` 的 `TooltipProvider delayDuration={200}` 是当前真源；Phase 0 不创建 200/300/400ms 多档方言。只有经可用性测试证明必要时，才允许组件显式覆盖 `Hint.delayDuration`。

### 5.3 文案语气（对标 Codex 的短、Cursor 的准）

| 避免 | 改为 |
|------|------|
| 未启用，点击启用到此工具（单向投影，非双向同步） | 未启用 · 点击启用 |
| 复制到共享库 ~/.agents/skills（不删本工具里的原文件） | 加入共享库（保留本工具文件） |
| 双击预览 SKILL.md · 右键更多操作 | （删行 title；首次提示可写：单击名称预览） |
| 这里是本地共享库。每一列是一个 AI 工具；… | 删除常驻段；图例折叠内保留一句 |

**术语表（产品内统一）**：

| 内部概念 | UI 默认用词 | 专家向（帮助） |
|----------|-------------|----------------|
| shared skills root | 共享库 | `~/.agents/skills` |
| projection / sync cell | 启用到某工具 | 单向投影 |
| private only | 只在本工具 | — |
| SKILL.md preview | 预览 | — |

### 5.4 Toast 模板

| 类型 | title | description |
|------|-------|-------------|
| 成功 | 已加入共享库 | 可在矩阵中启用到其他工具 |
| 失败 | 无法启用 | 一句可读原因（去掉 stack） |
| 带动作 | 已安装到共享库 | 描述一句 + `去矩阵启用` |

避免把完整路径塞进 description，除非「打开目录」是主诉求。

---

## 6. 预览信息体验规格

### 6.1 交互状态机

```
idle ──(单击名称|Enter|右键“预览”)──► previewOpen(activeKey)
previewOpen ──(单击另一名称|↑↓)──► previewOpen(新 activeKey)  // 不关面板
previewOpen ──(筛选隐藏 active 行)──► previewOpen(activeKey)  // 左侧可无可见 active
previewOpen ──(Esc|收起)──► idle
checkbox 多选 ⟂ activeKey  // 两套状态，禁止混用
```

`activeKey` 必须能区分同名/同 id 的共享与 agent 私有来源：

- 共享：`shared:${skillId}`
- 私有：`agent:${privateAgent}:${skillId}`

不得只以 `skillId` 或显示名判断 active。`sourceDir` 是展示/打开目录信息，不作为稳定身份。

**命中范围**：只有名称按钮/链接、`Enter` 和右键菜单「预览」会改变 `activeKey`。单击 checkbox、矩阵格、行内菜单或空白行区域，不得附带切换预览。

**跨筛选行为（已选 B）**：搜索、Agent/状态过滤不关闭预览；目标被过滤掉时不伪造可见 active 行，预览 header 持续显示名称与「共享库 / Agent」来源。清除过滤后，对应行恢复 active。此规则只约束筛选；Tab 切换维持现状，不在 Phase 1 扩张状态持久化范围。

**视觉状态优先级**：

| 状态 | 表达 | 与其他状态的关系 |
|------|------|------------------|
| Disabled | muted/disabled + 禁止交互 | 最高；不显示 hover |
| Focus-visible | 2px focus ring 或 inset outline | 可叠在 active 上，不用背景替代 |
| Active preview | `bg-active` 中性实底 | 不等于 selected；Phase 1 默认不加 accent 左条 |
| Selected | 仅 checkbox 与批量工具栏表达 | 不改变整行底色 |
| Hover | `bg-hover` | 仅非 active、非 disabled 时生效 |

### 6.2 布局

```
┌─ Skills (full bleed) ─────────────────────────────────────┐
│ Header: 标题 · 短 meta          [安装]                    │
├──────────────────────────┬─┬──────────────────────────────┤
│ Tabs + 过滤（紧）         │┆│ Preview chrome（单行）       │
│ 列表/矩阵（canvas）       │┆│ 预览|源码  打开目录  收起   │
│ active 行: bg-active     │┆│ ─────────────────────────── │
│ checkbox 仅表达多选      │┆│ Markdown 文档区（scroll）    │
│                          │┆│ footer: 路径 meta（可复制）  │
└──────────────────────────┴─┴──────────────────────────────┘
         separator 可拖/键盘调整；双击 separator → 默认宽度
```

### 6.3 加载与切换

| 场景 | 行为 |
|------|------|
| 首次打开 | 面板框架与固定高度骨架 3–5 行先出现；不使用孤立居中 spinner |
| 切换 skill | header 立即显示新目标，文档区切为同尺寸骨架 + 顶部 1px progress；**不得保留与新标题不匹配的旧正文**，也不得整栏闪白 |
| 请求竞态 | 响应必须按 `activeKey` 校验；过期响应直接丢弃，header/mode/content 同一目标 |
| 模式 | Markdown/源码模式在 skill 切换时保持；关闭面板后本次会话内可保持 |
| 失败 | 文档区显示目标名 + 一句错误 + 重试；不关面板，不回填旧 skill 内容 |
| 截断 | header/footer「已截断」muted，不弹 modal |

加载期间文档容器设置 `aria-busy="true"`；骨架不进入 Tab 顺序。

### 6.4 预览 chrome 收敛

- 标题：skill **显示名**（body 档 + semibold）。  
- 路径：默认 **不占第二行主视线**；放 footer 或 title tip。  
- 来源：header 用短 meta「共享库」或 Agent 名，保证跨筛选后仍知道当前预览对象来自哪里。
- 模式切换：segment 高度 24–28，与全站 Segmented 统一。  
- 收起：仅一个 icon 按钮（PanelRightClose），tooltip「收起预览」。
- `MarkdownView variant="document"`：H1=`text-title`（16），H2–H4/正文=`text-body`（13），code/pre=`text-meta`（12）；首个标题上边距为 0。不得直接沿用库默认的大号 GitHub README 排版。

### 6.5 键盘与焦点（Phase 1 最小闭环）

| 键 | 行为 |
|----|------|
| ↑ / ↓ | 列表有焦点且预览已开时移动 active；输入框/菜单内不劫持 |
| Enter | 名称有焦点时打开/切换预览；预览已开时把焦点移到文档区 |
| Esc | 仅在无 Dialog / Menu / Popover 等更高层浮层时关闭预览；浮层先消费 Esc |
| Tab | 名称 → 行内操作 → 分隔条 → 预览工具栏 → 文档区，顺序稳定 |
| 分隔条 ← / → | 每次调整 16px；`Shift` + 箭头调整 48px；有 `role="separator"` 与当前值 |
| 可选 Ctrl/Cmd+O | 打开目录 |

---

## 7. 分阶段优化方案

### Phase 0 — Token 与规范落地（0.5–1 天）

**改动**：

1. `globals.css`：只新增 `--bg-active`（浅 `#E4E4E7` / 深 `#2c2c31`，与 `tokens.ts` 一致）与 `--text-disabled`（浅 `#A1A1AA` / 深 `#52525B`）；**不改**现有 secondary/muted/accent 数值。
2. Accent 采用方案 B：盘点 Skills 中 `variant="default"`，一页只留一个主 CTA，其余用现有 secondary / ghost；不新建按钮变体、不换色相。
3. `docs/ui-design.md` 同步 token、Skills fullBleed 特例与本文边界。
4. 提示延迟统一沿用 `src/main.tsx` 的 200ms；在本文与 `tooltip.tsx` 约定中补「禁止业务 native title」核对清单。

**验收**：Skills 浅/深色 token 截图；active/hover/disabled 可区分，小字可读；accent 面积不增加；无业务逻辑变化。Phase 0 不全局压缩 `tableStyles`，避免 Dashboard 等表格被连带回归。

### Phase 1 — Skills 静音 + 预览一体（2–3 天）**【优先】**

**P1a — 静态层级与内容降噪**

| 项 | 动作 |
|----|------|
| 文案 | 删除 Tab 下常驻长说明；图例提供明确折叠按钮，默认折叠，并记住用户选择 |
| 表壳 | 为 `TableShell` 增加显式 workbench/flush 变体（组件已有；**Skills 矩阵仍用默认 TableShell = Card 壳，flush 未用**）；不修改所有表格默认值 |
| 列表 | 名称 13px 主文；描述存在时固定一行 secondary + ellipsis，所有行高度稳定；绝对路径移出行，放预览/footer |
| 提示 | 去掉行级 native `title` 教学串；按钮保留短 Hint，统一 200ms |
| Markdown | 收敛 `MarkdownView(document)` 标题、段间距、代码块与列表密度，按 §6.4 验收 |

**P1b — 预览状态与焦点闭环**

| 项 | 动作 |
|----|------|
| Active | `previewTarget` 派生复合 `activeKey`，驱动 `data-active` / `bg-active`；与 checkbox selected 分离 |
| 打开 | 单击名称或 `Enter` 打开/切换；矩阵格、checkbox、菜单按钮不改变预览 |
| 筛选 | 落实 B：筛选隐藏目标时保留预览，允许无可见 active；header 保留来源 meta |
| 加载 | 同尺寸骨架 + progress；按 `activeKey` 丢弃过期响应，禁止旧正文配新标题 |
| 预览 | header 收成单行主层；左 canvas / 右 panel；路径放 footer/Tip |
| 分隔与键盘 | 拖动时 body 禁选；双击重置；separator 可聚焦与方向键调整；完成 §6.5 最小闭环 |

**验收**：对照 §1.3 成功标准 1–3；另外必须覆盖 shared/private 同 id、快速切换请求竞态、active 被筛选隐藏、Dialog 打开时 Esc 不误关预览。

### Phase 2 — 提示与 Toast 文案包（0.5–1 天）

1. 集中 `skills` 文案表（可 `src/pages/skills/copy.ts`）：格子 tip、采用/删除确认、Toast。  
2. 按 §5.3 全面缩短。  
3. 市场 Tab 说明缩为一行 + 链到外站。

**验收**：任意操作 tip ≤2 短句；无「单向投影」出现在 L0/L1。

### Phase 3 — 全站控件方言收敛

**已落地**：`ListRow`；Segmented / Tabs / `AgentTabStrip` 同一视觉族；`PageHeader size="compact"`。

**仍做（以 [ui-component-standard.md](ui-component-standard.md) §8 为准，不按「天」排期）**：

1. SearchField 收口（Skills / Chat / Projects；Connections 无搜索框；禁止再手写搜索框）。  
2. 共享件字号改语义 token（`text-title` / `text-body` / `text-meta`）；存量页面别名不搞大扫除。  
3. loading 用 Skeleton，不用一句「正在加载…」。  
4. Accent 面积：一页一主 CTA，随 PR 自检。  
5. Card：新卡避免双重描边；不重做旧卡密度。

**先不做**：`TableShell` 改 workbench、`ListRow` 去边框、全站 `text-sm` 机械替换。

### Phase 4 — 动效与主题质感（1 天）

1. 预览开合宽度 transition（拖拽中关闭 transition）。
2. `prefers-reduced-motion` 已有 boot，扩到面板。
3. 复核深色 active/muted/Markdown 对比。
4. 不以「6px 滚动条」作为对标目标；仅在系统滚动条明显破坏布局时单独处理。

### Phase 5 — 主题精修（可选）

1. Accent 方案 A 试色（A/B 或设置项「强调色」过重则不做）。  
2. 深色模式同步 active/muted 对比。  
3. 用同一状态重拍 Phase 1 前后图；若色相仍显著抢视线，再进入 A 试色。

---

## 8. 文件级改造地图

| 区域 | 文件 | Phase |
|------|------|-------|
| Token | `src/styles/tokens.ts`, `tailwind.config.ts` | 0 |
| 表壳/密度 | `src/components/ui/table.tsx`（新增 workbench/flush 变体，不改默认表壳） | 1 |
| 提示 | `src/components/ui/tooltip.tsx`, 清 Skills `title=` | 1–2 |
| 预览 | `SkillMarkdownPreviewPanel.tsx` | 1 |
| 文档排版 | `src/components/shared/MarkdownView.tsx` 的 `document` variant | 1 |
| Skills 壳 | `pages/skills/index.tsx` | 1 |
| 矩阵/工作区 | `SkillMatrix.tsx`, `AgentWorkspace.tsx` | 1–2 |
| 文案 | 新建 `pages/skills/copy.ts`（建议） | 2 |
| 布局特例 | `App.tsx`（Chat + Skills 已 fullBleed，保持） | — |
| 设计契约 | `docs/ui-design.md` 同步 token 与 Chat/Skills fullBleed 边界 | 0 |
| 按钮/输入 | `button.tsx`, `input.tsx` | 3 |

---

## 9. 明确不抄的清单

| Cursor / Codex 点 | AgentHub 做法 |
|-------------------|---------------|
| Cursor 橙主色 | 不用；仅 agent 点 |
| 以编辑器为中心 | 保留功能导航 + Agent 筛选 |
| 极简到无管理矩阵 | **保留** skill×agent 矩阵（核心价值） |
| 英文 UI only | 导航英文专有名词 + 中文正文（已有原则） |
| 命令面板驱动一切 | 可后做；不阻塞视觉对齐 |

---

## 10. 风险与约束

| 风险 | 缓解 |
|------|------|
| 减文案导致新手不懂「投影」 | L3 首次 callout + 图例折叠内一句；非每行复读 |
| active 与 selected 两态用户混 | 视觉区分：checkbox 勾选 vs 行底 active；文案避免「选中」混用 |
| 改 token 影响全站 | Phase 0 先加 `--bg-active` 等非破坏项；accent 面积先减后改色 |
| 表格更紧导致误点 | 矩阵格子保持最小 32px 点击热区 |
| shared/private 出现同 `skillId` | 使用复合 `activeKey`，测试同 id 不串 active/响应 |
| 筛选后找不到预览来源 | 采用 B：保留预览，header 常驻短来源 meta；不强行让隐藏行 active |
| Markdown 过度压缩损害文档可读性 | 只收敛 heading/spacing/chrome，正文保持 13px/1.45，代码保持 12px mono |

---

## 11. 建议执行顺序（历史排期；Phase 0–2 已大体落地）

```text
已落地: Phase 0 + Phase 1 + Phase 2（token / Skills 静音与预览 / 文案包）
Phase 3 收口: 文档 + SearchField / 共享件字号 / 钱包骨架（见 ui-component-standard.md）；其余随 PR
Backlog: Phase 4–5
```

Phase 0–1 的首个 PR 当时标题：`style(skills): quiet chrome, active row, preview surface hierarchy`。

---

## 12. 附录

### 12.1 自检清单（PR 前）

- [ ] 主界面无超一行教学段落  
- [ ] 预览打开时 active 行可辨  
- [ ] 无业务原生 `title` 教学串（允许空或极短）  
- [ ] 无双重 Card 描边套表  
- [ ] 字号仅来自角色表  
- [ ] Toast 无堆路径  
- [ ] 浅色截图灰阶不脏、accent 不霸屏  
- [ ] Tooltip 默认延迟与 Provider 同为 200ms
- [ ] shared/private 同 id 不串 active；过期响应不覆盖新预览
- [ ] 筛选隐藏 active 时预览保留，header 来源明确
- [ ] Markdown H1/H2 不压过预览 header，首屏无库默认大号 README 感

### 12.2 对照结论（仓库不存实机图）

当时核验条件：Windows 浅色主题；窗口宽度均为 1440px。AgentHub 与 ChatGPT/Codex 为 1440×900；Cursor 原图同宽，为避免账户区域进入仓库裁去底部 80px。  
**仓库不存实机图**（`docs/assets/ui-experience-alignment/` 目录不存在），以 `pnpm dev:mock` 为准。AgentHub Skills 演示须用 mock 路径（`c:\mock\…`），不得提交含真实用户目录的截图。

#### A. AgentHub Skills — 关闭预览

**可见结论**：侧栏选中态与画布灰阶已接近桌面工具；噪声集中在内容区——Tab 下常驻教学段、Card 外壳、表头与逐行分隔同时存在，且每行把名称、描述、绝对路径全部常驻。Phase 1 应减内容面噪声，不重做侧栏。

#### B. AgentHub Skills — 打开预览

**可见结论**：左右分栏方向正确，但左侧没有与右侧目标对应的 active 行；右栏双层 header、常驻路径和 Markdown 大标题共同抢视线。仅瘦身预览容器不足，必须同时处理 `activeKey` 与 `MarkdownView(document)`。

#### C. Cursor Settings

**可见结论**：当前项主要靠中性灰底而非品牌色；设置组靠轻微面差分层，外包边框与逐行分隔很少。AgentHub 可借其「中性 active + 少边框」，无需照抄具体圆角或颜色。

#### D. ChatGPT/Codex 工作台

**可见结论**：主区域大面积留白，任务卡与 composer 才有弱轮廓；一个时刻只有一个明确任务入口。AgentHub 不适合复制这种留白比例，但应借其「说明不常驻、路径不铺满、功能色只落在小图标/状态」的克制。

#### E. 对 Phase 0–1 的直接判断

| 判断项 | 实机结论 |
|--------|----------|
| 灰阶与侧栏 | 基础合格，不列 Phase 0 重构项 |
| 边框 | 诊断成立；Skills 表格壳与行线明显比 Cursor 重 |
| 字号 | UI 字号总体接近；主要问题是 Markdown 标题与三行元信息，而非 body 13px |
| Accent | 当前曝光不高；Phase 0 选 B 正确，换色相后置 |
| 预览 | 分栏成立；active、加载竞态、header/Markdown 层级是 Phase 1 必改 |
| 辅助信息 | 常驻教学段和绝对路径过吵；折叠图例与路径后移成立 |

### 12.3 相关文档

- [ui-design.md](ui-design.md) — 布局与页面契约  
- [ui-component-standard.md](ui-component-standard.md) — 组件用法标准与决策树  
- [chat-process-streaming.md](chat-process-streaming.md) — Chat 过程展示（另一条体验线）  
- [architecture.md](architecture.md) — 前端分层  

---

## 13. 结论

实机核验后，AgentHub 的侧栏与基础灰阶已具备桌面工具底子；Skills 体感弱于 Cursor / Codex，主因不是「少做一两个按钮」，而是：

1. **表面分层被边框替代**，IDE 感不足；  
2. **角色与元信息密度不清**，一行承载名称、描述、路径；
3. **L3 教学信息占满 L0 主界面**；  
4. **预览与列表未完成焦点叙事**；  
5. **管理后台式 Table/Card 外壳**重于当前有限的 accent 曝光。

Phase 0–2 已大体落地，Skills 上「表面层级 + 预览焦点 + 辅助信息」三条主线已对齐。Phase 0 保留 indigo 并降低暴露；是否换色相仍属 Phase 5 计划，不为对标而抄品牌皮肤。Skills 矩阵仍用默认 `TableShell`（Card 壳），`flush` 未用。
