export type HelpRect = { top: number; left: number; width: number; height: number };
export type HelpPlacement = 'top' | 'bottom' | 'left' | 'right';
export type HelpDock =
  | 'center'
  | 'center-left'
  | 'center-right'
  | 'top-center'
  | 'top-left'
  | 'top-right'
  | 'bottom-left'
  | 'bottom-right'
  | 'bottom-center';

export const HELP_BUBBLE_WIDTH = 360;
export const HELP_BUBBLE_GAP = 10;
export const HELP_VIEW_PAD = 8;
export const HELP_DOCKS: readonly HelpDock[] = [
  'center',
  'center-left',
  'center-right',
  'top-center',
  'top-left',
  'top-right',
  'bottom-left',
  'bottom-right',
  'bottom-center',
];
export const HELP_HIGHLIGHT_PAD = 6;
export const HELP_HIGHLIGHT_MAX_W = 420;
export const HELP_HIGHLIGHT_MAX_H = 72;
export const HELP_ARROW_INSET = 14;

export const PAGE_HELP_FALLBACK_TARGET = '[data-page-help]';
export const PAGE_HELP_TITLE_TARGET = '[data-help="page-title"]';

function clamp(n: number, min: number, max: number): number {
  if (max < min) return min;
  return Math.min(max, Math.max(min, n));
}

export function expandHighlight(target: HelpRect, pad = HELP_HIGHLIGHT_PAD): HelpRect {
  return {
    top: target.top - pad,
    left: target.left - pad,
    width: target.width + pad * 2,
    height: target.height + pad * 2,
  };
}

/** Keep the spotlight on the control, not the whole list column. */
export function capHighlight(
  rect: HelpRect,
  maxW = HELP_HIGHLIGHT_MAX_W,
  maxH = HELP_HIGHLIGHT_MAX_H,
): HelpRect {
  return {
    top: rect.top,
    left: rect.left,
    width: Math.min(rect.width, maxW),
    height: Math.min(rect.height, maxH),
  };
}

export function helpViewportSize(
  win: { innerWidth: number; innerHeight: number } | null | undefined = typeof window === 'undefined' ? null : window,
): { width: number; height: number } {
  if (!win) return { width: 0, height: 0 };
  return { width: win.innerWidth, height: win.innerHeight };
}

/** Visible slice of a control in the layout viewport. Off-screen → null. */
export function visibleOverlap(
  rect: HelpRect,
  viewport: { width: number; height: number },
): HelpRect | null {
  const left = Math.max(rect.left, 0);
  const top = Math.max(rect.top, 0);
  const right = Math.min(rect.left + rect.width, viewport.width);
  const bottom = Math.min(rect.top + rect.height, viewport.height);
  const width = right - left;
  const height = bottom - top;
  if (width <= 0 || height <= 0) return null;
  return { top, left, width, height };
}

/** Prefer the control that is actually on screen; otherwise the first one with size. */
export function pickHelpRect(
  rects: readonly HelpRect[],
  viewport: { width: number; height: number },
): HelpRect | null {
  if (rects.length === 0) return null;
  let best = rects[0];
  let bestArea = 0;
  for (const rect of rects) {
    const vis = visibleOverlap(rect, viewport);
    const area = vis ? vis.width * vis.height : 0;
    if (area > bestArea) {
      best = rect;
      bestArea = area;
    }
  }
  return best;
}

function rectFromElement(el: HTMLElement): HelpRect | null {
  const rect = el.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return null;
  return { top: rect.top, left: rect.left, width: rect.width, height: rect.height };
}

export type HelpTargetHit = {
  selector: string;
  element: HTMLElement;
  rect: HelpRect;
};

export function listHelpHits(
  selector: string,
  root: ParentNode | null | undefined,
): Array<{ element: HTMLElement; rect: HelpRect }> {
  if (!root) return [];
  let nodes: NodeListOf<Element>;
  try {
    nodes = root.querySelectorAll(selector);
  } catch {
    return [];
  }
  const hits: Array<{ element: HTMLElement; rect: HelpRect }> = [];
  nodes.forEach((node) => {
    if (!(node instanceof HTMLElement)) return;
    const rect = rectFromElement(node);
    if (rect) hits.push({ element: node, rect });
  });
  return hits;
}

export function pickHelpHit(
  hits: readonly { element: HTMLElement; rect: HelpRect }[],
  viewport: { width: number; height: number },
): { element: HTMLElement; rect: HelpRect } | null {
  if (hits.length === 0) return null;
  const picked = pickHelpRect(hits.map((hit) => hit.rect), viewport);
  return hits.find((hit) => hit.rect === picked) ?? hits[0];
}

export function resolveHelpTarget(
  selector: string,
  root: ParentNode | null | undefined = typeof document === 'undefined' ? null : document,
  viewport: { width: number; height: number } = helpViewportSize(),
): HelpRect | null {
  return pickHelpHit(listHelpHits(selector, root), viewport)?.rect ?? null;
}

export function pickHelpTargetRect(
  preferred: string,
  root: ParentNode | null | undefined = typeof document === 'undefined' ? null : document,
  fallback = false,
  viewport: { width: number; height: number } = helpViewportSize(),
): HelpTargetHit | null {
  const selectors = fallback
    ? [preferred, PAGE_HELP_TITLE_TARGET, PAGE_HELP_FALLBACK_TARGET]
    : [preferred];
  for (const selector of selectors) {
    const hit = pickHelpHit(listHelpHits(selector, root), viewport);
    if (hit) return { selector, ...hit };
  }
  return null;
}

export type PageHelpKeyAction = 'back' | 'next' | 'skip';

function pageHelpTypingTarget(target: EventTarget | null): boolean {
  if (!target || typeof target !== 'object') return false;
  const el = target as { tagName?: string; isContentEditable?: boolean };
  const tag = el.tagName?.toUpperCase();
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  return Boolean(el.isContentEditable);
}

/** Map a key to tour back / next / skip. Ignores typing and modified keys. */
export function pageHelpKeyAction(
  event: {
    key: string;
    altKey?: boolean;
    ctrlKey?: boolean;
    metaKey?: boolean;
    defaultPrevented?: boolean;
  },
  target: EventTarget | null = null,
): PageHelpKeyAction | null {
  if (event.defaultPrevented) return null;
  if (event.altKey || event.ctrlKey || event.metaKey) return null;
  if (event.key === 'Escape') return 'skip';
  if (pageHelpTypingTarget(target)) return null;
  const tag = target && typeof target === 'object'
    ? (target as { tagName?: string }).tagName?.toUpperCase()
    : undefined;
  if (event.key === 'Enter' && tag === 'BUTTON') return null;
  if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') return 'back';
  if (event.key === 'ArrowRight' || event.key === 'ArrowDown' || event.key === 'Enter') return 'next';
  return null;
}

/** Keep steps whose preferred target is on screen; if none, stay on step 0. */
export function filterVisibleHelpSteps(found: readonly boolean[]): number[] {
  const indexes = found
    .map((ok, i) => (ok ? i : -1))
    .filter((i) => i >= 0);
  return indexes.length > 0 ? indexes : [0];
}

export type HelpBubbleLayout = {
  top: number;
  left: number;
  placement: HelpPlacement;
  highlight: HelpRect | null;
  /** Distance from the start of the connecting edge to the arrow center. */
  arrowOffset: number;
  /** Stable corner/edge slot; `adjacent` is a last-resort fallback. */
  dock: HelpDock | 'adjacent';
};

/** Dim panes around a click-through hole so the highlighted control stays usable. */
export function dimPaneRects(
  hole: HelpRect | null,
  viewport: { width: number; height: number },
): HelpRect[] {
  const { width: vw, height: vh } = viewport;
  if (!hole) return [{ top: 0, left: 0, width: vw, height: vh }];
  const right = hole.left + hole.width;
  const bottom = hole.top + hole.height;
  return [
    { top: 0, left: 0, width: vw, height: Math.max(0, hole.top) },
    { top: hole.top, left: 0, width: Math.max(0, hole.left), height: hole.height },
    { top: hole.top, left: right, width: Math.max(0, vw - right), height: hole.height },
    { top: bottom, left: 0, width: vw, height: Math.max(0, vh - bottom) },
  ].filter((pane) => pane.width > 0 && pane.height > 0);
}

export function rectsOverlap(a: HelpRect, b: HelpRect, gap = HELP_BUBBLE_GAP): boolean {
  return (
    a.left < b.left + b.width + gap &&
    a.left + a.width + gap > b.left &&
    a.top < b.top + b.height + gap &&
    a.top + a.height + gap > b.top
  );
}

export function dockOrigin(
  dock: HelpDock,
  viewport: { width: number; height: number },
  bubble: { width: number; height: number },
): { top: number; left: number } {
  const maxLeft = viewport.width - bubble.width - HELP_VIEW_PAD;
  const maxTop = viewport.height - bubble.height - HELP_VIEW_PAD;
  const left = {
    left: HELP_VIEW_PAD,
    center: clamp((viewport.width - bubble.width) / 2, HELP_VIEW_PAD, Math.max(HELP_VIEW_PAD, maxLeft)),
    right: clamp(maxLeft, HELP_VIEW_PAD, Math.max(HELP_VIEW_PAD, maxLeft)),
  };
  const top = {
    top: HELP_VIEW_PAD,
    center: clamp((viewport.height - bubble.height) / 2, HELP_VIEW_PAD, Math.max(HELP_VIEW_PAD, maxTop)),
    bottom: clamp(maxTop, HELP_VIEW_PAD, Math.max(HELP_VIEW_PAD, maxTop)),
  };
  const [vertical, horizontal] = (dock === 'center' ? 'center-center' : dock).split('-') as [
    'top' | 'center' | 'bottom',
    'left' | 'center' | 'right',
  ];
  return { top: top[vertical], left: left[horizontal] };
}

function inViewport(
  top: number,
  left: number,
  bubble: { width: number; height: number },
  viewport: { width: number; height: number },
): boolean {
  return (
    top >= HELP_VIEW_PAD &&
    left >= HELP_VIEW_PAD &&
    top + bubble.height <= viewport.height - HELP_VIEW_PAD &&
    left + bubble.width <= viewport.width - HELP_VIEW_PAD
  );
}

function arrowToward(
  pos: { top: number; left: number },
  bubble: { width: number; height: number },
  highlight: HelpRect | null,
): { placement: HelpPlacement; arrowOffset: number } {
  if (!highlight) {
    return { placement: 'bottom', arrowOffset: clamp(bubble.width / 2, HELP_ARROW_INSET, bubble.width - HELP_ARROW_INSET) };
  }
  const cx = pos.left + bubble.width / 2;
  const cy = pos.top + bubble.height / 2;
  const hx = highlight.left + highlight.width / 2;
  const hy = highlight.top + highlight.height / 2;
  const overlapX = hx >= pos.left && hx <= pos.left + bubble.width;
  let placement: HelpPlacement;
  if (hy < cy && (Math.abs(hy - cy) >= Math.abs(hx - cx) || overlapX)) {
    placement = 'bottom';
  } else if (hy > cy && (Math.abs(hy - cy) >= Math.abs(hx - cx) || overlapX)) {
    placement = 'top';
  } else if (hx < cx) {
    placement = 'right';
  } else {
    placement = 'left';
  }
  const alongEdge = placement === 'top' || placement === 'bottom' ? hx - pos.left : hy - pos.top;
  const edgeMax = placement === 'top' || placement === 'bottom' ? bubble.width : bubble.height;
  return {
    placement,
    arrowOffset: clamp(alongEdge, HELP_ARROW_INSET, Math.max(HELP_ARROW_INSET, edgeMax - HELP_ARROW_INSET)),
  };
}

function layoutAt(
  pos: { top: number; left: number },
  dock: HelpBubbleLayout['dock'],
  highlight: HelpRect | null,
  bubble: { width: number; height: number },
): HelpBubbleLayout {
  const arrow = arrowToward(pos, bubble, highlight);
  return { top: pos.top, left: pos.left, highlight, dock, ...arrow };
}

function placeAdjacentHelpBubble(
  highlight: HelpRect,
  viewport: { width: number; height: number },
  bubble: { width: number; height: number },
): HelpBubbleLayout {
  const maxLeft = viewport.width - bubble.width - HELP_VIEW_PAD;
  const maxTop = viewport.height - bubble.height - HELP_VIEW_PAD;
  const cx = highlight.left + highlight.width / 2;
  const cy = highlight.top + highlight.height / 2;
  const candidates: Array<{ placement: HelpPlacement; top: number; left: number }> = [
    {
      placement: 'bottom',
      top: highlight.top + highlight.height + HELP_BUBBLE_GAP,
      left: cx - bubble.width / 2,
    },
    {
      placement: 'top',
      top: highlight.top - bubble.height - HELP_BUBBLE_GAP,
      left: cx - bubble.width / 2,
    },
    {
      placement: 'right',
      top: cy - bubble.height / 2,
      left: highlight.left + highlight.width + HELP_BUBBLE_GAP,
    },
    {
      placement: 'left',
      top: cy - bubble.height / 2,
      left: highlight.left - bubble.width - HELP_BUBBLE_GAP,
    },
  ];

  let chosen = candidates[0];
  for (const c of candidates) {
    if (inViewport(c.top, c.left, bubble, viewport)) {
      chosen = c;
      break;
    }
  }
  const left = clamp(chosen.left, HELP_VIEW_PAD, Math.max(HELP_VIEW_PAD, maxLeft));
  const top = clamp(chosen.top, HELP_VIEW_PAD, Math.max(HELP_VIEW_PAD, maxTop));
  return layoutAt({ top, left }, 'adjacent', highlight, bubble);
}

function dockClear(
  dock: HelpDock,
  viewport: { width: number; height: number },
  bubble: { width: number; height: number },
  highlight: HelpRect | null,
): { top: number; left: number } | null {
  const pos = dockOrigin(dock, viewport, bubble);
  if (!inViewport(pos.top, pos.left, bubble, viewport)) return null;
  if (highlight && rectsOverlap({ ...pos, width: bubble.width, height: bubble.height }, highlight)) {
    return null;
  }
  return pos;
}

/**
 * Keep the bubble on a stable dock so Next/Back/Skip stay put.
 * Move only when that dock would cover the highlighted control.
 */
export function placeHelpBubble(input: {
  target: HelpRect | null;
  viewport: { width: number; height: number };
  bubble: { width: number; height: number };
  previousDock?: HelpDock | null;
}): HelpBubbleLayout {
  const { viewport, bubble } = input;
  const highlight = input.target ? capHighlight(expandHighlight(input.target)) : null;
  const order = input.previousDock
    ? [input.previousDock, ...HELP_DOCKS.filter((dock) => dock !== input.previousDock)]
    : HELP_DOCKS;

  for (const dock of order) {
    const pos = dockClear(dock, viewport, bubble, highlight);
    if (pos) return layoutAt(pos, dock, highlight, bubble);
  }

  if (highlight) return placeAdjacentHelpBubble(highlight, viewport, bubble);
  const fallback = dockOrigin('center', viewport, bubble);
  return layoutAt(fallback, 'center', null, bubble);
}
