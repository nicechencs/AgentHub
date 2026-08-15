import { describe, expect, it } from 'vitest';
import {
  actionCountClass,
  segmentedCountClass,
  segmentedItemClass,
  segmentedItemSizeClass,
  segmentedTrackClass,
} from '@/components/ui/segmented-styles';

describe('segmented-styles (shared track family)', () => {
  it('track class is stable for Tabs / Segmented / AgentTabStrip', () => {
    expect(segmentedTrackClass).toContain('bg-hover');
    expect(segmentedTrackClass).toContain('rounded-card');
  });

  it('sm/md size tokens stay distinct', () => {
    expect(segmentedItemSizeClass('sm')).toContain('text-xs');
    expect(segmentedItemSizeClass('md')).toContain('text-sm');
  });

  it('count badge is plain muted tabular nums', () => {
    expect(segmentedCountClass).toContain('tabular-nums');
    expect(segmentedCountClass).toContain('text-muted');
    expect(segmentedCountClass).not.toContain('rounded-full');
  });

  it('action count uses warning tokens (not raw amber)', () => {
    expect(actionCountClass).toContain('bg-warning/15');
    expect(actionCountClass).toContain('text-warning');
    expect(actionCountClass).not.toContain('amber-');
  });

  it('active item uses panel lift without accent fill', () => {
    const active = segmentedItemClass(true, 'md');
    const inactive = segmentedItemClass(false, 'sm');
    expect(active).toContain('bg-panel');
    expect(active).toContain('font-medium');
    expect(active).not.toContain('bg-accent');
    expect(inactive).toContain('text-secondary');
    expect(inactive).toContain('text-xs');
  });
});

/** Preview chrome may use tighter sm; page filters use SegmentedControl default sm + count. */
describe('segmented control usage contract (docs in segmented-styles)', () => {
  it('documents filter size as sm via SegmentedControl default', () => {
    // Contract: list filters → SegmentedControl (default sm); page nav → Tabs (md); agent → AgentTabStrip (md)
    expect(segmentedItemSizeClass('sm')).toBeTruthy();
    expect(segmentedItemSizeClass('md')).toBeTruthy();
  });
});
