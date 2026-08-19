import { describe, expect, it } from 'vitest';
import { getBackend } from '@/app/runtime';
import { planTicket } from '@/lib/api/tickets';
import { upsertMockAccount } from '@/dev/mocks/account';
import type { Account, AgentStatus, Provider } from '@/lib/types';
import type { AdapterApplyPlan, AdapterProfile, AdapterRouteAnalysis } from '@/lib/api/adapter';
import { buildSourceOptions, isOauthIncomplete, planMaturityLabel, planToEligibility } from './eligibility';

function analysis(overrides: Partial<AdapterRouteAnalysis> = {}): AdapterRouteAnalysis {
  return {
    route: 'native_endpoint',
    support: 'stable',
    reason: '默认原因',
    actions: [],
    limitations: [],
    evidence: [],
    ...overrides,
  };
}

function plan(overrides: Partial<AdapterApplyPlan> = {}): AdapterApplyPlan {
  return {
    analysis: analysis(),
    targetAgentId: 'claude',
    canApply: true,
    serviceImpact: 'none',
    changes: [],
    ...overrides,
  };
}

function account(overrides: Partial<Account> = {}): Account {
  return {
    id: 'acc-1',
    agentId: 'claude',
    kind: 'oauth',
    label: 'claude@example.com',
    isCurrent: false,
    tokenValid: true,
    authHealth: 'renewable',
    ...overrides,
  };
}

function provider(overrides: Partial<Provider> = {}): Provider {
  return {
    id: 'prov-1',
    agentId: 'claude',
    name: 'Claude API',
    preset: 'anthropic',
    configText: '{}',
    configFormat: 'json',
    isCurrent: false,
    ...overrides,
  };
}

function agentStatus(overrides: Partial<AgentStatus> & Pick<AgentStatus, 'agentId'>): AgentStatus {
  return {
    installed: true,
    authStatus: 'valid',
    authLabel: '已登录',
    running: false,
    ...overrides,
  };
}

function profile(overrides: Partial<AdapterProfile> = {}): AdapterProfile {
  return {
    id: 'profile-1',
    name: 'kimi → claude',
    sourceKind: 'provider',
    sourceId: 'kimi-src',
    targetAgentId: 'claude',
    route: 'native_endpoint',
    mode: 'api',
    status: 'active',
    ruleId: 'kimi-membership-to-claude-v1',
    ruleVersion: '1',
    generatedProviderId: 'gen-claude',
    autoStart: false,
    createdAt: '2026-08-01T00:00:00.000Z',
    updatedAt: '2026-08-01T00:00:00.000Z',
    ...overrides,
  };
}

describe('planMaturityLabel', () => {
  it('maps the four planner maturity tiers', () => {
    expect(planMaturityLabel('stable')).toBe('稳定');
    expect(planMaturityLabel('experimental')).toBe('');
    expect(planMaturityLabel('preview')).toBe('可预览');
    expect(planMaturityLabel('none')).toBe('');
    expect(planMaturityLabel(undefined)).toBe('');
  });
});

describe('planToEligibility', () => {
  it('maps canApply=true to a ready selectable branch', () => {
    const ready = planToEligibility(plan({ canApply: true }));
    expect(ready).toMatchObject({
      kind: 'ready',
      canApply: true,
      routeSummary: '直连',
    });
    expect(ready.kind === 'ready' && ready.reason).toBeUndefined();
  });

  it('maps canApply=false even when support is stable, and passes reason verbatim', () => {
    const reason = [
      'Codex / ChatGPT 订阅 → Claude Code：当前不支持。',
      '尚未通过上游授权、条款与协议兼容性门禁，plan.canApply=false。',
    ].join('');
    const ready = planToEligibility(plan({
      canApply: false,
      analysis: analysis({
        route: 'unsupported',
        support: 'stable',
        reason,
      }),
    }));
    expect(ready).toEqual({
      kind: 'ready',
      plan: expect.objectContaining({ canApply: false }),
      canApply: false,
      routeSummary: '当前不支持',
      reason,
    });
    expect(ready.kind === 'ready' && ready.reason).toBe(reason);
  });

  it('Account Anthropic → Pi is writable from plan.canApply', async () => {
    getBackend();
    upsertMockAccount({
      id: 'anth-acc-elig',
      agentId: 'claude',
      kind: 'apikey',
      label: 'Anthropic key',
      isCurrent: false,
      tokenValid: true,
      extra: { provider: 'anthropic' },
    } as Account);
    const planned = await planTicket('account:anth-acc-elig', 'pi');
    expect(planned.canApply).toBe(true);
    expect(planned.analysis.route).toBe('config_sync');
    const eligibility = planToEligibility(planned);
    expect(eligibility).toMatchObject({
      kind: 'ready',
      canApply: true,
      routeSummary: '直连',
    });
    expect(eligibility.kind === 'ready' && eligibility.reason).toBeUndefined();
  });

  it('prefers plan.reason over analysis.reason when canApply is false', () => {
    const ready = planToEligibility(plan({
      canApply: false,
      reason: '同边但暂不可写：写入仍只接受 Provider 行，下一步 bind 打通。',
      analysis: analysis({
        route: 'config_sync',
        support: 'stable',
        reason: '显式 Anthropic API Key 可预览为 Pi 的配置同步。',
      }),
    }));
    expect(ready.kind === 'ready' && ready.reason).toContain('Provider');
    expect(ready.kind === 'ready' && ready.reason).toContain('写入');
  });

  it('extracts human route summaries from AdapterRoute', () => {
    const bridge = planToEligibility(plan({
      analysis: analysis({ route: 'local_bridge' }),
    }));
    const sync = planToEligibility(plan({
      analysis: analysis({ route: 'config_sync' }),
    }));
    expect(bridge.kind).toBe('ready');
    expect(sync.kind).toBe('ready');
    if (bridge.kind === 'ready') expect(bridge.routeSummary).toBe('本机路由');
    if (sync.kind === 'ready') expect(sync.routeSummary).toBe('直连');
  });

  it('prefers native subscription reusePath over config_sync route', () => {
    const preview = planToEligibility(plan({
      canApply: false,
      reusePath: 'native_subscription',
      analysis: analysis({ route: 'config_sync', gateKind: 'preview_only' }),
    }));
    expect(preview.kind).toBe('ready');
    if (preview.kind === 'ready') {
      expect(preview.routeSummary).toBe('用这份登录');
      expect(preview.canApply).toBe(false);
    }
  });
});

describe('isOauthIncomplete', () => {
  it('is false for API Key and completed OAuth', () => {
    expect(isOauthIncomplete(account({ kind: 'apikey', authHealth: 'needs_login' }))).toBe(false);
    expect(isOauthIncomplete(account({ kind: 'oauth', authHealth: 'renewable', tokenValid: true }))).toBe(false);
  });

  it('detects incomplete OAuth via health and inferred status (adapter-sources replica)', () => {
    expect(isOauthIncomplete(account({ kind: 'oauth', authHealth: 'needs_login' }))).toBe(true);
    expect(isOauthIncomplete(account({ kind: 'oauth', authHealth: 'missing' }))).toBe(true);
    expect(isOauthIncomplete(account({
      kind: 'oauth',
      authHealth: undefined,
      tokenValid: false,
      refreshable: false,
    }))).toBe(true);
  });
});

describe('buildSourceOptions', () => {
  it('groups target-owned credentials as native and the rest as cross/plannable', () => {
    const options = buildSourceOptions({
      targetAgentId: 'claude',
      accounts: [
        account({ id: 'claude-cur', isCurrent: true, label: 'current@claude' }),
        account({ id: 'claude-other', label: 'other@claude' }),
        account({ id: 'codex-acc', agentId: 'codex', label: 'codex@openai' }),
      ],
      providers: [
        provider({ id: 'claude-p', name: 'Claude Key', isCurrent: false }),
        provider({ id: 'kimi-p', agentId: 'kimi', name: 'Kimi Member' }),
      ],
      profiles: [],
    });

    const native = options.filter((item) => item.group === 'native');
    const cross = options.filter((item) => item.group === 'cross');
    expect(native.map((item) => item.ref.id)).toEqual(['claude-cur', 'claude-other', 'claude-p']);
    expect(native.find((item) => item.ref.id === 'claude-cur')?.state).toEqual({ kind: 'current' });
    expect(native.find((item) => item.ref.id === 'claude-other')?.state).toEqual({ kind: 'switchable' });
    expect(native.find((item) => item.ref.id === 'claude-p')?.state).toEqual({ kind: 'switchable' });
    expect(cross.map((item) => item.ref.id)).toEqual(['codex-acc', 'kimi-p']);
    expect(cross.every((item) => item.state.kind === 'plannable')).toBe(true);
  });

  it('puts adapter-generated Providers in the native group with viaAdapter.sourceLabel', () => {
    const options = buildSourceOptions({
      targetAgentId: 'claude',
      accounts: [],
      providers: [
        provider({ id: 'kimi-src', agentId: 'kimi', name: 'Kimi 会员' }),
        provider({ id: 'gen-claude', name: 'Claude via Kimi', isCurrent: true }),
      ],
      profiles: [profile({ sourceKind: 'provider', sourceId: 'kimi-src', generatedProviderId: 'gen-claude' })],
    });

    const generated = options.find((item) => item.ref.id === 'gen-claude');
    expect(generated).toMatchObject({
      group: 'native',
      state: { kind: 'current' },
      viaAdapter: { sourceLabel: 'Kimi 会员' },
    });
    expect(options.some((item) => item.group === 'cross' && item.ref.id === 'gen-claude')).toBe(false);
    expect(options.some((item) => item.group === 'cross' && item.ref.id === 'kimi-src')).toBe(true);
  });

  it('excludes adapter-generated Providers from the cross group even when they belong to another Agent', () => {
    const options = buildSourceOptions({
      targetAgentId: 'codex',
      accounts: [],
      providers: [
        provider({ id: 'kimi-src', agentId: 'kimi', name: 'Kimi 会员' }),
        provider({ id: 'gen-claude', agentId: 'claude', name: 'Claude via Kimi' }),
      ],
      profiles: [profile({ generatedProviderId: 'gen-claude', sourceId: 'kimi-src' })],
    });

    expect(options.map((item) => item.ref.id)).toEqual(['kimi-src']);
    expect(options[0]?.group).toBe('cross');
  });

  it('marks native account switch blocked with Connections reason text (workbuddy catalog)', () => {
    const options = buildSourceOptions({
      targetAgentId: 'workbuddy',
      accounts: [account({ id: 'wb-1', agentId: 'workbuddy', label: 'wb@local', isCurrent: false })],
      providers: [],
      profiles: [],
    });
    expect(options[0]?.state).toEqual({
      kind: 'blocked_native',
      reason: '暂不支持账号池切换',
    });
  });

  it('marks native provider switch blocked with Connections gate reason (cursor catalog)', () => {
    const options = buildSourceOptions({
      targetAgentId: 'cursor',
      accounts: [],
      providers: [provider({ id: 'cur-p', agentId: 'cursor', name: 'Cursor Key' })],
      profiles: [],
    });
    expect(options[0]?.state).toEqual({
      kind: 'blocked_native',
      reason: '无稳定配置写入契约，fail-closed',
    });
  });

  it('keeps isCurrent as current even when the native capability gate is closed', () => {
    const options = buildSourceOptions({
      targetAgentId: 'workbuddy',
      accounts: [account({ id: 'wb-cur', agentId: 'workbuddy', isCurrent: true })],
      providers: [],
      profiles: [],
    });
    expect(options[0]?.state).toEqual({ kind: 'current' });
  });

  it('falls back to catalog capabilities when agentStatuses is omitted', () => {
    const options = buildSourceOptions({
      targetAgentId: 'claude',
      accounts: [account({ id: 'claude-other', label: 'other@claude' })],
      providers: [],
      profiles: [],
    });
    expect(options[0]?.state).toEqual({ kind: 'switchable' });
  });

  it('blocks native account using live agentStatuses accountSwitch reason', () => {
    const reason = 'doctor: 账号切换已关闭';
    const options = buildSourceOptions({
      targetAgentId: 'claude',
      accounts: [account({ id: 'claude-other', label: 'other@claude' })],
      providers: [],
      profiles: [],
      agentStatuses: [
        agentStatus({
          agentId: 'claude',
          capabilities: {
            accountSwitch: { level: 'unsupported', reason },
          },
        }),
      ],
    });
    expect(options[0]?.state).toEqual({ kind: 'blocked_native', reason });
  });

  it('marks native account switchable when live agentStatuses allow accountSwitch', () => {
    const options = buildSourceOptions({
      targetAgentId: 'workbuddy',
      accounts: [account({ id: 'wb-1', agentId: 'workbuddy', label: 'wb@local' })],
      providers: [],
      profiles: [],
      agentStatuses: [
        agentStatus({
          agentId: 'workbuddy',
          capabilities: {
            accountSwitch: { level: 'full' },
          },
        }),
      ],
    });
    expect(options[0]?.state).toEqual({ kind: 'switchable' });
  });
});
