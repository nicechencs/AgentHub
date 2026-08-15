import { describe, expect, it, vi } from 'vitest';
import { createMockInstallPort } from './install';

describe('mock InstallPort.onProgress', () => {
  it('is a no-op and never invokes the handler', async () => {
    const port = createMockInstallPort();
    const handler = vi.fn();
    const unsub = await port.onProgress(handler);
    expect(typeof unsub).toBe('function');
    expect(handler).not.toHaveBeenCalled();
    unsub();
    expect(handler).not.toHaveBeenCalled();
  });
});
