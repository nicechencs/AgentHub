import { describe, expect, it, vi } from 'vitest';
import type { Backend } from '@/lib/backend/contracts';
import { createTauriDashboardPort } from './dashboard';

describe('tauri dashboard alerts', () => {
  it('propagates agent load failure instead of fabricating an empty alert list', async () => {
    const error = new Error('backend unavailable');
    const backend = {
      agent: { listAgents: vi.fn().mockRejectedValue(error) },
    } as unknown as Backend;
    const port = createTauriDashboardPort(backend);

    await expect(port.listAlerts()).rejects.toBe(error);
  });
});
