import { describe, expect, it } from 'vitest';

import { usageTrendHoverPoint, usageTrendTipOffset } from './UsageTrendTooltip';

describe('usageTrendHoverPoint', () => {
  it('prefers the active coordinate, then chartX/Y', () => {
    expect(
      usageTrendHoverPoint({
        activeCoordinate: { x: 40, y: 80 },
        chartX: 1,
        chartY: 2,
      }),
    ).toEqual({ x: 40, y: 80 });
    expect(usageTrendHoverPoint({ chartX: 12, chartY: 24 })).toEqual({ x: 12, y: 24 });
    expect(usageTrendHoverPoint({ activeLabel: '2026-09-02' })).toBeNull();
    expect(usageTrendHoverPoint(null)).toBeNull();
  });
});

describe('usageTrendTipOffset', () => {
  it('keeps the full card width when hovering the right edge', () => {
    const pos = usageTrendTipOffset(390, 40, 400, 288, 280, 120);
    expect(pos.left + 280).toBeLessThanOrEqual(400);
    expect(pos.left).toBeGreaterThanOrEqual(4);
    expect(pos.left).toBeLessThan(120);
  });

  it('places the card to the right of the point when there is room', () => {
    const pos = usageTrendTipOffset(40, 40, 400, 288, 200, 80);
    expect(pos.left).toBe(52);
  });
});
