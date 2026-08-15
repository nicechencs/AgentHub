import { describe, expect, it, vi } from 'vitest';
import { createMockSkillPort } from './skill';

describe('mock SkillPort.onFsChanged', () => {
  it('is a no-op and never invokes the handler', async () => {
    const port = createMockSkillPort();
    const handler = vi.fn();
    const unsub = await port.onFsChanged(handler);
    expect(typeof unsub).toBe('function');
    expect(handler).not.toHaveBeenCalled();
    unsub();
    expect(handler).not.toHaveBeenCalled();
  });
});
