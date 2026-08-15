import { describe, expect, it } from 'vitest';
import {
  bindingRouteDashboardLabel,
  bindingRouteUsageLabel,
  isActiveBindingForAgent,
  mapBindTicketResult,
  mapBindingView,
  mapPlanTicketResult,
  mapTicketView,
  mapTicketWallet,
  mapUnbindTicketResult,
  ticketCredentialClassLabel,
  ticketIdFor,
  ticketSurfaceLabel,
} from './ticket';

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
    const plan = mapPlanTicketResult({
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
      targetAgentId: 'claude',
      canApply: true,
      serviceImpact: 'none',
      changes: [],
    });
    expect(plan).toMatchObject({
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

  it('rejects bind_ticket wire without binding, and accepts empty unbind_ticket', () => {
    expect(() => mapBindTicketResult({} as never)).toThrow(/binding/);
    expect(() => mapUnbindTicketResult('nope')).toThrow(/unbind_ticket/);
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
  it('maps route labels for usage and dashboard', () => {
    expect(bindingRouteUsageLabel('native')).toBe('切换');
    expect(bindingRouteUsageLabel('reshape')).toBe('改配置');
    expect(bindingRouteUsageLabel('bridge')).toBe('本机桥');
    expect(bindingRouteDashboardLabel('native')).toBe('直连');
    expect(bindingRouteDashboardLabel('reshape')).toBe('改配置');
    expect(bindingRouteDashboardLabel('bridge')).toBe('本机桥');
  });

  it('maps credential and surface chip labels', () => {
    expect(ticketCredentialClassLabel('oauth')).toBe('官方登录');
    expect(ticketCredentialClassLabel('api_key')).toBe('API Key');
    expect(ticketCredentialClassLabel('unknown')).toBe('未识别');
    expect(ticketSurfaceLabel('kimi-code-membership')).toBe('会员');
    expect(ticketSurfaceLabel('anthropic-api')).toBe('官方');
    expect(ticketSurfaceLabel('codex-chatgpt-subscription')).toBe('订阅');
    expect(ticketSurfaceLabel('unknown')).toBe('未识别');
  });
});
