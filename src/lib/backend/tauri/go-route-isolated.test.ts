import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createTauriGoRouteIsolatedPort } from './go-route-isolated';

const invokeMock = vi.fn();
vi.mock('./invoke', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

beforeEach(() => invokeMock.mockReset());

describe('Tauri isolated Go route port', () => {
  it('uses the dedicated lifecycle commands without arguments', async () => {
    const stopped = {
      state: 'stopped' as const,
      listenReady: false,
      port: null,
      lastError: null,
      home: null,
    };
    const ready = {
      state: 'ready' as const,
      listenReady: true,
      port: 18765,
      lastError: null,
      home: '/tmp/agenthub-go-route-isolated/test',
    };
    invokeMock
      .mockResolvedValueOnce(ready)
      .mockResolvedValueOnce(stopped)
      .mockResolvedValueOnce(stopped);
    const port = createTauriGoRouteIsolatedPort();

    await expect(port.start()).resolves.toEqual(ready);
    await expect(port.stop()).resolves.toEqual(stopped);
    await expect(port.status()).resolves.toEqual(stopped);

    expect(invokeMock.mock.calls).toEqual([
      ['start_go_route_isolated'],
      ['stop_go_route_isolated'],
      ['get_go_route_isolated_status'],
    ]);
  });
});
