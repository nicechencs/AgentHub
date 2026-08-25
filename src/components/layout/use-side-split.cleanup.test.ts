import { describe, expect, it, vi } from 'vitest';
import { createIdempotentCleanup } from './side-split-model';

describe('side-split drag cleanup', () => {
  it('does not double-restore when blur/cancel is followed by unmount', () => {
    const restore = vi.fn();
    const cleanup = createIdempotentCleanup((reason: string) => restore(reason));

    cleanup('blur');
    cleanup('pointercancel');
    cleanup('unmount');

    expect(restore).toHaveBeenCalledOnce();
    expect(restore).toHaveBeenCalledWith('blur');
  });
});
