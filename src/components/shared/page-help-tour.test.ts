import { describe, expect, it } from 'vitest';
import {
  HELP_BUBBLE_WIDTH,
  HELP_VIEW_PAD,
  capHighlight,
  dimPaneRects,
  dockOrigin,
  expandHighlight,
  filterVisibleHelpSteps,
  isPageHelpOpenKey,
  pageHelpKeyAction,
  pickHelpRect,
  placeHelpBubble,
  rectsOverlap,
  visibleOverlap,
} from './page-help-tour';

const viewport = { width: 1200, height: 800 };
const bubble = { width: HELP_BUBBLE_WIDTH, height: 120 };

describe('placeHelpBubble', () => {
  it('docks in the middle for a top-of-page target and does not cover it', () => {
    const target = { top: 120, left: 200, width: 240, height: 40 };
    const placed = placeHelpBubble({ target, viewport, bubble });
    const home = dockOrigin('center', viewport, bubble);
    expect(placed.dock).toBe('center');
    expect(placed.top).toBe(home.top);
    expect(placed.left).toBe(home.left);
    expect(placed.highlight).toEqual(capHighlight(expandHighlight(target)));
    expect(
      rectsOverlap(
        { top: placed.top, left: placed.left, width: bubble.width, height: bubble.height },
        placed.highlight!,
      ),
    ).toBe(false);
  });

  it('moves off the middle when that slot would cover the control', () => {
    const home = dockOrigin('center', viewport, bubble);
    const target = { top: home.top + 8, left: home.left + 40, width: 160, height: 36 };
    const placed = placeHelpBubble({ target, viewport, bubble });
    expect(placed.dock).not.toBe('center');
    expect(
      rectsOverlap(
        { top: placed.top, left: placed.left, width: bubble.width, height: bubble.height },
        capHighlight(expandHighlight(target)),
      ),
    ).toBe(false);
  });

  it('reuses the previous dock when it still leaves the control clear', () => {
    const target = { top: 200, left: 80, width: 180, height: 32 };
    const placed = placeHelpBubble({
      target,
      viewport,
      bubble,
      previousDock: 'bottom-left',
    });
    expect(placed.dock).toBe('bottom-left');
    expect(placed.left).toBe(HELP_VIEW_PAD);
  });

  it('drops the previous dock when it would cover the next control', () => {
    const home = dockOrigin('center', viewport, bubble);
    const target = { top: home.top + 8, left: home.left + 40, width: 160, height: 36 };
    const placed = placeHelpBubble({
      target,
      viewport,
      bubble,
      previousDock: 'center',
    });
    expect(placed.dock).not.toBe('center');
  });

  it('caps a tall list so the spotlight stays on the control', () => {
    const huge = expandHighlight({ top: 80, left: 20, width: 900, height: 600 });
    const capped = capHighlight(huge);
    expect(capped.height).toBeLessThan(huge.height);
    expect(capped.width).toBeLessThan(huge.width);
    expect(capped.top).toBe(huge.top);
  });

  it('keeps the bubble inside the viewport without a target', () => {
    const placed = placeHelpBubble({ target: null, viewport, bubble });
    expect(placed.highlight).toBeNull();
    expect(placed.dock).toBe('center');
    expect(placed.left + bubble.width).toBeLessThanOrEqual(viewport.width - HELP_VIEW_PAD + 0.01);
    expect(placed.top).toBeGreaterThanOrEqual(HELP_VIEW_PAD);
  });
});

describe('dimPaneRects', () => {
  it('leaves a click-through hole around the highlight', () => {
    const panes = dimPaneRects(
      { top: 100, left: 50, width: 200, height: 40 },
      { width: 1000, height: 800 },
    );
    expect(panes).toHaveLength(4);
    expect(panes.some((pane) => pane.top === 0 && pane.height === 100)).toBe(true);
    expect(panes.every((pane) => pane.width > 0 && pane.height > 0)).toBe(true);
  });

  it('covers the viewport when there is no hole', () => {
    expect(dimPaneRects(null, { width: 800, height: 600 })).toEqual([
      { top: 0, left: 0, width: 800, height: 600 },
    ]);
  });
});

describe('visibleOverlap', () => {
  it('clips a control that sits partly below the fold', () => {
    expect(visibleOverlap({ top: 760, left: 40, width: 200, height: 80 }, viewport)).toEqual({
      top: 760,
      left: 40,
      width: 200,
      height: 40,
    });
  });

  it('returns null when the control is fully off-screen', () => {
    expect(visibleOverlap({ top: 900, left: 40, width: 200, height: 40 }, viewport)).toBeNull();
  });
});

describe('pickHelpRect', () => {
  it('prefers the control that is on screen when data has many rows', () => {
    const off = { top: 1200, left: 20, width: 240, height: 36 };
    const on = { top: 180, left: 20, width: 240, height: 36 };
    expect(pickHelpRect([off, on], viewport)).toEqual(on);
  });

  it('falls back to the first sized control when none are on screen', () => {
    const first = { top: 1200, left: 20, width: 240, height: 36 };
    const second = { top: 1240, left: 20, width: 240, height: 36 };
    expect(pickHelpRect([first, second], viewport)).toEqual(first);
  });
});

describe('isPageHelpOpenKey', () => {
  it('maps F1 to open the current-page tutorial', () => {
    expect(isPageHelpOpenKey({ key: 'F1' })).toBe(true);
  });

  it('ignores modified or already-handled keys', () => {
    expect(isPageHelpOpenKey({ key: 'F1', ctrlKey: true })).toBe(false);
    expect(isPageHelpOpenKey({ key: 'F1', metaKey: true })).toBe(false);
    expect(isPageHelpOpenKey({ key: 'F1', altKey: true })).toBe(false);
    expect(isPageHelpOpenKey({ key: 'F1', defaultPrevented: true })).toBe(false);
    expect(isPageHelpOpenKey({ key: 'Escape' })).toBe(false);
  });
});

describe('pageHelpKeyAction', () => {
  it('maps arrows and enter to back, next, and skip', () => {
    expect(pageHelpKeyAction({ key: 'ArrowLeft' })).toBe('back');
    expect(pageHelpKeyAction({ key: 'ArrowUp' })).toBe('back');
    expect(pageHelpKeyAction({ key: 'ArrowRight' })).toBe('next');
    expect(pageHelpKeyAction({ key: 'ArrowDown' })).toBe('next');
    expect(pageHelpKeyAction({ key: 'Enter' })).toBe('next');
    expect(pageHelpKeyAction({ key: 'Escape' })).toBe('skip');
  });

  it('does not steal keys while typing or when a button already handles Enter', () => {
    expect(pageHelpKeyAction({ key: 'ArrowLeft' }, { tagName: 'INPUT' })).toBeNull();
    expect(pageHelpKeyAction({ key: 'Enter' }, { tagName: 'BUTTON' })).toBeNull();
    expect(pageHelpKeyAction({ key: 'ArrowRight', metaKey: true })).toBeNull();
    expect(pageHelpKeyAction({ key: 'Escape' }, { tagName: 'INPUT' })).toBe('skip');
  });
});

describe('filterVisibleHelpSteps', () => {
  it('keeps only steps whose control is on the page', () => {
    expect(filterVisibleHelpSteps([true, false, true])).toEqual([0, 2]);
  });

  it('falls back to the first step when nothing is mounted', () => {
    expect(filterVisibleHelpSteps([false, false, false])).toEqual([0]);
  });
});
