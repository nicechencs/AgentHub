/**
 * 页面区块节奏 — 与 docs/ui-design.md 间距阶梯（4/8/12/16/24/32）对齐。
 * 业务页优先引用语义 class，避免自创 mb-3 / mt-4 / mt-8 混用。
 *
 * ## 边缘（edge）
 * | 场景 | 水平 | 垂直 |
 * |------|------|------|
 * | 常规页（App main） | 24 (`px-6`) | 24 (`py-6`) |
 * | TopBar | 24 (`px-6`) | h-10 |
 * | Skills / Projects 全高工作台 | 24 (`px-6`) | 列表 `py-3`；预览见 `pageEdgePx` |
 * | Chat 全高 | 主区 chrome 16 (`px-4`) | 会话 rail 自管 |
 *
 * 层级（自上而下）：
 * 1. PageHeader（自带 mb-4 / compact mb-2）
 * 2. chrome：筛选条 / AgentTab / 工具行
 * 3. lead：环境条、Notice 等引导块
 * 4. stack / blocks：列表与同段卡片
 * 5. section / sectionRuled：主内容大段
 */
export const pageRhythm = {
  /** 常规页外壳：App 对非 fullBleed 路由施加 */
  pageShell: 'mx-auto max-w-content px-6 py-6',
  /** 全高工作台水平 inset（Skills / Projects 页眉 / 列表）— 与常规页水平一致 */
  workbenchX: 'px-6',
  /** 全高工作台内容区垂直（列表） */
  workbenchY: 'py-3',
  /** Chat 主区 chrome 水平（header / composer 外框） */
  chatChromeX: 'px-4',

  /** 页头下：Agent 条 / Tabs / 单行筛选工具带 */
  chrome: 'mb-3',
  /** 工具行（flex 包装 + 底距） */
  chromeRow: 'mb-3 flex flex-wrap items-center gap-2',
  /** Header 后引导区（环境条、提示组） */
  lead: 'mb-4 space-y-3',
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
  /** 与 px-6 一致：常规页 / Skills / Projects 预览距窗边 */
  x: 24,
  /** Skills / Projects 预览卡片上下距画布 */
  previewY: 12,
  /** 分隔条约宽 */
  separator: 6,
} as const;
