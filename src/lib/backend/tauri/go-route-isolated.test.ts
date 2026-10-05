import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  type GoRouteIsolatedStatus,
  shouldApplyGoRouteResult,
} from '../contracts/go-route-isolated';
import { createTauriGoRouteIsolatedPort } from './go-route-isolated';

const invokeMock = vi.fn();
vi.mock('./invoke', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

beforeEach(() => invokeMock.mockReset());

describe('Tauri isolated Go route port', () => {
  it('rejects stale or paused status results', () => {
    expect(shouldApplyGoRouteResult(4, 4, false)).toBe(true);
    expect(shouldApplyGoRouteResult(3, 4, false)).toBe(false);
    expect(shouldApplyGoRouteResult(4, 4, true)).toBe(false);
  });

  it('uses the dedicated lifecycle commands without arguments', async () => {
    const stopped: GoRouteIsolatedStatus = {
      state: 'stopped' as const,
      listenReady: false,
      port: null,
      lastError: null,
      home: null,
      lifecycle: 'stopped',
      inFlightCount: 0,
      memberCount: 0,
      healthyMemberCount: 0,
      edgeStatuses: [],
      recovering: false,
      restartCount: 0,
    };
    const ready: GoRouteIsolatedStatus = {
      state: 'ready' as const,
      listenReady: true,
      port: 18765,
      lastError: null,
      home: '/tmp/agenthub-go-route-isolated/test',
      lifecycle: 'serving',
      inFlightCount: 2,
      memberCount: 3,
      healthyMemberCount: 2,
      edgeStatuses: [{
        poolId: 'pool-codex',
        surface: 'responses',
        memberCount: 3,
        healthyMemberCount: 2,
        inFlightCount: 2,
        requestSuccessCount: 8,
        requestFailureCount: 1,
        lastErrorCode: 'upstream_unavailable',
      }],
      recovering: false,
      restartCount: 1,
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

  it('normalizes a pre-edge-status supervisor response to an empty list', async () => {
    const legacy = {
      state: 'ready' as const,
      listenReady: true,
      port: 18765,
      lastError: null,
      home: '/tmp/agenthub-go-route-isolated/test',
      lifecycle: 'serving',
      inFlightCount: 0,
      memberCount: 1,
      healthyMemberCount: 1,
      recovering: false,
      restartCount: 0,
    };
    invokeMock.mockResolvedValueOnce(legacy);

    await expect(createTauriGoRouteIsolatedPort().status()).resolves.toEqual({
      ...legacy,
      edgeStatuses: [],
    });
  });
});
