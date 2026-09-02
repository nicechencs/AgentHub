import { createElement, type ComponentProps, type ReactNode } from 'react';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { AdapterBridgeRuntimeStatus } from '@/lib/backend/contracts/adapter';
import { AdapterProfiles } from './adapter-components';
import { RouteDetailPanel } from './RouteDetailPanel';
import { TooltipProvider } from '@/components/ui/tooltip';

vi.mock('@/components/ui/dialog', () => {
  const passthrough = ({ children }: { children?: ReactNode }) => children ?? null;
  return {
    Dialog: ({ open, children }: { open?: boolean; children?: ReactNode }) =>
      (open ? children : null),
    DialogContent: passthrough,
    DialogHeader: passthrough,
    DialogFooter: passthrough,
    DialogTitle: passthrough,
    DialogDescription: passthrough,
  };
});
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
  loadAdapterProfilesList,
  mergeAdapterProfileLoad,
  resolveBridgesProfileQuery,
  shouldPollAdapterBridgeStatus,
} from './adapter-model';
import { startAdapterBridgeStatusPoll } from './use-bridge-resources';
import type { ConnectionEntry } from '@/lib/connection-entry';
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

/** OpenRouter source declaring all three client endpoints on one base URL. */
function openRouterEntry(configText?: string): ConnectionEntry {
  return {
    key: 'provider:kimi-1',
    source: 'provider',
    kind: 'apikey',
    id: 'kimi-1',
    agentId: 'claude',
    title: 'OpenRouter',
    subtitle: '已配置',
    isCurrent: true,
    authStatus: 'valid',
    authHealth: 'configured',
    sortKey: '',
    provider: {
      id: 'kimi-1',
      agentId: 'claude',
      name: 'OpenRouter',
      preset: 'openrouter',
      configText: configText ?? JSON.stringify({
        vendor: 'openrouter',
        baseURL: 'https://openrouter.ai/api/v1',
        listedModels: ['stealth/ox-alpha'],
        endpoints: [
          { target: 'claude', enabled: true, url: 'https://openrouter.ai/api/v1' },
          { target: 'codex', enabled: true, url: 'https://openrouter.ai/api/v1' },
          { target: 'grok', enabled: true, url: 'https://openrouter.ai/api/v1' },
        ],
      }),
      configFormat: 'json',
      isCurrent: false,
      official: false,
    },
  };
}

type ProfilesProps = ComponentProps<typeof AdapterProfiles>;
type DetailProps = ComponentProps<typeof RouteDetailPanel>;

const emptyListProps: Omit<ProfilesProps, 'profiles'> = {
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

function renderProfiles(props: ProfilesProps) {
  return renderToStaticMarkup(
    createElement(TooltipProvider, null, createElement(AdapterProfiles, props)),
  );
}

function renderDetail(props: DetailProps) {
  return renderToStaticMarkup(
    createElement(TooltipProvider, null, createElement(RouteDetailPanel, props)),
  );
}

describe('Bridges page', () => {
  it('describes the page as local-bridge runtime ops', () => {
    expect(adapterPageDescription()).toBe('本机转发 · 仅 127.0.0.1 · 含端口');
  });

  it('rewrites /adapter, /router and /bridges bookmarks onto /routes and drops ?tab=', () => {
    expect(legacyBridgesRedirectTo('')).toBe('/routes/board');
    expect(legacyBridgesRedirectTo('?tab=oauth')).toBe('/routes/board');
    expect(legacyBridgesRedirectTo('?tab=api&profile=bridge-1')).toBe('/routes/pool?profile=bridge-1');
    expect(legacyBridgesRedirectTo('?profile=bridge-1')).toBe('/routes/pool?profile=bridge-1');
  });

  it('keeps the grouped card selected when inspect is on a sibling profile', () => {
    const claude = { ...localBridgeProfile('p-claude'), targetAgentId: 'claude' as const };
    const codex = { ...localBridgeProfile('p-codex'), targetAgentId: 'codex' as const };
    const markup = renderProfiles({
      ...emptyListProps,
      profiles: [codex],
      siblingProfiles: [claude, codex],
      activeProfileId: claude.id,
    });
    expect(markup).toContain('data-active="true"');
  });

  it('opens ?profile= only when the runtime exists', () => {
    expect(resolveBridgesProfileQuery('bridge-1', [{ id: 'bridge-1' }])).toBe('bridge-1');
    expect(resolveBridgesProfileQuery('missing', [{ id: 'bridge-1' }])).toBeNull();
    expect(resolveBridgesProfileQuery(null, [{ id: 'bridge-1' }])).toBeNull();
  });

  it('auth-pool workbench wires the profile deep-link helper', () => {
    const source = readFileSync(
      path.join(path.dirname(fileURLToPath(import.meta.url)), '../routes/pool/index.tsx'),
      'utf8',
    );
    expect(source).toContain('resolveBridgesProfileQuery');
    expect(source).toContain("searchParams.get('profile')");
    expect(source).toContain("inspect.open({ kind: 'detail', profile })");
  });

  it('renders a healthy empty list without leaving-the-page CTAs', () => {
    const markup = renderProfiles({
      ...emptyListProps,
      profiles: [],
    });
    expect(markup).toContain(BRIDGES_EMPTY_TITLE);
    expect(markup).toContain(BRIDGES_EMPTY_DESCRIPTION);
    expect(markup).not.toContain('去 Dashboard');
    expect(markup).not.toContain('去 Connections');
    expect(markup).not.toContain('没有已绑定的本机路由');
  });

  it('keeps 新建路由 copy for the create-route action', async () => {
    const { CREATE_ROUTE_TARGETS, canSubmitCreateRoute } = await import('./create-route-flow');
    expect(CREATE_ROUTE_TARGETS).toEqual(['claude', 'codex', 'grok']);
    expect(canSubmitCreateRoute({
      name: 'OpenRouter',
      url: 'https://openrouter.ai/api/v1',
      key: 'test-key',
      vendor: 'openrouter',
      endpoints: ['claude'],
    })).toBe(true);
  });

  it('keeps a hidden-target profile stop-only', () => {
    const profile = localBridgeProfile();
    const markup = renderProfiles({
      ...emptyListProps,
      profiles: [profile],
      bridgeStatuses: { [profile.id]: runningStatus(profile.id) },
      hiddenTargetIds: new Set([profile.targetAgentId]),
    });
    expect(markup).toContain('目标已隐藏，仅可停止');
    expect(markup).toContain('停止');
  });

  it('renders a running bridge as single-layer health plus port', () => {
    const profile = localBridgeProfile();
    const markup = renderProfiles({
      ...emptyListProps,
      profiles: [profile],
      bridgeStatuses: { [profile.id]: runningStatus(profile.id) },
      onRequestEdit: vi.fn(),
    });
    expect(markup).toContain('运行中');
    expect(markup).toContain('127.0.0.1:43121');
    expect(markup).toContain('停止');
    expect(markup).toContain('编辑');
    expect(markup).not.toContain('随 AgentHub 自动启动');
    expect(markup).not.toContain('仅在 AgentHub 运行时恢复，不是开机自启');
    expect(markup).toContain('详情');
    expect(markup).not.toContain('aria-expanded');
    expect(markup).not.toContain('收起');
    expect(markup).not.toContain('data-route-detail="bridge-1"');
    expect(markup).not.toContain('上游和本机');
    expect(markup).toContain('详情');
    expect(markup).not.toContain('本机桥');
    expect(markup).not.toContain('客户端接入');
    expect(markup).not.toContain('目标写入');
    expect(markup).not.toContain('配置已生效');
    expect(markup).not.toContain('role="dialog"');
  });

  it('keeps detail off the list and marks the row when the inspect pane is open', () => {
    const profile = localBridgeProfile();
    const markup = renderProfiles({
      ...emptyListProps,
      profiles: [profile],
      bridgeStatuses: { [profile.id]: runningStatus(profile.id) },
      activeProfileId: profile.id,
    });
    expect(markup).not.toContain('aria-expanded');
    expect(markup).toContain('data-active="true"');
    expect(markup).not.toContain('data-route-detail="bridge-1"');
    expect(markup).not.toContain('上游和本机');
  });

  it('shows a reorder handle when there are two routes and onMove is provided', () => {
    const first = localBridgeProfile('bridge-1');
    const second = localBridgeProfile('bridge-2');
    const markup = renderProfiles({
      ...emptyListProps,
      profiles: [first, second],
      bridgeStatuses: {
        [first.id]: runningStatus(first.id),
        [second.id]: runningStatus(second.id),
      },
      onMove: vi.fn(),
    });
    expect(markup).toContain('拖动排序');
    expect(markup).toContain('data-sortable-id');
  });

  it('shows the upstream → loopback flow and the clients one OpenRouter route serves', () => {
    const profile = localBridgeProfile();
    const markup = renderProfiles({
      ...emptyListProps,
      profiles: [{ ...profile, targetAgentId: 'claude', ruleId: 'openai-api-to-claude-v1' }],
      bridgeStatuses: { [profile.id]: runningStatus(profile.id) },
      entries: [openRouterEntry()],
      onRequestEdit: vi.fn(),
    });
    expect(markup).toContain('https://openrouter.ai/api/v1');
    expect(markup).toContain('http://127.0.0.1:43121');
    expect(markup).toContain('支持');
    expect(markup).toContain('编辑');
    expect(markup).toContain('Claude');
    expect(markup).toContain('Codex');
    expect(markup).toContain('Grok');
    expect(markup).toContain('/v1/messages');
    expect(markup).toContain('/v1/responses');
    expect(markup).toContain('GET /models');
    expect(markup).toContain('写入客户端');
    expect(markup).not.toContain('一键配置');
    expect(markup).not.toContain('将勾选项写入客户端配置');
    expect(markup).not.toContain('备选');
    expect(markup).not.toContain('同一类登录');
    expect(markup).not.toContain('票');
    expect(markup).not.toContain('钱包');
    expect(markup).not.toContain('投影');
    expect(markup).not.toContain('协议桥');
    expect(markup).not.toContain('data-route-detail="bridge-1"');
  });

  it('keeps last-known running as 状态不可用 + 停止 when status read fails', () => {
    const profile = localBridgeProfile();
    const markup = renderProfiles({
      ...emptyListProps,
      profiles: [profile],
      bridgeStatuses: { [profile.id]: runningStatus(profile.id) },
      statusErrors: { [profile.id]: new Error('status read failed') },
    });
    expect(markup).toContain('状态不可用');
    expect(markup).toContain('停止');
    expect(markup).not.toContain('启动失败');
    expect(markup).not.toContain('重试启动');
  });

  it('lists who this route connects, without a conversion table', () => {
    const profile = localBridgeProfile();
    const markup = renderDetail({
      profile: { ...profile, targetAgentId: 'claude', ruleId: 'openai-api-to-claude-v1' },
      bridgeStatus: runningStatus(profile.id),
      entries: [openRouterEntry()],
      busy: false,
      error: null,
      onRequestRemove: vi.fn(),
    });
    expect(markup).toContain('上游和本机');
    expect(markup).toContain('接到');
    expect(markup).toContain('上游');
    expect(markup).toContain('本机入口');
    expect(markup).toContain('https://openrouter.ai/api/v1');
    expect(markup).toContain('http://127.0.0.1:43121');
    expect(markup).toContain('Claude');
    expect(markup).toContain('Codex');
    expect(markup).toContain('Grok');
    expect(markup).toContain('/v1/messages');
    expect(markup).toContain('/v1/responses');
    expect(markup).toContain('/v1/chat/completions');
    expect(markup).toContain('Claude 对话');
    expect(markup).toContain('Codex / Grok 对话');
    expect(markup).toContain('Kimi 等补全');
    expect(markup).toContain('GET /models');
    expect(markup).toContain('模型名单');
    expect(markup).toContain('还没有工具连上');
    expect(markup).toContain('复制本机端点 http://127.0.0.1:43121/v1/messages');
    expect(markup).toContain('仅放行：stealth/ox-alpha（其余模型将被拒绝）');
    // #217 added a 转换 stage label in the route-trace legend; assert no hop-link conversion TABLE.
    expect(markup).toContain('转换');
    expect(markup).toContain('data-stage-box="conversion"');
    expect(markup).not.toContain('data-hop-link');
    expect(markup).not.toContain('OpenAI Responses');
    expect(markup).not.toContain('写 settings.json');
    expect(markup).not.toContain('Anthropic Messages');
  });

  it('still lists the local Claude address when only that client is open', () => {
    const profile = localBridgeProfile();
    const entry = openRouterEntry(JSON.stringify({
      baseURL: 'https://open.bigmodel.cn/api/coding/paas/v4',
      endpoints: [
        { target: 'claude', enabled: true, url: 'https://open.bigmodel.cn/api/anthropic' },
      ],
    }));
    const markup = renderDetail({
      profile: { ...profile, targetAgentId: 'claude', ruleId: 'openai-api-to-claude-v1' },
      bridgeStatus: runningStatus(profile.id),
      entries: [entry],
      busy: false,
      error: null,
      onRequestRemove: vi.fn(),
    });
    expect(markup).toContain('接到');
    expect(markup).toContain('Claude');
    expect(markup).toContain('https://open.bigmodel.cn/api/coding/paas/v4');
    expect(markup).toContain('复制本机端点 http://127.0.0.1:43121/v1/messages');
    expect(markup).toContain('未开放');
    expect(markup).not.toContain('直通');
    expect(markup).not.toContain('data-hop-link');
  });

  it('renders detail as single-layer runtime without a Connections projection link', () => {
    const profile = localBridgeProfile();
    const markup = renderDetail({
      profile,
      bridgeStatus: runningStatus(profile.id),
      entries: [],
      busy: false,
      error: null,
      onRequestRemove: vi.fn(),
    });
    expect(markup).not.toContain('编辑');
    expect(markup).toContain('删除路由');
    expect(markup).not.toContain('收起');
    expect(markup).toContain('data-route-detail="bridge-1"');
    expect(markup).toContain('来源登录已删除，路由仅可查看或解除绑定');
    expect(markup).not.toContain('客户端接入');
    expect(markup).not.toContain('将勾选项写入客户端配置');
    expect(markup).not.toContain('同一类登录');
    expect(markup).not.toContain('目标写入');
    expect(markup).not.toContain('在 Connections 查看');
    expect(markup).not.toContain('删除适配');
    expect(markup).not.toContain('role="dialog"');
    expect(markup).not.toContain('运行中');
    expect(markup).toContain('本机入口');
    expect(markup).not.toContain('交给本机网关');
    expect(markup).not.toContain('已接入的登录');
    expect(markup).not.toContain('入口 Key 已保存');
  });

  it('shows default pool members when flag is on, and hides them when off', () => {
    const profile = localBridgeProfile();
    const pool = {
      id: profile.id,
      targetAgentId: 'codex' as const,
      surface: 'responses' as const,
      dialect: 'codex' as const,
      v2Enrolled: true,
      gatewayPort: 43121,
      members: [{ sourceKind: 'provider' as const, sourceId: 'kimi-1', enabled: true }],
      listedModels: ['kimi-k2.5'],
    };
    const off = renderDetail({
      profile,
      bridgeStatus: runningStatus(profile.id),
      entries: [openRouterEntry()],
      busy: false,
      error: null,
      onRequestRemove: vi.fn(),
      routePoolV2: false,
      defaultPool: pool,
    });
    expect(off).not.toContain('已接入的登录');
    expect(off).not.toContain('入口 Key 已保存');
    const on = renderDetail({
      profile,
      bridgeStatus: runningStatus(profile.id),
      entries: [openRouterEntry()],
      busy: false,
      error: null,
      onRequestRemove: vi.fn(),
      routePoolV2: true,
      defaultPool: pool,
    });
    expect(on).toContain('http://127.0.0.1:43121');
    expect(on).toContain('Responses');
    expect(on).toContain('已接入的登录');
    expect(on).toContain('OpenRouter');
    expect(on).toContain('入口 Key 已保存');
    expect(on).toContain('kimi-k2.5');
    expect(on).not.toContain('hubToken');
    expect(on).not.toContain('ahb_');
  });

  it('shows enroll CTA only when flag is on, route is native, and plan allows local_bridge', () => {
    const native = { ...localBridgeProfile(), id: 'native-1', route: 'native_endpoint' as const, localPort: null };
    const hidden = renderDetail({
      profile: native,
      entries: [],
      busy: false,
      error: null,
      onRequestRemove: vi.fn(),
      routePoolV2: true,
      canApplyLocalBridge: false,
      onEnrollNative: vi.fn(),
    });
    expect(hidden).not.toContain('交给本机网关');
    const shown = renderDetail({
      profile: native,
      entries: [],
      busy: false,
      error: null,
      onRequestRemove: vi.fn(),
      routePoolV2: true,
      canApplyLocalBridge: true,
      onEnrollNative: vi.fn(),
    });
    expect(shown).toContain('交给本机网关');
  });

  it('shows recent inbound requests newest first, and empty copy when none', () => {
    const profile = localBridgeProfile();
    const empty = renderDetail({
      profile,
      bridgeStatus: runningStatus(profile.id),
      entries: [openRouterEntry()],
      busy: false,
      error: null,
      onRequestRemove: vi.fn(),
    });
    expect(empty).toContain('最近请求');
    expect(empty).toContain('还没有工具连上');
    expect(empty).not.toContain('票');
    expect(empty).not.toContain('钱包');
    expect(empty).not.toContain('投影');
    expect(empty).not.toContain('PKCE');
    expect(empty).not.toContain('loopback');

    const listed = renderDetail({
      profile,
      bridgeStatus: {
        ...runningStatus(profile.id),
        recentInbound: [
          { at: '2026-08-12T00:00:02.000Z', method: 'POST', path: '/v1/responses', status: 200, ok: true },
          { at: '2026-08-12T00:00:01.000Z', method: 'GET', path: '/models', status: 401, ok: false },
        ],
      },
      entries: [openRouterEntry()],
      busy: false,
      error: null,
      onRequestRemove: vi.fn(),
    });
    expect(listed).toContain('最近请求');
    expect(listed).not.toContain('还没有工具连上');
    expect(listed).toContain('POST');
    expect(listed).toContain('/v1/responses');
    expect(listed).toContain('200');
    expect(listed).toContain('成功');
    expect(listed).toContain('/models');
    expect(listed).toContain('401');
    expect(listed).toContain('失败');
    const inbound = listed.slice(listed.indexOf('data-route-inbound'));
    expect(inbound.indexOf('/v1/responses')).toBeLessThan(inbound.indexOf('/models'));
    expect(listed).not.toContain('Authorization');
    expect(listed).not.toContain('sk-');
    expect(listed).not.toContain('ahb_');
  });

  it('opens detail as the same inspect chrome as edit', () => {
    const profile = localBridgeProfile();
    const markup = renderDetail({
      profile,
      bridgeStatus: runningStatus(profile.id),
      entries: [],
      busy: false,
      error: null,
      onRequestRemove: vi.fn(),
      onRequestEdit: vi.fn(),
      asPanel: true,
      open: true,
      onOpenChange: vi.fn(),
    });
    expect(markup).toContain('data-side-inspect');
    expect(markup).not.toContain('取消');
    expect(markup).toContain('编辑');
    expect(markup).toContain('收起');
    expect(markup).toContain('删除路由');
    expect(markup.indexOf('删除路由')).toBeLessThan(markup.indexOf('编辑'));
    expect(markup.indexOf('编辑')).toBeLessThan(markup.indexOf('收起'));
    expect(markup).not.toContain('justify-start gap-2 border-t');
    expect(markup).toContain('路由详情');
    expect(markup).toContain('data-route-detail="bridge-1"');
    expect(markup).not.toContain('role="dialog"');
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

  it('loads profiles without waiting on bridge status', async () => {
    const profile = localBridgeProfile();
    const getBridgeStatus = vi.fn();
    const result = await loadAdapterProfilesList(async () => [profile]);
    expect(result.profiles).toEqual([profile]);
    expect(result.profileState).toBe('ready');
    expect(getBridgeStatus).not.toHaveBeenCalled();
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
