import { describe, expect, it, vi } from 'vitest';
import { createIdempotentCleanup } from './table';

describe('table drag cleanup', () => {
  it('runs once when release, cancel, and unmount race', () => {
    const release = vi.fn();
    const cleanup = createIdempotentCleanup((reason: string) => release(reason));

    cleanup('pointerup');
    cleanup('pointercancel');
    cleanup('unmount');

    expect(release).toHaveBeenCalledOnce();
    expect(release).toHaveBeenCalledWith('pointerup');
  });
});
