/**
 * 页面区块节奏 — 与 docs/ui-design.md 间距阶梯（4/8/12/16/24/32）对齐。
 * 业务页优先引用语义 class，避免自创 mb-3 / mt-4 / mt-8 混用。
 *
 * ## 内容宽度（两套，docs/ui-design.md §3.1）
 * 1. 阅读列 `readingColumn`：Chat 消息列、Settings 表单区。固定 `max-w-3xl` 居中。
 * 2. 贴边列：其余页。铺满主列，左右 18px（`pageShell` / `workbenchX`）。
 * 新页默认贴边列；对话 / 表单 / 长文才用阅读列。禁止第三套 `max-w-*`。
 * 页标题一律贴边、同一行（大号深色标题 + 小号浅色说明），放在非对话页顶栏左侧。
 *
 * ## 边缘（edge）
 * | 场景 | 水平 | 垂直 |
 * |------|------|------|
 * | 窗内画布缝 | 8 (`p-2 gap-2`) | 8 |
 * | 贴边列（相对主列内缘） | 18 (`px-[18px]`) | 18 (`py-[18px]`) |
 * | TopBar | 18 (`px-[18px]`) | h-10（同一行标题 + 说明 + 通知铃） |
 * | Skills / Projects / Connections / Routes / Agents 列表 | 18 (`px-[18px]`)；分栏打开时改 `workbenchXSplit` | 顶 18 / 底 18，与预览列相同 |
 * | Chat 全高 | 主区 chrome 16 (`px-4`) | 会话 rail 自管 |
 * | 阅读列（Chat 消息列 / Settings 表单） | `readingColumn` `max-w-3xl` 居中 | — |
 *
 * 层级（自上而下）：
 * 1. TopBar 页标题（非对话页；对话页自管会话名）
 * 2. 正文起点：顶栏已含页标题。常规页 pageShell 顶距 18px；全高页列表顶距 18px。第一块不加额外顶距
 * 3. chrome / chromeRow：筛选条 / AgentTab / 工具行（`mb-3`）。页内操作放右侧 `chromeActions`，不独占一行
 * 4. lead：环境条、Notice 等引导块
 * 5. stack / blocks：列表与同段卡片
 * 6. section / sectionRuled：主内容大段
 */
export const pageRhythm = {
  /** 窗内画布留缝，侧栏/主列两块圆角面板 */
  shell: 'flex h-full min-h-0 gap-2 bg-canvas p-2',
  shellNav:
    'flex min-h-0 shrink-0 flex-col overflow-hidden rounded-card border border-border bg-panel shadow-xs',
  shellMain:
    'flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-card border border-border bg-canvas shadow-xs',
  /** 常规页外壳：铺满主列，与 Skills / Projects 右缘对齐 */
  pageShell: 'w-full min-w-0 px-[18px] py-[18px]',
  /** Chat 消息列与 Settings 表单区共用：居中阅读宽。页头不进此列。 */
  readingColumn: 'mx-auto w-full max-w-3xl',
  /** 侧栏品牌行与非对话页顶栏同高，横线对齐 */
  topChrome: 'h-10',
  /** 全高工作台水平 inset（Skills / Projects / Connections / Routes 列表）— 与常规页水平一致 */
  workbenchX: 'px-[18px]',
  /**
   * 分栏打开时的列表水平 inset。
   * 左 18 与页头对齐；右内边距收到 8，并把 8px 让到滚动条与分隔条之间。
   * 不要把 18px 右内边距留在 overflow 容器上：那会把空白加在卡片和滚动条之间，
   * 滚动条仍贴着分隔条（分隔条 hit 区还会叠进滚动条）。
   */
  workbenchXSplit: 'pl-[18px] pr-2 mr-2',
  /** Settings 等：顶栏下的 Tab 行，顶距 18px */
  workbenchHeader: 'shrink-0 px-[18px] pt-[18px]',
  /** 页标题：非对话页顶栏 h1（大号、深色） */
  pageTitle: 'text-title font-semibold tracking-tight text-primary',
  /** 页说明：紧跟标题同一行（小号、浅色），过长截断 */
  pageTitleMeta: 'min-w-0 truncate text-meta font-normal text-secondary',
  /** 顶栏标题行：标题与说明基线对齐 */
  pageTitleBlock: 'flex min-w-0 items-baseline gap-2.5',
  /**
   * 全高工作台列表底距。顶距由 WorkbenchSplitPage 与预览列共用 18px，
   * 左右列上下内边对齐。
   */
  workbenchY: 'pb-[18px]',
  /** Chat 消息列 chrome 水平（transcript / composer 外框，不含页头） */
  chatChromeX: 'px-4',

  /** 页头下：Agent 条 / Tabs / 单行筛选工具带 */
  chrome: 'mb-3',
  /** 工具行：与预览列页头同高，左侧筛选、右侧操作 */
  chromeRow: 'mb-3 flex min-h-10 flex-wrap items-center gap-2',
  /** 工具行右侧页内操作，与左侧 Tab/筛选同一行 */
  chromeActions: 'ml-auto flex shrink-0 items-center gap-2',
  /** Header 后引导区（环境条、提示组）；底距与 chrome 相同，切页列表顶边对齐 */
  lead: 'mb-3 space-y-3',
  /** 主列表纵向（Agent 卡、连接卡） */
  stack: 'flex flex-col gap-3',
  /** 更密列表（分组内卡片） */
  stackDense: 'flex flex-col gap-2',
  /** 指标卡网格 */
  metricGrid: 'grid grid-cols-2 gap-3 lg:grid-cols-4',
  /** 同段内块间距（指标 → 图 → 分布） */
  blocks: 'space-y-4',
  /** 二级段顶距（无分割线） */
  section: 'mt-6',
  /** 大段：顶距 + 分割线 + 上内边距 */
  sectionRuled: 'mt-8 border-t border-border pt-6',
  /** 段标题区底距 */
  sectionHead: 'mb-3',
  /**
   * 导航/工具条分组眉题（侧栏 Workspace、Chat 历史）。
   * 页内主内容段请用 `PageSection`（body 档 + semibold），不要再手写 uppercase h2。
   */
  sectionEyebrow: 'text-meta font-medium uppercase tracking-wide text-muted',
  /** 深链滚动偏移 */
  scrollMt: 'scroll-mt-4',
} as const;

/** 像素常量：分栏/预览壳等无法用 Tailwind class 表达时 */
export const pageEdgePx = {
  /** 与 pageShell / workbenchX 一致：常规页 / Skills / Projects 预览距主列内缘 */
  x: 18,
  /** 预览卡片底距，与 pageShell 垂直 inset 一致。顶距由页头 18px 槽承担。 */
  previewY: 18,
  /** 分隔条约宽 */
  separator: 6,
} as const;
