import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { AdapterBridgeRuntimeStatus } from '@/lib/backend/contracts/adapter';
import { AdapterProfiles } from './adapter-components';
import { AdapterProfileDetailDialog } from './AdapterProfileDetailDialog';
import {
  ADAPTER_BRIDGE_STATUS_POLL_MS,
  BRIDGES_EMPTY_DESCRIPTION,
  BRIDGES_EMPTY_TITLE,
  adapterBridgeProfilesToPoll,
  adapterPageDescription,
  applyAdapterBridgeStatusPoll,
  legacyBridgesRedirectTo,
  loadAdapterPageResources,
  loadAdapterProfileResources,
  mergeAdapterProfileLoad,
  resolveBridgesProfileQuery,
  shouldPollAdapterBridgeStatus,
} from './adapter-model';
import { startAdapterBridgeStatusPoll } from './use-bridge-resources';
import type { Account, Provider } from '@/lib/types';

function localBridgeProfile(id = 'bridge-1') {
  return {
    id,
    name: 'Kimi → Codex',
    sourceKind: 'provider' as const,
    sourceId: 'kimi-1',
    targetAgentId: 'codex' as const,
    route: 'local_bridge' as const,
    mode: 'api' as const,
    status: 'active' as const,
    ruleId: 'bridge',
    ruleVersion: '1',
    generatedProviderId: 'codex-bridge-1',
    localPort: 43121,
    autoStart: true,
    createdAt: '2026-08-12T00:00:00Z',
    updatedAt: '2026-08-12T00:00:00Z',
  };
}

function runningStatus(profileId: string): AdapterBridgeRuntimeStatus {
  return {
    profileId,
    state: 'running',
    port: 43121,
    endpoint: 'http://127.0.0.1:43121',
    startedAt: '2026-08-12T00:00:00Z',
    upstreamStatus: 'connected',
  };
}

const emptyListProps = {
  bridgeStatuses: {},
  statusErrors: {},
  entries: [],
  loading: false,
  loadError: null,
  errors: {},
  removingProfileId: null,
  busyProfileIds: {},
  onStartBridge: vi.fn(),
  onRequestStopBridge: vi.fn(),
  onShowDetail: vi.fn(),
  onRetry: vi.fn(),
};

describe('Bridges page', () => {
  it('describes the page as local-bridge runtime ops', () => {
    expect(adapterPageDescription()).toBe('本机协议转换 · 仅 127.0.0.1');
  });

  it('rewrites /adapter and /router bookmarks onto /bridges and drops ?tab=', () => {
    expect(legacyBridgesRedirectTo('')).toBe('/bridges');
    expect(legacyBridgesRedirectTo('?tab=oauth')).toBe('/bridges');
    expect(legacyBridgesRedirectTo('?tab=api&profile=bridge-1')).toBe('/bridges?profile=bridge-1');
    expect(legacyBridgesRedirectTo('?profile=bridge-1')).toBe('/bridges?profile=bridge-1');
  });

  it('opens ?profile= only when the runtime exists', () => {
    expect(resolveBridgesProfileQuery('bridge-1', [{ id: 'bridge-1' }])).toBe('bridge-1');
    expect(resolveBridgesProfileQuery('missing', [{ id: 'bridge-1' }])).toBeNull();
    expect(resolveBridgesProfileQuery(null, [{ id: 'bridge-1' }])).toBeNull();
  });

  it('renders a healthy empty list without leaving-the-page CTAs', () => {
    const markup = renderToStaticMarkup(
      createElement(AdapterProfiles, {
        ...emptyListProps,
        profiles: [],
      }),
    );
    expect(markup).toContain(BRIDGES_EMPTY_TITLE);
    expect(markup).toContain(BRIDGES_EMPTY_DESCRIPTION);
    expect(markup).not.toContain('去 Dashboard');
    expect(markup).not.toContain('去 Connections');
    expect(markup).not.toContain('没有已绑定的本机桥');
  });

  it('renders a running bridge as single-layer health plus port', () => {
    const profile = localBridgeProfile();
    const markup = renderToStaticMarkup(
      createElement(AdapterProfiles, {
        ...emptyListProps,
        profiles: [profile],
        bridgeStatuses: { [profile.id]: runningStatus(profile.id) },
      }),
    );
    expect(markup).toContain('运行中');
    expect(markup).toContain('127.0.0.1:43121');
    expect(markup).toContain('停止');
    expect(markup).not.toContain('配置已生效');
    expect(markup).not.toContain('桥接运行中');
    expect(markup).not.toContain('本地协议转换');
  });

  it('keeps last-known running as 状态不可用 + 停止 when status read fails', () => {
    const profile = localBridgeProfile();
    const markup = renderToStaticMarkup(
      createElement(AdapterProfiles, {
        ...emptyListProps,
        profiles: [profile],
        bridgeStatuses: { [profile.id]: runningStatus(profile.id) },
        statusErrors: { [profile.id]: new Error('status read failed') },
      }),
    );
    expect(markup).toContain('状态不可用');
    expect(markup).toContain('停止');
    expect(markup).not.toContain('启动失败');
    expect(markup).not.toContain('重试启动');
  });

  it('renders detail as single-layer runtime without a Connections projection link', () => {
    const profile = localBridgeProfile();
    const markup = renderToStaticMarkup(
      createElement(AdapterProfileDetailDialog, {
        profile,
        bridgeStatus: runningStatus(profile.id),
        statusUnavailable: false,
        entries: [],
        busy: false,
        error: null,
        onClose: vi.fn(),
        onSetAutoStart: vi.fn(),
        onRequestRemove: vi.fn(),
      }),
    );
    expect(markup).toContain('运行中');
    expect(markup).toContain('本机端点');
    expect(markup).toContain('目标写入');
    expect(markup).toContain('解除绑定');
    expect(markup).not.toContain('配置已生效');
    expect(markup).not.toContain('在 Connections 查看');
    expect(markup).not.toContain('删除适配');
  });

  it('preserves successful resources when another pool or bridge status fails', async () => {
    const profile = localBridgeProfile();
    const account: Account = {
      id: 'account-1234', agentId: 'claude', kind: 'apikey', label: 'Work key', isCurrent: true, tokenValid: true,
    };
    const result = await loadAdapterPageResources({
      listAccounts: async () => [account],
      listProviders: async () => Promise.reject(new Error('provider unavailable')) as Promise<Provider[]>,
      listProfiles: async () => [profile],
      getBridgeStatus: async () => Promise.reject(new Error('bridge unavailable')),
    });

    expect(result.connectionState).toBe('partial');
    expect(result.entries).toHaveLength(1);
    expect(result.profiles).toEqual([profile]);
    expect(result.errors.providers).toBeInstanceOf(Error);
    expect(result.errors.bridgeStatuses[profile.id]).toBeInstanceOf(Error);
    expect(result.bridgeStatuses[profile.id]).toMatchObject({ state: 'error', upstreamStatus: 'unavailable' });
  });

  it('reports a failed profile request instead of treating it as an empty profile list', async () => {
    const result = await loadAdapterPageResources({
      listAccounts: async () => [],
      listProviders: async () => [],
      listProfiles: async () => Promise.reject(new Error('profiles unavailable')) as Promise<never[]>,
      getBridgeStatus: async () => ({ profileId: 'unused', state: 'stopped' }),
    });

    expect(result.profileState).toBe('error');
    expect(result.errors.profiles).toBeInstanceOf(Error);
    expect(result.profiles).toEqual([]);
  });

  it('keeps the last successful profiles when a later listProfiles call fails', async () => {
    const profile = localBridgeProfile('adapter-1');
    const previous = await loadAdapterProfileResources({
      listProfiles: async () => [profile],
      getBridgeStatus: async () => ({ profileId: 'unused', state: 'stopped' }),
    });
    const failed = await loadAdapterProfileResources({
      listProfiles: async () => Promise.reject(new Error('profiles unavailable')) as Promise<never[]>,
      getBridgeStatus: async () => ({ profileId: 'unused', state: 'stopped' }),
    });
    const merged = mergeAdapterProfileLoad(previous, failed);

    expect(failed.profiles).toEqual([]);
    expect(merged.profiles).toEqual([profile]);
    expect(merged.profileState).toBe('error');
    expect(merged.profileError).toBeInstanceOf(Error);
  });

  it('polls only running or degraded local-bridge profiles and keeps last-known state', () => {
    const running = localBridgeProfile('bridge-running');
    const stopped = { ...running, id: 'bridge-stopped' };
    const native = { ...running, id: 'native-1', route: 'native_endpoint' as const, localPort: null };
    const statuses = {
      [running.id]: { profileId: running.id, state: 'running' as const, port: 32123, upstreamStatus: 'connected' as const },
      [stopped.id]: { profileId: stopped.id, state: 'stopped' as const, port: 32123, upstreamStatus: 'stopped' as const },
    };
    expect(shouldPollAdapterBridgeStatus(running, statuses[running.id])).toBe(true);
    expect(shouldPollAdapterBridgeStatus(running, { ...statuses[running.id], state: 'degraded' })).toBe(true);
    expect(shouldPollAdapterBridgeStatus(stopped, statuses[stopped.id])).toBe(false);
    expect(shouldPollAdapterBridgeStatus(native, statuses[running.id])).toBe(false);
    expect(adapterBridgeProfilesToPoll([running, stopped, native], statuses).map((item) => item.id)).toEqual([running.id]);
    expect(ADAPTER_BRIDGE_STATUS_POLL_MS).toBeGreaterThanOrEqual(3_000);
    expect(ADAPTER_BRIDGE_STATUS_POLL_MS).toBeLessThanOrEqual(5_000);

    const current = {
      entries: [],
      profiles: [running],
      bridgeStatuses: statuses,
      errors: { bridgeStatuses: {} },
      connectionState: 'ready' as const,
      profileState: 'ready' as const,
    };
    const next = applyAdapterBridgeStatusPoll(current, [running], [{
      status: 'fulfilled',
      value: { profileId: running.id, state: 'degraded', port: 32123, upstreamStatus: 'degraded' },
    }]);
    expect(next.bridgeStatuses[running.id]).toMatchObject({ state: 'degraded', upstreamStatus: 'degraded' });

    const failed = applyAdapterBridgeStatusPoll(current, [running], [{
      status: 'rejected',
      reason: new Error('status read failed'),
    }]);
    expect(failed.bridgeStatuses[running.id]).toMatchObject({
      state: 'running',
      port: 32123,
      upstreamStatus: 'unavailable',
    });
    expect(failed.errors.bridgeStatuses[running.id]).toBeInstanceOf(Error);

    let generation = 1;
    const setIntervalFn = vi.fn().mockReturnValue(77);
    const clearIntervalFn = vi.fn();
    const stop = startAdapterBridgeStatusPoll({
      getGeneration: () => generation,
      getResources: () => current,
      apply: vi.fn(),
      getBridgeStatus: async () => {
        throw new Error('poll must not run after dispose');
      },
      setIntervalFn: setIntervalFn as unknown as typeof setInterval,
      clearIntervalFn: clearIntervalFn as unknown as typeof clearInterval,
    });
    expect(setIntervalFn).toHaveBeenCalledWith(expect.any(Function), ADAPTER_BRIDGE_STATUS_POLL_MS);
    generation += 1;
    stop();
    expect(clearIntervalFn).toHaveBeenCalledWith(77);
  });
});
