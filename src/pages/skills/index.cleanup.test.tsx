import { describe, expect, it, vi } from 'vitest';
import { createIdempotentCleanup } from './index';

describe('skills preview drag cleanup', () => {
  it('does not double-restore when pointercancel races with unmount', () => {
    const restore = vi.fn();
    const cleanup = createIdempotentCleanup((reason: string) => restore(reason));

    cleanup('pointercancel');
    cleanup('blur');
    cleanup('unmount');

    expect(restore).toHaveBeenCalledOnce();
    expect(restore).toHaveBeenCalledWith('pointercancel');
  });
});
