import { describe, expect, it } from 'vitest';

import type { AgentMeta } from '@/config/agents';
import type { AgentId, AgentStatus, AuthStatus } from '@/lib/types';

import {
  AGENT_CARD_BRIDGE_LABEL,
  AGENT_OVERVIEW_GRID,
  agentCardConnectFallback,
  buildAgentCardView,
  cardAuthStatus,
  isAgentIssue,
  mergeAgentsInOrder,
  resolveAgentCardInteraction,
  summarizeAgentOverview,
  type AgentCardBridgeState,
} from './agentOverviewModel';

function meta(id: AgentId, name: string = id): AgentMeta {
  return {
    id,
    name,
    color: '#000',
    letter: id[0]!.toUpperCase(),
    installChannels: [],
  };
}

function status(
  agentId: AgentId,
  overrides: Partial<AgentStatus> = {},
): AgentStatus {
  return {
    agentId,
    installed: true,
    authStatus: 'valid',
    authLabel: '已登录',
    currentProvider: '官方',
    effectiveKind: 'api',
    effectiveLabel: '官方',
    version: '1.0.0',
    running: false,
    envReady: true,
    ...overrides,
  };
}

const METAS = [
  meta('claude', 'Claude Code'),
  meta('codex', 'Codex'),
  meta('kimi', 'Kimi'),
  meta('grok', 'Grok'),
  meta('pi', 'Pi'),
] as const;

describe('AGENT_OVERVIEW_GRID', () => {
  it('uses auto-fit minmax and does not hardcode column count', () => {
    expect(AGENT_OVERVIEW_GRID).toContain('auto-fit');
    expect(AGENT_OVERVIEW_GRID).toContain('minmax(190px,1fr)');
    expect(AGENT_OVERVIEW_GRID).not.toMatch(/grid-cols-\d+/);
  });
});

describe('isAgentIssue', () => {
  it('treats missing status as issue', () => {
    expect(isAgentIssue(undefined)).toBe(true);
  });

  it('treats not installed as issue', () => {
    expect(isAgentIssue(status('claude', { installed: false }))).toBe(true);
  });

  it('treats envReady false as issue even when installed', () => {
    expect(isAgentIssue(status('claude', { envReady: false }))).toBe(true);
  });

  it.each(['expired', 'expiring'] as AuthStatus[])(
    'treats authStatus %s as issue',
    (authStatus) => {
      expect(isAgentIssue(status('claude', { authStatus }))).toBe(true);
    },
  );

  it('treats valid installed agent as ready', () => {
    expect(isAgentIssue(status('claude'))).toBe(false);
  });

  it('treats auth none (未配置) as ready if installed', () => {
    // 异常定义仅 expired/expiring；none 不算待处理
    expect(isAgentIssue(status('claude', { authStatus: 'none', authLabel: '未配置' }))).toBe(
      false,
    );
  });
});

describe('summarizeAgentOverview', () => {
  it('counts ready/issue without hardcoding total', () => {
    // claude+codex ready; kimi missing; grok expiring; pi missing from list → issue
    const agents = [
      status('claude'),
      status('codex'),
      status('kimi', { installed: false }),
      status('grok', { authStatus: 'expiring', authLabel: '即将过期' }),
    ];
    const s = summarizeAgentOverview(METAS, agents);
    expect(s.total).toBe(METAS.length);
    expect(s.readyCount).toBe(2);
    expect(s.issueCount).toBe(METAS.length - 2);
    expect(s.summaryText).toBe(`2/${METAS.length} 就绪 · ${METAS.length - 2} 项待处理`);
  });

  it('omits 待处理 suffix when all ready', () => {
    const agents = METAS.map((m) => status(m.id));
    const s = summarizeAgentOverview(METAS, agents);
    expect(s.summaryText).toBe(`${METAS.length}/${METAS.length} 就绪`);
    expect(s.summaryText).not.toContain('待处理');
  });

  it('scales with N metas (no fixed agent count)', () => {
    const two = [meta('claude', 'A'), meta('codex', 'B')];
    const s2 = summarizeAgentOverview(two, [status('claude')]);
    expect(s2.total).toBe(2);
    expect(s2.readyCount).toBe(1);
    expect(s2.issueCount).toBe(1);
    expect(s2.summaryText).toBe('1/2 就绪 · 1 项待处理');

    // 额外 id 用类型断言，仅验证汇总算法随列表长度变化
    const expanded: AgentMeta[] = [
      ...METAS,
      { ...meta('claude', 'Extra-1'), id: 'extra1' as AgentId },
      { ...meta('claude', 'Extra-2'), id: 'extra2' as AgentId },
    ];
    const n = METAS.length;
    const sExpanded = summarizeAgentOverview(expanded, METAS.map((m) => status(m.id)));
    expect(sExpanded.total).toBe(n + 2);
    expect(sExpanded.readyCount).toBe(n);
    expect(sExpanded.issueCount).toBe(2);
    expect(sExpanded.summaryText).toBe(`${n}/${n + 2} 就绪 · 2 项待处理`);
  });

  it('missing agent in list counts as issue', () => {
    const s = summarizeAgentOverview(METAS, [status('claude')]);
    expect(s.issueCount).toBe(METAS.length - 1);
    expect(s.readyCount).toBe(1);
  });
});

describe('cardAuthStatus', () => {
  it('returns none when missing', () => {
    expect(cardAuthStatus(status('claude', { authStatus: 'valid' }), true)).toBe('none');
  });

  it('returns status auth when installed', () => {
    expect(cardAuthStatus(status('claude', { authStatus: 'expired' }), false)).toBe('expired');
  });

  it('falls back to none when status undefined and not missing flag alone', () => {
    expect(cardAuthStatus(undefined, false)).toBe('none');
  });
});

describe('buildAgentCardView', () => {
  const claude = meta('claude', 'Claude Code');

  it('installed API: shows provider·url and uses connect action', () => {
    const view = buildAgentCardView(
      claude,
      status('claude', {
        effectiveKind: 'api',
        effectiveLabel: 'xx云中转 · relay.xxyun.example.com',
        currentProvider: 'xx云中转 · relay.xxyun.example.com',
        version: '2.1.218',
        authLabel: 'API',
        authStatus: 'valid',
      }),
    );
    expect(view.missing).toBe(false);
    expect(view.envMissing).toBe(false);
    expect(view.versionText).toBe('v2.1.218');
    expect(view.metaText).toBe('xx云中转 · relay.xxyun.example.com');
    expect(view.metaClass).toBe('text-muted');
    expect(view.titleFull).toBe('xx云中转 · relay.xxyun.example.com · API');
    expect(view.ariaLabel).toBe(
      'Claude Code，v2.1.218，API，当前 API 配置 xx云中转 · relay.xxyun.example.com，点击管理连接',
    );
    expect(view.action).toEqual({ kind: 'connect' });
    expect(view.authStatus).toBe('valid');
    expect(view.statusDotTitle).toBe('API');
    expect(view.twoLineLayout).toBe(true);
  });

  it('installed account: shows account label and uses connect action', () => {
    const view = buildAgentCardView(
      claude,
      status('claude', {
        effectiveKind: 'account',
        effectiveLabel: 'me@example.com',
        currentProvider: 'me@example.com',
        version: '2.1.218',
        authLabel: 'Claude Pro',
        authStatus: 'valid',
      }),
    );
    expect(view.versionText).toBe('v2.1.218');
    expect(view.metaText).toBe('me@example.com');
    expect(view.action).toEqual({ kind: 'connect' });
    expect(view.ariaLabel).toContain('当前账号/密钥 me@example.com');
  });

  it('installed with empty connection uses 未配置 fallbacks', () => {
    const view = buildAgentCardView(
      claude,
      status('claude', {
        effectiveKind: 'none',
        effectiveLabel: '未配置',
        currentProvider: undefined,
        version: undefined,
        authLabel: '',
      }),
    );
    expect(view.versionText).toBe('v—');
    expect(view.metaText).toBe('未配置');
    expect(view.titleFull).toBe('未配置 · —');
    expect(view.ariaLabel).toContain('当前连接 未配置');
    expect(view.action).toEqual({ kind: 'connect' });
  });

  it('not installed: install CTA and navigate /agents', () => {
    const view = buildAgentCardView(
      claude,
      status('claude', { installed: false, envReady: true }),
    );
    expect(view.missing).toBe(true);
    expect(view.envMissing).toBe(false);
    expect(view.versionText).toBeNull();
    expect(view.metaText).toBe('未安装 · 点击安装');
    expect(view.metaClass).toBe('text-muted');
    expect(view.titleFull).toBe('未安装 · 点击安装');
    expect(view.ariaLabel).toBe('Claude Code，未安装，点击安装');
    expect(view.action).toEqual({ kind: 'navigate', to: '/agents' });
    expect(view.authStatus).toBe('none');
    expect(view.statusDotTitle).toBe('未安装');
    expect(view.twoLineLayout).toBe(true);
  });

  it('not installed + env not ready: warning meta and navigate /agents', () => {
    const view = buildAgentCardView(
      claude,
      status('claude', { installed: false, envReady: false }),
    );
    expect(view.envMissing).toBe(true);
    expect(view.metaText).toBe('环境未就绪 · 点击修复');
    expect(view.metaClass).toBe('text-warning');
    expect(view.ariaLabel).toBe('Claude Code，环境未就绪，点击修复');
    expect(view.action).toEqual({ kind: 'navigate', to: '/agents' });
    expect(view.statusDotTitle).toBe('环境未就绪');
    expect(view.twoLineLayout).toBe(true);
  });

  it('undefined status behaves as not installed', () => {
    const view = buildAgentCardView(claude, undefined);
    expect(view.missing).toBe(true);
    expect(view.envMissing).toBe(false);
    expect(view.action).toEqual({ kind: 'navigate', to: '/agents' });
    expect(view.metaText).toBe('未安装 · 点击安装');
  });

  it('both install states use two-line layout (equal card height contract)', () => {
    const installed = buildAgentCardView(claude, status('claude'));
    const missing = buildAgentCardView(claude, status('claude', { installed: false }));
    expect(installed.twoLineLayout).toBe(true);
    expect(missing.twoLineLayout).toBe(true);
  });

  it('omits badges when no badge input is provided (equivalent to prior output)', () => {
    const view = buildAgentCardView(claude, status('claude'));
    const withUndefined = buildAgentCardView(claude, status('claude'), undefined);
    const withEmpty = buildAgentCardView(claude, status('claude'), {});
    expect(view.viaAdapter).toBeUndefined();
    expect(view.bridge).toBeUndefined();
    expect(view).toEqual(withUndefined);
    expect(view).toEqual(withEmpty);
    expect(view.action).toEqual({ kind: 'connect' });
  });

  it('maps viaAdapter hit with sourceLabel', () => {
    const view = buildAgentCardView(claude, status('claude'), {
      viaAdapter: { sourceLabel: 'Kimi 会员' },
    });
    expect(view.viaAdapter).toEqual({ sourceLabel: 'Kimi 会员' });
    expect(view.ariaLabel).toContain('经兼容路由 · Kimi 会员');
  });

  it('prefers wallet binding meta text over effectiveLabel', () => {
    const view = buildAgentCardView(
      claude,
      status('claude', { effectiveLabel: '官方' }),
      { binding: { ticketLabel: 'Kimi 会员', routeLabel: '改配置' } },
    );
    expect(view.metaText).toBe('Kimi 会员 · 改配置');
    expect(view.binding).toEqual({ ticketLabel: 'Kimi 会员', routeLabel: '改配置' });
    expect(view.ariaLabel).toContain('当前绑定 Kimi 会员（改配置）');
  });

  it('maps viaAdapter hit without sourceLabel', () => {
    const view = buildAgentCardView(claude, status('claude'), { viaAdapter: {} });
    expect(view.viaAdapter).toEqual({});
    expect(view.ariaLabel).toContain('经兼容路由');
    expect(view.ariaLabel).not.toContain('经兼容路由 ·');
  });

  it.each([
    ['running', '运行中'],
    ['stopped', '已停止'],
    ['degraded', '已降级'],
    ['unavailable', '状态不可用'],
  ] as const)('maps bridge state %s to label %s', (state, label) => {
    const view = buildAgentCardView(claude, status('claude'), { bridge: { state } });
    expect(view.bridge).toEqual({ state, label, profileId: null });
    expect(view.ariaLabel).toContain(label);
    expect(AGENT_CARD_BRIDGE_LABEL[state]).toBe(label);
  });

  it('does not hide unavailable bridge status', () => {
    const view = buildAgentCardView(claude, status('claude'), {
      bridge: { state: 'unavailable' },
    });
    expect(view.bridge).toEqual({ state: 'unavailable', label: '状态不可用', profileId: null });
  });

  it('forwards the current-bridge profileId for /routes deep links', () => {
    const view = buildAgentCardView(claude, status('claude'), {
      bridge: { state: 'running', profileId: 'bridge-1' },
    });
    expect(view.bridge).toEqual({ state: 'running', label: '运行中', profileId: 'bridge-1' });
  });

  it('treats null badge fields as absent', () => {
    const view = buildAgentCardView(claude, status('claude'), {
      viaAdapter: null,
      bridge: null,
    });
    expect(view.viaAdapter).toBeUndefined();
    expect(view.bridge).toBeUndefined();
  });
});

describe('resolveAgentCardInteraction', () => {
  it('keeps navigate action unchanged even when a connect handler exists', () => {
    const onConnect = (_id: AgentId) => undefined;
    expect(
      resolveAgentCardInteraction({ kind: 'navigate', to: '/agents' }, 'claude', onConnect),
    ).toEqual({ type: 'navigate', to: '/agents' });
  });

  it('connect with handler stays connect', () => {
    const onConnect = (_id: AgentId) => undefined;
    expect(resolveAgentCardInteraction({ kind: 'connect' }, 'claude', onConnect)).toEqual({
      type: 'connect',
      agentId: 'claude',
    });
  });

  it('connect without handler falls back to /connections?agent=X', () => {
    expect(resolveAgentCardInteraction({ kind: 'connect' }, 'claude')).toEqual({
      type: 'navigate',
      to: '/connections?agent=claude',
    });
    expect(agentCardConnectFallback('kimi')).toBe('/connections?agent=kimi');
  });
});

describe('mergeAgentsInOrder', () => {
  it('keeps AGENTS definition order even when issues exist', () => {
    // 异常在前的数据顺序不应影响展示顺序
    const agents = [
      status('grok', { installed: false }),
      status('kimi', { authStatus: 'expired', authLabel: '已失效' }),
      status('claude'),
      status('codex'),
    ];
    const merged = mergeAgentsInOrder(METAS, agents);
    expect(merged.map((c) => c.meta.id)).toEqual(METAS.map((m) => m.id));
  });

  it('attaches view model for each meta', () => {
    const merged = mergeAgentsInOrder(METAS, [status('claude')]);
    expect(merged).toHaveLength(METAS.length);
    expect(merged[0]!.view.missing).toBe(false);
    expect(merged[0]!.view.action).toEqual({ kind: 'connect' });
    expect(merged[1]!.view.missing).toBe(true);
    expect(merged[2]!.view.action).toEqual({ kind: 'navigate', to: '/agents' });
  });

  it('passes badge inputs through without reordering', () => {
    const merged = mergeAgentsInOrder(METAS, [status('claude'), status('kimi')], {
      claude: {
        viaAdapter: { sourceLabel: 'Kimi 会员' },
        bridge: { state: 'running' satisfies AgentCardBridgeState },
      },
    });
    expect(merged.map((c) => c.meta.id)).toEqual(METAS.map((m) => m.id));
    expect(merged[0]!.view.viaAdapter).toEqual({ sourceLabel: 'Kimi 会员' });
    expect(merged[0]!.view.bridge).toEqual({ state: 'running', label: '运行中', profileId: null });
    expect(merged[2]!.view.viaAdapter).toBeUndefined();
    expect(merged[2]!.view.bridge).toBeUndefined();
  });

  it('does not reorder when later agents are healthier', () => {
    const onlyLastReady = [status('pi')];
    const ids = mergeAgentsInOrder(METAS, onlyLastReady).map((c) => c.meta.id);
    expect(ids[0]).toBe('claude');
    expect(ids[ids.length - 1]).toBe('pi');
  });
});
