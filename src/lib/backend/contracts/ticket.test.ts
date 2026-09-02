import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  adapterRouteToBinding,
  bindingRouteDashboardLabel,
  bindingRouteUsageLabel,
  groupTicketSurfaceMembers,
  isActiveBindingForAgent,
  isBindSuccessForAgent,
  mapBindTicketResult,
  mapBindingView,
  mapPlanTicketResult,
  mapTicketView,
  mapTicketWallet,
  mapUnbindTicketResult,
  memberHealthFromAuthHealth,
  surfaceGroupMemberCount,
  ticketCredentialClassLabel,
  ticketIdFor,
  ticketSurfaceLabel,
} from './ticket';

function thrownMessage(run: () => unknown): string {
  try {
    run();
  } catch (error) {
    return error instanceof Error ? error.message : String(error);
  }
  throw new Error('expected mapper to throw');
}

describe('Ticket Rust wire mappers', () => {
  it('maps a full wallet with provider and account tickets plus bindings', () => {
    const wallet = mapTicketWallet({
      tickets: [
        {
          id: 'provider:kimi-1',
          sourceKind: 'provider',
          sourceId: 'kimi-1',
          agentId: 'kimi',
          label: 'Kimi 会员',
          surface: 'kimi-code-membership',
          credentialClass: 'api_key',
          speaks: ['anthropic-messages'],
          importedFrom: null,
        },
        {
          id: 'account:claude-oauth',
          sourceKind: 'account',
          sourceId: 'claude-oauth',
          agentId: 'claude',
          label: 'me@example.com',
          surface: 'unknown',
          credentialClass: 'oauth',
          speaks: [],
          importedFrom: 'claude',
        },
      ],
      bindings: [
        {
          ticketId: 'provider:kimi-1',
          agentId: 'claude',
          route: 'reshape',
          active: true,
          profileId: 'prof-1',
          bridge: null,
        },
        {
          ticketId: 'provider:kimi-1',
          agentId: 'codex',
          route: 'bridge',
          active: true,
          profileId: 'prof-2',
          bridge: { port: 8123, running: true },
        },
      ],
    });

    expect(wallet.tickets).toHaveLength(2);
    expect(wallet.tickets[0]).toMatchObject({
      id: 'provider:kimi-1',
      surface: 'kimi-code-membership',
      credentialClass: 'api_key',
      importedFrom: null,
    });
    expect(wallet.tickets[1]).toMatchObject({
      sourceKind: 'account',
      credentialClass: 'oauth',
      importedFrom: 'claude',
    });
    expect(wallet.bindings[0]).toMatchObject({ route: 'reshape', bridge: null });
    expect(wallet.bindings[1]).toEqual({
      ticketId: 'provider:kimi-1',
      agentId: 'codex',
      route: 'bridge',
      active: true,
      profileId: 'prof-2',
      bridge: { port: 8123, running: true },
    });
    expect(wallet.surfaceGroups).toEqual([
      {
        surface: 'kimi-code-membership',
        credentialClass: 'api_key',
        members: [
          {
            ticketId: 'provider:kimi-1',
            sourceKind: 'provider',
            sourceId: 'kimi-1',
            agentId: 'kimi',
            label: 'Kimi 会员',
          },
        ],
      },
    ]);
  });

  it('keeps empty surfaceGroups and only regroups when the field is not an array', () => {
    const tickets = [
      {
        id: 'account:kimi-a',
        sourceKind: 'account',
        sourceId: 'kimi-a',
        agentId: 'kimi',
        label: 'A',
        surface: 'kimi-code-membership',
        credentialClass: 'api_key',
        speaks: [],
        importedFrom: 'kimi',
      },
    ];
    expect(
      mapTicketWallet({ tickets, bindings: [], surfaceGroups: [] }).surfaceGroups,
    ).toEqual([]);
    expect(mapTicketWallet({ tickets, bindings: [] }).surfaceGroups).toHaveLength(1);
    expect(
      mapTicketWallet({
        tickets,
        bindings: [],
        surfaceGroups: 'not-an-array' as unknown as [],
      }).surfaceGroups,
    ).toHaveLength(1);
  });

  it('adapterRouteToBinding never returns native', () => {
    expect(adapterRouteToBinding('native_endpoint')).toBe('reshape');
    expect(adapterRouteToBinding('config_sync')).toBe('reshape');
    expect(adapterRouteToBinding('local_bridge')).toBe('bridge');
    expect(adapterRouteToBinding('unsupported')).toBeNull();
  });

  it('maps explicit surfaceGroups and does not regroup unknown surfaces', () => {
    const wallet = mapTicketWallet({
      tickets: [],
      bindings: [],
      surfaceGroups: [
        {
          surface: 'grok-xai-subscription',
          credentialClass: 'oauth',
          members: [
            {
              ticketId: 'account:g1',
              sourceKind: 'account',
              sourceId: 'g1',
              agentId: 'grok',
              label: 'a@x.com',
            },
            {
              ticketId: 'account:g2',
              sourceKind: 'account',
              sourceId: 'g2',
              agentId: 'grok',
              label: 'b@x.com',
            },
          ],
        },
      ],
    });
    expect(wallet.surfaceGroups).toHaveLength(1);
    expect(wallet.surfaceGroups[0]?.members.map((m) => m.ticketId)).toEqual([
      'account:g1',
      'account:g2',
    ]);
  });

  it('maps optional member health and ignores unknown health tokens', () => {
    const wallet = mapTicketWallet({
      tickets: [],
      bindings: [],
      surfaceGroups: [
        {
          surface: 'kimi-code-membership',
          credentialClass: 'api_key',
          members: [
            {
              ticketId: 'provider:kimi-1',
              sourceKind: 'provider',
              sourceId: 'kimi-1',
              agentId: 'kimi',
              label: 'Kimi 会员',
              health: 'Renewable',
            },
            {
              ticketId: 'account:kimi-stale',
              sourceKind: 'account',
              sourceId: 'kimi-stale',
              agentId: 'kimi',
              label: 'Kimi 会员（失效号）',
              health: 'NeedsLogin',
            },
            {
              ticketId: 'account:kimi-try',
              sourceKind: 'account',
              sourceId: 'kimi-try',
              agentId: 'kimi',
              label: 'try',
              health: 'not-a-health',
            },
          ],
        },
      ],
    });
    expect(wallet.surfaceGroups[0]?.members.map((member) => member.health)).toEqual([
      'renewable',
      'needs_login',
      undefined,
    ]);
  });

  it('maps AuthHealth onto picker health and counts a C1 pool', () => {
    expect(memberHealthFromAuthHealth('needs_login')).toBe('needs_login');
    expect(memberHealthFromAuthHealth('missing')).toBe('needs_login');
    expect(memberHealthFromAuthHealth('unknown')).toBe('try_once');
    expect(memberHealthFromAuthHealth('configured')).toBe('renewable');
    expect(surfaceGroupMemberCount([], 'provider:kimi-1')).toBe(1);
    expect(surfaceGroupMemberCount([{
      surface: 'kimi-code-membership',
      credentialClass: 'api_key',
      members: [
        {
          ticketId: 'account:kimi-stale',
          sourceKind: 'account',
          sourceId: 'kimi-stale',
          agentId: 'kimi',
          label: 'stale',
        },
        {
          ticketId: 'provider:kimi-1',
          sourceKind: 'provider',
          sourceId: 'kimi-1',
          agentId: 'kimi',
          label: 'kimi',
        },
      ],
    }], 'provider:kimi-1')).toBe(2);
  });

  it('groups same surface+class, mixes account/provider, skips unknown', () => {
    const groups = groupTicketSurfaceMembers([
      {
        id: 'provider:kimi-b',
        sourceKind: 'provider',
        sourceId: 'kimi-b',
        agentId: 'kimi',
        label: 'B',
        surface: 'kimi-code-membership',
        credentialClass: 'api_key',
        speaks: [],
        importedFrom: 'kimi',
      },
      {
        id: 'account:kimi-a',
        sourceKind: 'account',
        sourceId: 'kimi-a',
        agentId: 'kimi',
        label: 'A',
        surface: 'kimi-code-membership',
        credentialClass: 'api_key',
        speaks: [],
        importedFrom: 'kimi',
      },
      {
        id: 'provider:anth',
        sourceKind: 'provider',
        sourceId: 'anth',
        agentId: 'claude',
        label: 'Anth',
        surface: 'anthropic-api',
        credentialClass: 'api_key',
        speaks: [],
        importedFrom: 'claude',
      },
      {
        id: 'account:unk',
        sourceKind: 'account',
        sourceId: 'unk',
        agentId: 'pi',
        label: 'x',
        surface: 'unknown',
        credentialClass: 'oauth',
        speaks: [],
        importedFrom: 'pi',
      },
    ]);
    expect(groups.map((g) => `${g.surface}:${g.credentialClass}`)).toEqual([
      'anthropic-api:api_key',
      'kimi-code-membership:api_key',
    ]);
    expect(groups[1]?.members.map((m) => m.ticketId)).toEqual([
      'account:kimi-a',
      'provider:kimi-b',
    ]);
  });

  it('fails closed: unknown surface/credentialClass → unknown; invalid route throws', () => {
    expect(
      mapTicketView({
        id: 'provider:x',
        sourceKind: 'provider',
        sourceId: 'x',
        agentId: 'claude',
        label: 'x',
        surface: 'future-surface',
        credentialClass: 'future-class',
        speaks: [],
      }).surface,
    ).toBe('unknown');
    expect(
      mapTicketView({
        id: 'provider:x',
        sourceKind: 'provider',
        sourceId: 'x',
        agentId: 'claude',
        label: 'x',
        surface: 'unknown',
        credentialClass: 'future-class',
        speaks: [],
      }).credentialClass,
    ).toBe('unknown');

    expect(() =>
      mapBindingView({
        ticketId: 'provider:x',
        agentId: 'claude',
        route: 'local_bridge',
        active: true,
      }),
    ).toThrow(/route/);
  });

  it('rejects invalid sourceKind', () => {
    expect(() =>
      mapTicketView({
        id: 'x',
        sourceKind: 'projection',
        sourceId: 'x',
        agentId: 'claude',
        label: 'x',
        surface: 'unknown',
        credentialClass: 'unknown',
        speaks: [],
      }),
    ).toThrow(/sourceKind/);
  });

  it('keeps the bridge when port is invalid or null (does not drop running)', () => {
    expect(
      mapBindingView({
        ticketId: 'provider:x',
        agentId: 'codex',
        route: 'bridge',
        active: true,
        bridge: { port: 0, running: true },
      }).bridge,
    ).toEqual({ port: null, running: true });

    expect(
      mapBindingView({
        ticketId: 'provider:x',
        agentId: 'codex',
        route: 'bridge',
        active: true,
        bridge: { port: null, running: false },
      }).bridge,
    ).toEqual({ port: null, running: false });
  });

  it('reuses adapter apply-plan mapping for plan_ticket', () => {
    const raw = {
      analysis: {
        route: 'config_sync',
        support: 'stable',
        reason: 'ok',
        actions: [],
        limitations: [],
        evidence: [],
        ruleId: 'kimi-membership-to-claude-v1',
        gateKind: 'none',
      },
      targetAgentId: 'claude' as const,
      canApply: true,
      serviceImpact: 'none',
      changes: [],
    };
    const plan = mapPlanTicketResult(raw);
    expect(plan).toMatchObject({
      canApply: true,
      analysis: { route: 'config_sync' },
      targetAgentId: 'claude',
    });
    expect(mapPlanTicketResult({ plan: raw })).toMatchObject({
      canApply: true,
      analysis: { route: 'config_sync' },
      targetAgentId: 'claude',
    });
  });

  it('maps bind_ticket { binding } with the same BindingView fields as list_ticket_wallet', () => {
    const result = mapBindTicketResult({
      binding: {
        ticketId: 'account:anth-1',
        agentId: 'pi',
        route: 'reshape',
        active: true,
        profileId: 'prof-pi',
        bridge: null,
      },
    });
    expect(result.binding).toEqual({
      ticketId: 'account:anth-1',
      agentId: 'pi',
      route: 'reshape',
      active: true,
      profileId: 'prof-pi',
      bridge: null,
    });
    expect(isActiveBindingForAgent(result.binding, 'pi')).toBe(true);
    expect(isActiveBindingForAgent(result.binding, 'claude')).toBe(false);
  });

  it('maps a raw TicketBinding object (live Rust bind_ticket serde)', () => {
    const result = mapBindTicketResult({
      ticketId: 'account:anth-1',
      agentId: 'pi',
      route: 'reshape',
      active: true,
      profileId: 'prof-pi',
      bridge: null,
    });
    expect(result.binding).toEqual({
      ticketId: 'account:anth-1',
      agentId: 'pi',
      route: 'reshape',
      active: true,
      profileId: 'prof-pi',
      bridge: null,
    });
    expect(isActiveBindingForAgent(result.binding, 'pi')).toBe(true);
  });

  it('maps a raw TicketBinding bridge object with port 44227', () => {
    const result = mapBindTicketResult({
      ticketId: 'provider:kimi-1',
      agentId: 'codex',
      route: 'bridge',
      active: true,
      profileId: 'prof-bridge',
      bridge: { port: 44227, running: true },
    });
    expect(result.binding).toEqual({
      ticketId: 'provider:kimi-1',
      agentId: 'codex',
      route: 'bridge',
      active: true,
      profileId: 'prof-bridge',
      bridge: { port: 44227, running: true },
    });
    expect(isActiveBindingForAgent(result.binding, 'codex')).toBe(true);
  });

  it('treats hosted local_bridge bind as success even when active is false', () => {
    const result = mapBindTicketResult({
      ticketId: 'provider:openrouter-1',
      agentId: 'claude',
      route: 'bridge',
      active: false,
      profileId: 'prof-or-claude',
      bridge: { port: 43121, running: true },
    });
    expect(isActiveBindingForAgent(result.binding, 'claude')).toBe(false);
    expect(isBindSuccessForAgent(result.binding, 'claude')).toBe(true);
    expect(isBindSuccessForAgent(result.binding, 'grok')).toBe(false);
  });


  it('rejects a bind result without ticketId/agentId, and accepts empty unbind', () => {
    const bindEmpty = thrownMessage(() => mapBindTicketResult({} as never));
    expect(bindEmpty).toMatch(/绑定结果无法识别/);
    expect(bindEmpty).not.toMatch(/wire|bind_ticket/i);

    const bindBadRoute = thrownMessage(() => mapBindTicketResult({
      ticketId: 'account:anth-1',
      agentId: 'pi',
      route: 'not-a-route',
      active: true,
    } as never));
    expect(bindBadRoute).toMatch(/绑定结果无法识别/);
    expect(bindBadRoute).not.toMatch(/wire|bind_ticket/i);

    const planEmpty = thrownMessage(() => mapPlanTicketResult({} as never));
    expect(planEmpty).toMatch(/连接方案无法识别/);
    expect(planEmpty).not.toMatch(/wire|plan_ticket/i);

    const unbindBad = thrownMessage(() => mapUnbindTicketResult('nope'));
    expect(unbindBad).toMatch(/停止并还原结果无法识别/);
    expect(unbindBad).not.toMatch(/wire|unbind_ticket/i);

    expect(mapUnbindTicketResult({})).toBeUndefined();
    expect(mapUnbindTicketResult({ tickets: [], bindings: [] })).toBeUndefined();
    expect(mapUnbindTicketResult(null)).toBeUndefined();
  });

  it('builds ticket ids from source kind + row id', () => {
    expect(ticketIdFor('account', 'anth-1')).toBe('account:anth-1');
    expect(ticketIdFor('provider', 'kimi-1')).toBe('provider:kimi-1');
  });
});

describe('ticket / binding display labels', () => {
  it('maps route labels for usage and dashboard (English fallback with no t)', () => {
    expect(bindingRouteUsageLabel('native')).toBe('');
    expect(bindingRouteUsageLabel('reshape')).toBe('Rewrite config');
    expect(bindingRouteUsageLabel('bridge')).toBe('Local route');
    expect(bindingRouteDashboardLabel('native')).toBe('');
    expect(bindingRouteDashboardLabel('reshape')).toBe('Rewrite config');
    expect(bindingRouteDashboardLabel('bridge')).toBe('Local route');
  });

  it('maps credential and surface chip labels (English fallback with no t)', () => {
    expect(ticketCredentialClassLabel('oauth')).toBe('Official login');
    expect(ticketCredentialClassLabel('api_key')).toBe('API Key');
    expect(ticketCredentialClassLabel('unknown')).toBe('Unrecognized');
    expect(ticketSurfaceLabel('kimi-code-membership')).toBe('Membership');
    expect(ticketSurfaceLabel('anthropic-api')).toBe('API');
    expect(ticketSurfaceLabel('openai-api')).toBe('OpenAI');
    expect(ticketSurfaceLabel('xai-api')).toBe('xAI');
    expect(ticketSurfaceLabel('glm-coding-plan')).toBe('GLM');
    expect(ticketSurfaceLabel('deepseek-api')).toBe('DeepSeek');
    expect(ticketSurfaceLabel('codex-chatgpt-subscription')).toBe('Subscription');
    expect(ticketSurfaceLabel('claude-subscription')).toBe('Subscription');
    expect(ticketSurfaceLabel('grok-xai-subscription')).toBe('Subscription');
    expect(ticketSurfaceLabel('unknown')).toBe('Unrecognized');
  });

  it('maps route labels via t for zh and en', () => {
    const zh = createTranslator('zh');
    const en = createTranslator('en');
    expect(bindingRouteUsageLabel('native', zh)).toBe('');
    expect(bindingRouteUsageLabel('reshape', zh)).toBe('改配置');
    expect(bindingRouteUsageLabel('bridge', zh)).toBe('本机路由');
    expect(bindingRouteUsageLabel('native', en)).toBe('');
    expect(bindingRouteUsageLabel('reshape', en)).toBe('Rewrite config');
    expect(bindingRouteUsageLabel('bridge', en)).toBe('Local route');
  });

  it('maps credential and surface chip labels via t for zh and en', () => {
    const zh = createTranslator('zh');
    const en = createTranslator('en');
    expect(ticketCredentialClassLabel('oauth', zh)).toBe('官方登录');
    expect(ticketCredentialClassLabel('oauth', en)).toBe('Official login');
    expect(ticketCredentialClassLabel('unknown', zh)).toBe('未识别');
    expect(ticketCredentialClassLabel('unknown', en)).toBe('Unrecognized');
    expect(ticketSurfaceLabel('kimi-code-membership', zh)).toBe('会员');
    expect(ticketSurfaceLabel('kimi-code-membership', en)).toBe('Membership');
    expect(ticketSurfaceLabel('codex-chatgpt-subscription', zh)).toBe('订阅');
    expect(ticketSurfaceLabel('codex-chatgpt-subscription', en)).toBe('Subscription');
    expect(ticketSurfaceLabel('unknown', zh)).toBe('未识别');
    expect(ticketSurfaceLabel('unknown', en)).toBe('Unrecognized');
  });
});
