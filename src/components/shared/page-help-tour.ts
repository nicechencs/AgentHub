export type HelpRect = { top: number; left: number; width: number; height: number };
export type HelpPlacement = 'top' | 'bottom' | 'left' | 'right';

export const HELP_BUBBLE_WIDTH = 280;
export const HELP_BUBBLE_GAP = 10;
export const HELP_VIEW_PAD = 8;
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

export function resolveHelpTarget(
  selector: string,
  root: ParentNode | null | undefined = typeof document === 'undefined' ? null : document,
): HelpRect | null {
  if (!root) return null;
  let el: Element | null = null;
  try {
    el = root.querySelector(selector);
  } catch {
    return null;
  }
  if (!(el instanceof HTMLElement)) return null;
  const rect = el.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return null;
  return { top: rect.top, left: rect.left, width: rect.width, height: rect.height };
}

export function pickHelpTargetRect(
  preferred: string,
  root: ParentNode | null | undefined = typeof document === 'undefined' ? null : document,
  fallback = false,
): { selector: string; rect: HelpRect } | null {
  const selectors = fallback
    ? [preferred, PAGE_HELP_TITLE_TARGET, PAGE_HELP_FALLBACK_TARGET]
    : [preferred];
  for (const selector of selectors) {
    const rect = resolveHelpTarget(selector, root);
    if (rect) return { selector, rect };
  }
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
};

/** Place a bubble next to a target, flipping if it would leave the viewport. */
export function placeHelpBubble(input: {
  target: HelpRect | null;
  viewport: { width: number; height: number };
  bubble: { width: number; height: number };
  preferred?: HelpPlacement;
}): HelpBubbleLayout {
  const { viewport, bubble } = input;
  const preferred = input.preferred ?? 'bottom';
  const maxLeft = viewport.width - bubble.width - HELP_VIEW_PAD;
  const maxTop = viewport.height - bubble.height - HELP_VIEW_PAD;

  if (!input.target) {
    const left = clamp(viewport.width - bubble.width - HELP_VIEW_PAD, HELP_VIEW_PAD, Math.max(HELP_VIEW_PAD, maxLeft));
    return {
      top: clamp(HELP_VIEW_PAD + 40, HELP_VIEW_PAD, Math.max(HELP_VIEW_PAD, maxTop)),
      left,
      placement: 'bottom',
      highlight: null,
      arrowOffset: bubble.width - HELP_ARROW_INSET,
    };
  }

  const highlight = capHighlight(expandHighlight(input.target));
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

  const order = [
    preferred,
    ...candidates.map((c) => c.placement).filter((p) => p !== preferred),
  ];
  const byPlacement = new Map(candidates.map((c) => [c.placement, c]));

  const fits = (top: number, left: number) =>
    top >= HELP_VIEW_PAD &&
    left >= HELP_VIEW_PAD &&
    top + bubble.height <= viewport.height - HELP_VIEW_PAD &&
    left + bubble.width <= viewport.width - HELP_VIEW_PAD;

  let chosen = byPlacement.get(preferred)!;
  for (const placement of order) {
    const c = byPlacement.get(placement);
    if (c && fits(c.top, c.left)) {
      chosen = c;
      break;
    }
  }

  const left = clamp(chosen.left, HELP_VIEW_PAD, Math.max(HELP_VIEW_PAD, maxLeft));
  const top = clamp(chosen.top, HELP_VIEW_PAD, Math.max(HELP_VIEW_PAD, maxTop));
  const alongEdge = chosen.placement === 'top' || chosen.placement === 'bottom'
    ? cx - left
    : cy - top;
  const edgeMax = chosen.placement === 'top' || chosen.placement === 'bottom'
    ? bubble.width
    : bubble.height;

  return {
    top,
    left,
    placement: chosen.placement,
    highlight,
    arrowOffset: clamp(alongEdge, HELP_ARROW_INSET, Math.max(HELP_ARROW_INSET, edgeMax - HELP_ARROW_INSET)),
  };
}
