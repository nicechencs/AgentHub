import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { AdapterCommandError } from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/pages/connections/connection-model';
import { AdapterPreviewResult, AdapterProfiles } from './adapter-components';
import { AdapterSourceList } from './AdapterSourceList';
import {
  adapterApplyStage,
  adapterApplyStageLabel,
  adapterBridgeProbeSummary,
  adapterFailurePresentation,
  adapterNeedsAttentionRecovery,
  adapterProfileLastErrorCode,
  adapterProfilePortLabel,
  adapterSourceCounts,
  excludeAdapterGeneratedSources,
  filterAdapterSourcesByCredential,
  filterAdapterSourcesByKind,
  groupAdapterSources,
  searchAdapterSources,
  isClearlyConfigurableAgent,
  isOAuthAuthIncomplete,
  oauthIncompleteAuthHint,
  selectableTargetAgentIds,
} from './adapter-sources';

function entry(partial: Partial<ConnectionEntry> & Pick<ConnectionEntry, 'key' | 'id' | 'agentId' | 'source'>): ConnectionEntry {
  return {
    kind: 'apikey',
    title: partial.title ?? partial.id,
    subtitle: '',
    isCurrent: false,
    authStatus: 'valid',
    sortKey: partial.id,
    ...partial,
  };
}

describe('adapter source grouping and target filter', () => {
  it('filters sources by API Key vs official login without using sourceKind', () => {
    const api = entry({ key: 'p:1', id: '1', agentId: 'kimi', source: 'provider', kind: 'apikey' });
    const oauth = entry({ key: 'a:2', id: '2', agentId: 'codex', source: 'account', kind: 'oauth' });
    const apiAccount = entry({ key: 'a:3', id: '3', agentId: 'claude', source: 'account', kind: 'apikey' });
    expect(filterAdapterSourcesByKind([api, oauth, apiAccount], 'apikey')).toEqual([api, apiAccount]);
    expect(filterAdapterSourcesByKind([api, oauth, apiAccount], 'oauth')).toEqual([oauth]);
    expect(filterAdapterSourcesByCredential([api, oauth, apiAccount], 'all')).toEqual([api, oauth, apiAccount]);
    expect(filterAdapterSourcesByCredential([api, oauth, apiAccount], 'api')).toEqual([api, apiAccount]);
    expect(adapterSourceCounts([api, oauth, apiAccount])).toEqual({ all: 3, api: 2, oauth: 1 });
    expect(searchAdapterSources([api, oauth, apiAccount], '官方登录')).toEqual([oauth]);
    expect(searchAdapterSources([api, oauth, apiAccount], 'kimi')).toEqual([api]);
  });

  it('falls back to AGENT_IDS only when detect status is unavailable', () => {
    expect(selectableTargetAgentIds({
      state: 'idle',
      statuses: [],
      fallbackIds: ['claude', 'codex', 'pi'],
    })).toEqual(['claude', 'codex', 'pi']);
    expect(selectableTargetAgentIds({
      state: 'error',
      statuses: [{ agentId: 'claude', installed: true }],
      fallbackIds: ['claude', 'codex'],
    })).toEqual(['claude', 'codex']);
  });

  it('keeps only installed or clearly configurable agents when status is ready', () => {
    expect(isClearlyConfigurableAgent({
      agentId: 'workbuddy',
      installed: false,
      capabilities: { configWrite: { level: 'full' } },
    })).toBe(true);
    expect(selectableTargetAgentIds({
      state: 'ready',
      statuses: [
        { agentId: 'claude', installed: true },
        { agentId: 'codex', installed: false },
        { agentId: 'pi', installed: false, capabilities: { accountSwitch: { level: 'partial' } } },
        { agentId: 'cursor', installed: false, capabilities: { configWrite: { level: 'unsupported' } } },
      ],
      fallbackIds: ['claude', 'codex', 'pi', 'cursor', 'kimi'],
    })).toEqual(['claude', 'pi']);
  });

  it('groups sources by Agent then Account/Provider', () => {
    const groups = groupAdapterSources([
      entry({ key: 'provider:kimi-1', id: 'kimi-1', agentId: 'kimi', source: 'provider', title: 'Kimi key' }),
      entry({ key: 'account:claude-1', id: 'claude-1', agentId: 'claude', source: 'account', kind: 'oauth', title: 'Claude login' }),
      entry({ key: 'provider:claude-2', id: 'claude-2', agentId: 'claude', source: 'provider', title: 'Claude key' }),
      entry({ key: 'account:claude-3', id: 'claude-3', agentId: 'claude', source: 'account', kind: 'oauth', title: 'Claude alt' }),
    ], ['claude', 'kimi']);

    expect(groups.map((group) => group.label)).toEqual([
      'Claude Code · 账户',
      'Claude Code · Provider',
      'Kimi · Provider',
    ]);
    expect(groups[0]?.entries.map((item) => item.id)).toEqual(['claude-1', 'claude-3']);
    expect(groups[1]?.entries.map((item) => item.id)).toEqual(['claude-2']);
  });

  it('treats incomplete OAuth as a Connections redirect, not an applyable source', () => {
    expect(isOAuthAuthIncomplete({ kind: 'oauth', authHealth: 'needs_login', authStatus: 'expired' })).toBe(true);
    expect(isOAuthAuthIncomplete({ kind: 'oauth', authHealth: 'missing', authStatus: 'none' })).toBe(true);
    expect(isOAuthAuthIncomplete({ kind: 'oauth', authStatus: 'expired' })).toBe(true);
    expect(isOAuthAuthIncomplete({ kind: 'oauth', authHealth: 'unknown', authStatus: 'expired' })).toBe(true);
    expect(isOAuthAuthIncomplete({ kind: 'oauth', authHealth: 'verified', authStatus: 'valid' })).toBe(false);
    expect(isOAuthAuthIncomplete({ kind: 'apikey', authHealth: 'needs_login', authStatus: 'expired' })).toBe(false);
    expect(oauthIncompleteAuthHint()).toContain('Connections');
    expect(oauthIncompleteAuthHint()).not.toMatch(/oauth apply|强制授权|假授权/i);
  });

  it('excludes Adapter-generated Provider sources', () => {
    const filtered = excludeAdapterGeneratedSources(
      [entry({ key: 'provider:generated-1', id: 'generated-1', agentId: 'codex', source: 'provider' })],
      [{ generatedProviderId: 'generated-1' }],
    );
    expect(filtered).toEqual([]);
  });

  it('keeps Providers that were not generated by Adapter', () => {
    const real = entry({ key: 'provider:kimi-1', id: 'kimi-1', agentId: 'kimi', source: 'provider' });
    expect(excludeAdapterGeneratedSources([real], [{ generatedProviderId: 'generated-1' }])).toEqual([real]);
    expect(excludeAdapterGeneratedSources([real], [{ generatedProviderId: null }])).toEqual([real]);
  });

  it('does not exclude account rows even when the id matches a generated Provider', () => {
    const account = entry({
      key: 'account:generated-1',
      id: 'generated-1',
      agentId: 'claude',
      source: 'account',
      kind: 'oauth',
    });
    expect(excludeAdapterGeneratedSources([account], [{ generatedProviderId: 'generated-1' }])).toEqual([account]);
  });
});

describe('adapter apply and profile recovery presentation', () => {
  it('surfaces applying/active stages and local-bridge probe text', () => {
    expect(adapterApplyStage({ applying: true })).toBe('applying');
    expect(adapterApplyStageLabel('applying')).toBe('应用中');
    expect(adapterApplyStage({ applying: false, successMessage: '适配已应用。', profileStatus: 'active' })).toBe('active');
    expect(adapterApplyStageLabel('active')).toBe('已生效');
    expect(adapterBridgeProbeSummary({
      profileId: 'bridge-1',
      state: 'running',
      port: 32123,
      upstreamStatus: 'connected',
    })).toContain('本地桥接检查');
    expect(adapterBridgeProbeSummary({
      profileId: 'bridge-1',
      state: 'running',
      port: 32123,
      upstreamStatus: 'connected',
    })).toContain('已连接');
  });

  it('classifies retryable versus non-retryable apply failures', () => {
    const retryable = adapterFailurePresentation(new AdapterCommandError({
      code: 'adapter.bridge_start',
      message: '桥接启动失败',
      retryable: true,
    }), '应用适配失败');
    const blocked = adapterFailurePresentation(new AdapterCommandError({
      code: 'adapter.unsupported',
      message: '当前路径不可应用',
      retryable: false,
    }), '应用适配失败');
    expect(retryable.retryable).toBe(true);
    expect(retryable.hint).toContain('可重试');
    expect(blocked.retryable).toBe(false);
    expect(blocked.hint).toContain('不可重试');
  });

  it('exposes port, lastErrorCode, and needs_attention recovery without auto-retry', () => {
    const recovery = adapterNeedsAttentionRecovery({
      status: 'needs_attention',
      route: 'local_bridge',
      lastErrorCode: 'adapter.bridge_start',
    }, 'error');
    expect(adapterProfilePortLabel({ localPort: 43121 })).toBe('127.0.0.1:43121');
    expect(adapterProfileLastErrorCode({ lastErrorCode: 'adapter.bridge_start' })).toBe('adapter.bridge_start');
    expect(recovery.startLabel).toBe('重试启动');
    expect(recovery.canDelete).toBe(true);
    expect(recovery.hint).toContain('不会自动反复重试');
  });

  it('renders a searchable source list with credential badges and no apply controls', () => {
    const oauth = entry({
      key: 'a:2',
      id: '2',
      agentId: 'codex',
      source: 'account',
      kind: 'oauth',
      title: 'Codex login',
      authHealth: 'needs_login',
      authStatus: 'expired',
    });
    const markup = renderToStaticMarkup(
      createElement(AdapterSourceList, {
        groups: [{ id: 'codex:account', label: 'Codex · 账户', entries: [oauth] }],
        selectedKey: oauth.key,
        filter: 'all',
        counts: { all: 1, api: 0, oauth: 1 },
        query: '',
        loading: false,
        loadError: null,
        totalCount: 1,
        visibleCount: 1,
        onSelect: vi.fn(),
        onFilterChange: vi.fn(),
        onQueryChange: vi.fn(),
        onRetry: vi.fn(),
        onGoConnections: vi.fn(),
      }),
    );
    expect(markup).toContain('可用连接');
    expect(markup).toContain('官方登录');
    expect(markup).toContain('授权未完成');
    expect(markup).toContain('全部');
    expect(markup).toContain('API Key');
    expect(markup).not.toContain('应用配置');
  });

  it('renders an OAuth auth hint and Connections link instead of apply', () => {
    const markup = renderToStaticMarkup(
      createElement(AdapterPreviewResult, {
        analysis: {
          route: 'native_endpoint',
          support: 'stable',
          reason: 'test route',
          actions: [],
          limitations: [],
          evidence: [],
        },
        plan: {
          analysis: {
            route: 'native_endpoint',
            support: 'stable',
            reason: 'test route',
            actions: [],
            limitations: [],
            evidence: [],
          },
          targetAgentId: 'claude',
          canApply: true,
          serviceImpact: 'none',
          changes: [],
        },
        loading: false,
        error: null,
        onRetry: vi.fn(),
        onApply: vi.fn(),
        authIncomplete: true,
        authHint: oauthIncompleteAuthHint(),
      }),
    );
    expect(markup).toContain('前往 Connections');
    expect(markup).toContain('#/connections');
    expect(markup).toContain('不会代替发起 OAuth');
    expect(markup).not.toContain('应用配置');
    expect(markup).not.toContain('启用本地桥接');
  });

  it('renders profile details for port, lastErrorCode, and recovery actions', () => {
    const markup = renderToStaticMarkup(
      createElement(AdapterProfiles, {
        profiles: [{
          id: 'bridge-1',
          name: 'Kimi → Codex',
          sourceKind: 'provider',
          sourceId: 'kimi-1',
          targetAgentId: 'codex',
          route: 'local_bridge',
          mode: 'api',
          status: 'needs_attention',
          ruleId: 'bridge',
          ruleVersion: '1',
          generatedProviderId: 'codex-bridge-1',
          localPort: 32123,
          autoStart: false,
          lastErrorCode: 'adapter.bridge_start',
          createdAt: '2026-08-12T00:00:00Z',
          updatedAt: '2026-08-12T00:00:00Z',
        }],
        bridgeStatuses: {
          'bridge-1': {
            profileId: 'bridge-1',
            state: 'error',
            port: 32123,
            upstreamStatus: 'unavailable',
          },
        },
        loading: false,
        loadError: null,
        errors: {},
        removingProfileId: null,
        busyProfileIds: {},
        onRemove: vi.fn(),
        onStartBridge: vi.fn(),
        onRequestStopBridge: vi.fn(),
        onSetBridgeAutoStart: vi.fn(),
        onRetry: vi.fn(),
      }),
    );
    expect(markup).toContain('端口：127.0.0.1:32123');
    expect(markup).toContain('lastErrorCode：adapter.bridge_start');
    expect(markup).toContain('凭据类型');
    expect(markup).toContain('API Key');
    expect(markup).toContain('本地协议转换');
    expect(markup).toContain('重试启动');
    expect(markup).toContain('删除');
    expect(markup).toContain('不会自动反复重试');
    expect(markup).not.toContain('代理路由');
  });
});
