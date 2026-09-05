import { describe, expect, it } from 'vitest';
import {
  HELP_BUBBLE_GAP,
  HELP_BUBBLE_WIDTH,
  HELP_VIEW_PAD,
  capHighlight,
  dimPaneRects,
  expandHighlight,
  filterVisibleHelpSteps,
  placeHelpBubble,
} from './page-help-tour';

const viewport = { width: 1200, height: 800 };
const bubble = { width: HELP_BUBBLE_WIDTH, height: 120 };

describe('placeHelpBubble', () => {
  it('puts the bubble below a mid-page target and aims the arrow at it', () => {
    const target = { top: 120, left: 200, width: 240, height: 40 };
    const placed = placeHelpBubble({ target, viewport, bubble, preferred: 'bottom' });
    expect(placed.placement).toBe('bottom');
    const highlight = capHighlight(expandHighlight(target));
    expect(placed.top).toBe(highlight.top + highlight.height + HELP_BUBBLE_GAP);
    expect(placed.highlight).toEqual(highlight);
    expect(placed.arrowOffset).toBeGreaterThan(0);
    expect(placed.left).toBeGreaterThanOrEqual(HELP_VIEW_PAD);
  });

  it('flips above when there is no room below', () => {
    const target = { top: 700, left: 200, width: 240, height: 40 };
    const placed = placeHelpBubble({ target, viewport, bubble, preferred: 'bottom' });
    expect(placed.placement).toBe('top');
    expect(placed.top + bubble.height).toBeLessThanOrEqual(target.top);
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

describe('filterVisibleHelpSteps', () => {
  it('keeps only steps whose control is on the page', () => {
    expect(filterVisibleHelpSteps([true, false, true])).toEqual([0, 2]);
  });

  it('falls back to the first step when nothing is mounted', () => {
    expect(filterVisibleHelpSteps([false, false, false])).toEqual([0]);
  });
});
