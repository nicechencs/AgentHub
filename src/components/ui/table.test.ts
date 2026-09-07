import { describe, expect, it, vi } from 'vitest';
import { createIdempotentCleanup, tableStyles } from './table';

describe('table row dividers', () => {
  it('keeps the top border on the last body row', () => {
    expect(tableStyles.tr).toContain('border-t');
    expect(tableStyles.tr).not.toContain('last:border-0');
    expect(tableStyles.trWorkbench).toContain('border-t');
    expect(tableStyles.trWorkbench).not.toContain('last:border-0');
  });
});

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
