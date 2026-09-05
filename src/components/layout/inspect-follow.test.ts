import { describe, expect, it, vi } from 'vitest';
import { followInspectOpen } from './inspect-follow';

describe('followInspectOpen', () => {
  it('returns the opener only while inspect is expanded', () => {
    const open = vi.fn();
    expect(followInspectOpen(false, open)).toBeUndefined();
    expect(followInspectOpen(true, open)).toBe(open);
    followInspectOpen(true, open)?.();
    expect(open).toHaveBeenCalledTimes(1);
  });
});
