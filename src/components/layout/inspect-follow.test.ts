import { describe, expect, it, vi } from 'vitest';
import { followInspectOpen } from './inspect-follow';

describe('followInspectOpen', () => {
  it('opens detail from a row click even when the pane is closed', () => {
    const open = vi.fn();
    expect(followInspectOpen(false, open)).toBe(open);
    expect(followInspectOpen(true, open)).toBe(open);
    followInspectOpen(false, open)();
    expect(open).toHaveBeenCalledTimes(1);
  });
});
