/**
 * 页面区块节奏 — 与 docs/ui/design-system.md 间距阶梯（4/8/12/16/24/32）对齐。
 * 业务页优先引用语义 class，避免自创 mb-3 / mt-4 / mt-8 混用。
 *
 * ## 页边（只改 `pageEdge`）
 * 贴边列、顶栏、工作台列表、预览列共用 `pageEdge.inset`。
 * class 走 `pageRhythm`，分栏像素走 `pageEdgePx`，都从这一处派生。
 * 窗内画布缝是 `pageEdge.canvas`；Chat 主列水平是 `pageEdge.chat`，不要和页边混用。
 *
 * ## 内容宽度（两套，docs/ui/design-system.md §3.4）
 * 1. 阅读列 `readingColumn`：Chat 消息列。固定 `max-w-3xl` 居中。设置表单正文（备份分栏页除外）同列居中；页签留在页头贴左，不进阅读列。
 * 2. 贴边列：其余页。铺满主列，左右用 `pageEdge.inset`（`pageShell` / `workbenchX`）。
 * 新页默认贴边列；对话 / 表单 / 长文才用阅读列。禁止第三套 `max-w-*`。
 * 页标题一律贴边、同一行（大号深色标题 + 小号浅色说明），放在非对话页顶栏左侧。
 *
 * 层级（自上而下）：
 * 1. TopBar 页标题（非对话页；对话页自管会话名）
 * 2. 正文起点：顶栏已含页标题。常规页 pageShell、全高页列表顶距都是 `pageEdge.inset`。第一块不加额外顶距
 * 3. chrome / chromeRow：筛选条 / AgentTab / 工具行（`mb-3`）。页内操作放右侧 `chromeActions`，不独占一行
 * 4. lead：环境条、Notice 等引导块
 * 5. stack / blocks：列表与同段卡片
 * 6. section / sectionRuled：主内容大段
 */

/** Tailwind 完整 class，供 JIT 扫描；只收录阶梯上会当作页边的几档。 */
const SPACE = {
  8: {
    px: 8,
    x: 'px-2',
    y: 'py-2',
    t: 'pt-2',
    b: 'pb-2',
    l: 'pl-2',
    r: 'pr-2',
    mr: 'mr-2',
    p: 'p-2',
    gap: 'gap-2',
  },
  12: {
    px: 12,
    x: 'px-3',
    y: 'py-3',
    t: 'pt-3',
    b: 'pb-3',
    l: 'pl-3',
    r: 'pr-3',
    mr: 'mr-3',
    p: 'p-3',
    gap: 'gap-3',
  },
  16: {
    px: 16,
    x: 'px-4',
    y: 'py-4',
    t: 'pt-4',
    b: 'pb-4',
    l: 'pl-4',
    r: 'pr-4',
    mr: 'mr-4',
    p: 'p-4',
    gap: 'gap-4',
  },
} as const;

type SpacePx = keyof typeof SPACE;

/**
 * 页面几何。改页边只动 `inset`（必须是 8 / 12 / 16）。
 * `pageRhythm` 与 `pageEdgePx` 从这里派生，业务页不要再写死页边 class / 像素。
 */
export const pageEdge = {
  /** 窗内画布缝（侧栏与主列外的 p / gap） */
  canvas: 8,
  /** 主列贴边：pageShell、顶栏、工作台列表、预览列 */
  inset: 8,
  /** Chat 消息列 / composer 水平 chrome，不是页边 */
  chat: 16,
  /** 分栏分隔条占位 */
  separator: 6,
} as const satisfies {
  canvas: SpacePx;
  inset: SpacePx;
  chat: SpacePx;
  separator: number;
};

export const pageCanvasTw = SPACE[pageEdge.canvas];
export const pageInsetTw = SPACE[pageEdge.inset];
export const pageChatTw = SPACE[pageEdge.chat];

export const pageRhythm = {
  /** 窗内画布留缝，侧栏/主列两块圆角面板 */
  shell: `flex h-full min-h-0 ${pageCanvasTw.gap} bg-canvas ${pageCanvasTw.p}`,
  shellNav:
    'flex min-h-0 shrink-0 flex-col overflow-hidden rounded-card border border-border bg-panel shadow-xs',
  shellMain:
    'flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-card border border-border bg-canvas shadow-xs',
  /** 常规页外壳：铺满主列，与 Skills / Projects 右缘对齐 */
  pageShell: `w-full min-w-0 ${pageInsetTw.x} ${pageInsetTw.y}`,
  /** Chat 消息列：居中阅读宽。页头不进此列。设置表单正文（备份分栏页除外）共用同一列。 */
  readingColumn: 'mx-auto w-full max-w-3xl',
  /** 侧栏品牌行与非对话页顶栏同高，横线对齐 */
  topChrome: 'h-10',
  /** 全高工作台水平 inset — 与常规页水平一致 */
  workbenchX: pageInsetTw.x,
  /**
   * 分栏打开时的列表水平 inset。
   * 左缘与页头同为 `pageEdge.inset`；右侧改用画布缝，把空隙让到滚动条与分隔条之间。
   * 不要把页边右距留在 overflow 容器上：那会把空白加在卡片和滚动条之间，
   * 滚动条仍贴着分隔条（分隔条 hit 区还会叠进滚动条）。
   */
  workbenchXSplit: `${pageInsetTw.l} ${pageCanvasTw.r} ${pageCanvasTw.mr}`,
  /** 表单页：顶栏下的 Tab 行，顶距与页边相同。分栏页把 Tab 放进列表列 chromeRow。 */
  workbenchHeader: `shrink-0 ${pageInsetTw.x} ${pageInsetTw.t}`,
  /** 全高列表顶距，与预览列 padTop 相同 */
  workbenchPadT: pageInsetTw.t,
  /** 页标题：非对话页顶栏 h1（大号、深色） */
  pageTitle: 'text-title font-semibold tracking-tight text-primary',
  /** 页说明：紧跟标题同一行（小号、浅色），过长截断 */
  pageTitleMeta: 'min-w-0 truncate text-meta font-normal text-secondary',
  /** 顶栏标题行：标题与说明基线对齐 */
  pageTitleBlock: 'flex min-w-0 items-baseline gap-2.5',
  /**
   * 全高工作台列表底距。顶距由 WorkbenchSplitPage 与预览列共用 `pageEdge.inset`，
   * 左右列上下内边对齐。
   */
  workbenchY: pageInsetTw.b,
  /** Chat 消息列 chrome 水平（transcript / composer 外框，不含页头） */
  chatChromeX: pageChatTw.x,

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

/** 像素常量：分栏/预览壳等无法用 Tailwind class 表达时。与 `pageEdge` 同步。 */
export const pageEdgePx = {
  /** 与 pageShell / workbenchX 一致：常规页 / Skills / Projects 预览距主列内缘 */
  x: pageEdge.inset,
  /** 预览卡片底距，与 pageShell 垂直 inset 一致。 */
  previewY: pageEdge.inset,
  /** 分隔条约宽 */
  separator: pageEdge.separator,
} as const;
