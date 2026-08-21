import { describe, expect, it } from 'vitest';
import type { AdapterBridgeRuntimeStatus, AdapterProfile } from '@/lib/backend/contracts/adapter';
import { startAdapterBridgeStatusPoll } from './use-bridge-resources';
import type { AdapterPageResources } from './adapter-resources';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

function resources(profile: AdapterProfile, status: AdapterBridgeRuntimeStatus): AdapterPageResources {
  return {
    entries: [],
    profiles: [profile],
    bridgeStatuses: { [profile.id]: status },
    errors: { bridgeStatuses: {} },
    connectionState: 'ready',
    profileState: 'ready',
  };
}

const profile: AdapterProfile = {
  id: 'profile-1',
  name: 'Profile 1',
  sourceKind: 'provider',
  sourceId: 'source-1',
  targetAgentId: 'claude',
  route: 'local_bridge',
  mode: 'oauth',
  status: 'active',
  ruleId: 'rule-1',
  ruleVersion: '1',
  localPort: 4310,
  autoStart: true,
  createdAt: 'now',
  updatedAt: 'now',
};

const running: AdapterBridgeRuntimeStatus = {
  profileId: profile.id,
  state: 'running',
  port: 4310,
  endpoint: 'http://127.0.0.1:4310',
  startedAt: 'now',
  upstreamStatus: 'ok',
};

describe('startAdapterBridgeStatusPoll', () => {
  it('does not overlap polls and ignores a response invalidated by a mutation', async () => {
    let tick!: () => void;
    let cleared = false;
    let generation = 0;
    let applyCount = 0;
    const nextStatus = deferred<AdapterBridgeRuntimeStatus>();
    let calls = 0;

    const stop = startAdapterBridgeStatusPoll({
      getGeneration: () => generation,
      getResources: () => resources(profile, running),
      apply: () => { applyCount += 1; },
      getBridgeStatus: async () => {
        calls += 1;
        return nextStatus.promise;
      },
      setIntervalFn: ((callback: () => void) => {
        tick = callback;
        return 1 as unknown as ReturnType<typeof setInterval>;
      }) as unknown as typeof setInterval,
      clearIntervalFn: (() => { cleared = true; }) as typeof clearInterval,
    });

    tick();
    tick();
    expect(calls).toBe(1);

    // A start/stop mutation advances the resource generation before its
    // authoritative response is applied.
    generation += 1;
    nextStatus.resolve({ ...running, state: 'stopped', upstreamStatus: 'stopped' });
    await Promise.resolve();
    await Promise.resolve();
    expect(applyCount).toBe(0);

    stop();
    expect(cleared).toBe(true);
  });

  it('invalidates an old poll when the hook is disposed', async () => {
    let tick!: () => void;
    let applyCount = 0;
    const nextStatus = deferred<AdapterBridgeRuntimeStatus>();
    const stop = startAdapterBridgeStatusPoll({
      getGeneration: () => 0,
      getResources: () => resources(profile, running),
      apply: () => { applyCount += 1; },
      getBridgeStatus: () => nextStatus.promise,
      setIntervalFn: ((callback: () => void) => {
        tick = callback;
        return 1 as unknown as ReturnType<typeof setInterval>;
      }) as unknown as typeof setInterval,
      clearIntervalFn: (() => undefined) as typeof clearInterval,
    });

    tick();
    stop();
    nextStatus.resolve(running);
    await Promise.resolve();
    await Promise.resolve();
    expect(applyCount).toBe(0);
  });
});
