import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AgentId } from '@/lib/types';
import {
  clearLiveAuthProbeCache,
  probeLiveAuthWithPort,
} from './live-auth-probe-cache';
import type { LiveAuthProbe } from './ports';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function probe(health: NonNullable<LiveAuthProbe['health']>): LiveAuthProbe {
  return {
    agentId: 'claude',
    kind: 'oauth',
    summary: health,
    hasCredentials: health !== 'missing',
    health,
    source: 'test-live',
    revision: health,
  };
}

describe('live auth probe cache', () => {
  beforeEach(() => clearLiveAuthProbeCache());

  it('allows a force probe to supersede an older pending request', async () => {
    const first = deferred<LiveAuthProbe>();
    const second = deferred<LiveAuthProbe>();
    const port = {
      probeLiveAuth: vi
        .fn()
        .mockImplementationOnce(() => first.promise)
        .mockImplementationOnce(() => second.promise),
    };

    const older = probeLiveAuthWithPort(port, 'claude');
    const forced = probeLiveAuthWithPort(port, 'claude', { force: true });

    second.resolve(probe('verified'));
    await expect(forced).resolves.toMatchObject({ health: 'verified' });
    first.resolve(probe('missing'));
    await expect(older).resolves.toMatchObject({ health: 'missing' });

    await expect(probeLiveAuthWithPort(port, 'claude')).resolves.toMatchObject({
      health: 'verified',
    });
    expect(port.probeLiveAuth).toHaveBeenCalledTimes(2);
  });

  it('increments generation on clear so a stale deferred result cannot repopulate cache', async () => {
    const stale = deferred<LiveAuthProbe>();
    const fresh = deferred<LiveAuthProbe>();
    const agentId: AgentId = 'claude';
    const port = {
      probeLiveAuth: vi
        .fn()
        .mockImplementationOnce(() => stale.promise)
        .mockImplementationOnce(() => fresh.promise),
    };

    const oldRequest = probeLiveAuthWithPort(port, agentId);
    clearLiveAuthProbeCache(agentId);
    stale.resolve(probe('missing'));
    await expect(oldRequest).resolves.toMatchObject({ health: 'missing' });

    const nextRequest = probeLiveAuthWithPort(port, agentId);
    fresh.resolve(probe('renewable'));
    await expect(nextRequest).resolves.toMatchObject({ health: 'renewable' });
    await expect(probeLiveAuthWithPort(port, agentId)).resolves.toMatchObject({
      health: 'renewable',
    });
    expect(port.probeLiveAuth).toHaveBeenCalledTimes(2);
  });
});
