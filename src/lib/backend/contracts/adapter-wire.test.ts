import { describe, expect, it } from 'vitest';
import {
  mapAdapterApplyPlan,
  mapAdapterApplyResult,
  mapAdapterBridgeStatusDto,
  mapAdapterRouteAnalysis,
  mapDefaultRoutePoolList,
  mapInboundRequest,
  mapLocalTokenProbeResult,
  mapLocalTokenRecord,
} from './adapter-wire';

describe('Adapter Rust wire mappers', () => {
  it('maps the serialized Rust AdapterApplyResult and reuses the core provider mapper', () => {
    const result = mapAdapterApplyResult({
      profile: {
        id: 'adapter-kimi-codex-1',
        name: 'Kimi → Codex 本地桥接',
        sourceKind: 'provider',
        sourceId: 'provider-kimi',
        targetAgentId: 'codex',
        route: 'local_bridge',
        mode: 'api',
        status: 'active',
        ruleId: 'kimi-membership-to-codex-bridge-v1',
        ruleVersion: '1',
        generatedProviderId: 'generated-codex-1',
        localPort: 43123,
        autoStart: true,
        createdAt: '2026-08-12T00:00:00.000Z',
        updatedAt: '2026-08-12T00:00:00.000Z',
      },
      provider: {
        id: 'generated-codex-1',
        agentId: 'codex',
        name: 'AgentHub Kimi 本地桥接',
        settingsConfig: {
          baseUrl: 'http://127.0.0.1:43123/v1',
          model: 'kimi-k2.5',
        },
        meta: { preset: 'openai-compatible' },
        isCurrent: true,
        createdAt: '2026-08-12T00:00:00.000Z',
        updatedAt: '2026-08-12T00:00:00.000Z',
      },
    });

    expect(result).toMatchObject({
      profile: { route: 'local_bridge', mode: 'api', localPort: 43123 },
      provider: {
        agentId: 'codex',
        preset: 'openai-compatible',
        configFormat: 'json',
        isCurrent: true,
      },
    });
    expect(JSON.parse(result.provider.configText)).toEqual({
      baseUrl: 'http://127.0.0.1:43123/v1',
      model: 'kimi-k2.5',
    });
  });

  it('maps a wrapped { result } apply payload the same as a raw AdapterApplyResult', () => {
    const raw = {
      profile: {
        id: 'adapter-kimi-codex-1',
        name: 'Kimi → Codex 本地桥接',
        sourceKind: 'provider' as const,
        sourceId: 'provider-kimi',
        targetAgentId: 'codex' as const,
        route: 'local_bridge',
        mode: 'api',
        status: 'active',
        ruleId: 'kimi-membership-to-codex-bridge-v1',
        ruleVersion: '1',
        generatedProviderId: 'generated-codex-1',
        localPort: 43123,
        autoStart: true,
        createdAt: '2026-08-12T00:00:00.000Z',
        updatedAt: '2026-08-12T00:00:00.000Z',
      },
      provider: {
        id: 'generated-codex-1',
        agentId: 'codex' as const,
        name: 'AgentHub Kimi 本地桥接',
        settingsConfig: {
          baseUrl: 'http://127.0.0.1:43123/v1',
          model: 'kimi-k2.5',
        },
        meta: { preset: 'openai-compatible' },
        isCurrent: true,
        createdAt: '2026-08-12T00:00:00.000Z',
        updatedAt: '2026-08-12T00:00:00.000Z',
      },
    };
    expect(mapAdapterApplyResult({ result: raw }).profile.id).toBe('adapter-kimi-codex-1');
    expect(mapAdapterApplyResult(raw).provider.id).toBe('generated-codex-1');
  });

  it('derives the loopback endpoint and ISO time from the Tauri bridge DTO', () => {
    const status = mapAdapterBridgeStatusDto({
      profileId: 'adapter-kimi-codex-1',
      port: 43123,
      running: true,
      state: 'running',
      upstreamStatus: 'unknown',
      startedAtUnixMs: 1_786_492_800_123,
    });

    expect(status).toEqual({
      profileId: 'adapter-kimi-codex-1',
      state: 'running',
      port: 43123,
      endpoint: 'http://127.0.0.1:43123/v1',
      startedAt: '2026-08-12T00:00:00.123Z',
      upstreamStatus: 'unknown',
      recentInbound: [],
      totalRequestCount: 0,
      failedRequestCount: 0,
      lastRequestAt: null,
      localToken: null,
    });
  });

  it('maps process-lifetime inbound counters from the bridge status DTO', () => {
    const status = mapAdapterBridgeStatusDto({
      profileId: 'adapter-kimi-codex-1',
      port: 43123,
      running: true,
      state: 'running',
      upstreamStatus: 'connected',
      totalRequestCount: 25,
      failedRequestCount: 5,
      lastRequestAtUnixMs: 1_786_492_800_500,
    });
    expect(status.totalRequestCount).toBe(25);
    expect(status.failedRequestCount).toBe(5);
    expect(status.lastRequestAt).toBe('2026-08-12T00:00:00.500Z');
  });

  it('inbound log struct never carries secrets, query, or extra wire fields', () => {
    const row = mapInboundRequest({
      atUnixMs: 1_786_492_800_000,
      method: 'POST',
      path: '/v1/responses?api_key=sk-secret&token=abc',
      status: 200,
      ok: true,
      authorization: 'Bearer sk-secret',
      body: '{"apiKey":"sk-secret","token":"ahb_secret"}',
      token: 'sk-secret',
      apiKey: 'sk-secret',
    });
    expect(row).toEqual({
      at: '2026-08-12T00:00:00.000Z',
      method: 'POST',
      path: '/v1/responses',
      status: 200,
      ok: true,
    });
    const keys = Object.keys(row ?? {}).sort();
    expect(keys).toEqual(['at', 'method', 'ok', 'path', 'status']);
    const json = JSON.stringify(row);
    expect(json).not.toMatch(/sk-secret|ahb_|Bearer|authorization|apiKey|token/i);
    expect(json).not.toContain('?');
    expect(
      mapAdapterBridgeStatusDto({
        profileId: 'adapter-kimi-codex-1',
        port: 43123,
        running: true,
        state: 'running',
        upstreamStatus: 'connected',
        recentInbound: [
          {
            atUnixMs: 1_786_492_800_002,
            method: 'GET',
            path: '/models',
            status: 200,
            ok: true,
          },
          {
            atUnixMs: 1_786_492_800_000,
            method: 'POST',
            path: '/v1/responses',
            status: 401,
            ok: false,
          },
        ],
      }).recentInbound,
    ).toEqual([
      {
        at: '2026-08-12T00:00:00.002Z',
        method: 'GET',
        path: '/models',
        status: 200,
        ok: true,
      },
      {
        at: '2026-08-12T00:00:00.000Z',
        method: 'POST',
        path: '/v1/responses',
        status: 401,
        ok: false,
      },
    ]);
  });

  it('fails closed for a bridge state unknown to the frontend', () => {
    const status = mapAdapterBridgeStatusDto({
      profileId: 'adapter-kimi-codex-1',
      port: 43123,
      running: true,
      state: 'future_live_state',
      upstreamStatus: 'future_upstream_state',
      startedAtUnixMs: Number.MAX_SAFE_INTEGER + 1,
    });

    expect(status).toMatchObject({
      state: 'error',
      endpoint: 'http://127.0.0.1:43123/v1',
      startedAt: null,
      upstreamStatus: 'unknown',
    });
  });

  it('passes through known upstream labels and fails closed on unknown ones', () => {
    expect(
      mapAdapterBridgeStatusDto({
        profileId: 'adapter-kimi-codex-1',
        port: 43123,
        running: true,
        state: 'running',
        upstreamStatus: 'connected',
      }).upstreamStatus,
    ).toBe('connected');
    expect(
      mapAdapterBridgeStatusDto({
        profileId: 'adapter-kimi-codex-1',
        port: null,
        running: false,
        state: 'stopped',
        upstreamStatus: 'stopped',
      }).upstreamStatus,
    ).toBe('stopped');
    expect(
      mapAdapterBridgeStatusDto({
        profileId: 'adapter-kimi-codex-1',
        port: 43123,
        running: true,
        state: 'degraded',
        upstreamStatus: 'degraded',
      }).upstreamStatus,
    ).toBe('degraded');
    expect(
      mapAdapterBridgeStatusDto({
        profileId: 'adapter-kimi-codex-1',
        port: 43123,
        running: false,
        state: 'error',
        upstreamStatus: 'unavailable',
      }).upstreamStatus,
    ).toBe('unavailable');
    expect(
      mapAdapterBridgeStatusDto({
        profileId: 'adapter-kimi-codex-1',
        port: 43123,
        running: false,
        state: 'error',
        upstreamStatus: 'not-a-real-label',
      }).upstreamStatus,
    ).toBe('unknown');
  });

  it('maps optional ruleId/gateKind and defaults missing gateKind to none', () => {
    const withGate = mapAdapterRouteAnalysis({
      route: 'local_bridge',
      support: 'experimental',
      reason: 'Codex / ChatGPT 订阅可通过本机路由到 Claude Code（Messages → Responses）。',
      actions: [],
      limitations: [],
      evidence: [],
      ruleId: 'codex-subscription-to-claude-responses-v1',
      gateKind: 'none',
    });
    expect(withGate.ruleId).toBe('codex-subscription-to-claude-responses-v1');
    expect(withGate.gateKind).toBe('none');

    const legacy = mapAdapterRouteAnalysis({
      route: 'native_endpoint',
      support: 'stable',
      reason: 'ok',
      actions: [],
      limitations: [],
      evidence: [],
    });
    expect(legacy.ruleId).toBeNull();
    expect(legacy.gateKind).toBe('none');
  });

  it('strips secret values from plan changes and analysis actions', () => {
    const plan = mapAdapterApplyPlan({
      analysis: {
        route: 'native_endpoint',
        support: 'stable',
        reason: 'ok',
        actions: [
          {
            kind: 'reference_connection_secret',
            target: 'claude',
            description: 'reference source secret',
            value: 'sk-must-not-leak',
            secret: true,
          },
          {
            kind: 'set_env',
            target: 'claude',
            description: 'set base url',
            value: 'https://api.example/',
            secret: false,
          },
        ],
        limitations: [],
        evidence: [],
      },
      targetAgentId: 'claude',
      canApply: true,
      maturity: 'stable',
      reason: 'ok',
      serviceImpact: 'none',
      changes: [
        { target: 'claude', field: 'apiKey', value: 'sk-must-not-leak', secret: true },
        { target: 'claude', field: 'baseUrl', value: 'https://api.example/', secret: false },
      ],
    });

    const secretAction = plan.analysis.actions.find((item) => item.secret);
    const secretChange = plan.changes.find((item) => item.secret);
    expect(secretAction).toEqual({
      kind: 'reference_connection_secret',
      target: 'claude',
      description: 'reference source secret',
      secret: true,
    });
    expect(secretAction).not.toHaveProperty('value');
    expect(secretChange).toEqual({ target: 'claude', field: 'apiKey', secret: true });
    expect(secretChange).not.toHaveProperty('value');
    expect(JSON.stringify(plan)).not.toContain('sk-must-not-leak');
    expect(plan.maturity).toBe('stable');
    expect(plan.reason).toBe('ok');
  });

  it('maps planner maturity and falls back reason / maturity on older wires', () => {
    const preview = mapAdapterApplyPlan({
      analysis: {
        route: 'unsupported',
        support: 'unsupported',
        reason: 'Codex / ChatGPT 订阅可通过本机路由到 Claude Code（Messages → Responses）。',
        actions: [],
        limitations: [],
        evidence: [],
      },
      targetAgentId: 'claude',
      canApply: false,
      maturity: 'preview',
      reusePath: 'none',
      reason: 'Codex / ChatGPT 订阅可通过本机路由到 Claude Code（Messages → Responses）。',
      serviceImpact: 'none',
      changes: [],
    });
    expect(preview.maturity).toBe('preview');
    expect(preview.canApply).toBe(false);
    expect(preview.reason).toContain('Messages → Responses');
    expect(preview.reusePath).toBe('none');

    const legacy = mapAdapterApplyPlan({
      analysis: {
        route: 'config_sync',
        support: 'stable',
        reason: '显式 Anthropic API Key 可预览为 Pi 的配置同步。',
        actions: [],
        limitations: [],
        evidence: [],
      },
      targetAgentId: 'pi',
      canApply: false,
      serviceImpact: 'none',
      changes: [],
    });
    expect(legacy.maturity).toBe('none');
    expect(legacy.reason).toBe('显式 Anthropic API Key 可预览为 Pi 的配置同步。');
    expect(legacy.reusePath).toBe('api_endpoint');
  });

  it('maps explicit reuse paths and fails closed for unknown values', () => {
    const legacyLocal = mapAdapterApplyPlan({
      analysis: {
        route: 'local_bridge',
        support: 'experimental',
        reason: 'bridge',
        actions: [],
        limitations: [],
        evidence: [],
      },
      targetAgentId: 'codex',
      canApply: true,
      serviceImpact: 'requires_local_bridge',
      changes: [],
    });
    expect(legacyLocal.reusePath).toBe('local_bridge');

    const legacyUnsupported = mapAdapterApplyPlan({
      analysis: {
        route: 'unsupported',
        support: 'unsupported',
        reason: 'unsupported',
        actions: [],
        limitations: [],
        evidence: [],
      },
      targetAgentId: 'claude',
      canApply: false,
      serviceImpact: 'none',
      changes: [],
    });
    expect(legacyUnsupported.reusePath).toBe('none');

    const nativeSubscription = mapAdapterApplyPlan({
      analysis: {
        route: 'config_sync',
        support: 'experimental',
        reason: 'preview',
        actions: [],
        limitations: [],
        evidence: [],
      },
      targetAgentId: 'pi',
      canApply: false,
      reusePath: 'native_subscription',
      serviceImpact: 'none',
      changes: [],
    });
    expect(nativeSubscription.reusePath).toBe('native_subscription');

    const unknown = mapAdapterApplyPlan({
      analysis: {
        route: 'native_endpoint',
        support: 'stable',
        reason: 'ok',
        actions: [],
        limitations: [],
        evidence: [],
      },
      targetAgentId: 'claude',
      canApply: true,
      reusePath: 'future_path',
      serviceImpact: 'none',
      changes: [],
    });
    expect(unknown.reusePath).toBe('none');
  });

  it('maps a default pool list and never keeps a hub token field', () => {
    const listed = mapDefaultRoutePoolList({
      enabled: true,
      pools: [{
        id: 'pool-1',
        targetAgentId: 'codex',
        surface: 'responses',
        dialect: 'codex',
        v2Enrolled: true,
        gatewayPort: 43121,
        members: [{
          sourceKind: 'account',
          sourceId: 'oauth-1',
          displayLabel: 'user@example.com',
          refreshTokenTail: '**5678',
          enabled: true,
        }],
        listedModels: ['kimi-k2.5'],
      }],
    });
    expect(listed.enabled).toBe(true);
    expect(listed.chatCompletionsShared).toBe(false);
    expect(listed.pools[0]?.gatewayPort).toBe(43121);
    expect(listed.pools[0]?.members[0]).toMatchObject({
      sourceId: 'oauth-1',
      displayLabel: 'user@example.com',
      refreshTokenTail: '**5678',
    });
    expect(JSON.stringify(listed)).not.toContain('hubToken');
  });

  it('maps loopback entry keys for the tokens page', () => {
    expect(mapLocalTokenRecord({ poolId: 'pool-1', token: 'ahb_secret' })).toEqual({
      poolId: 'pool-1',
      token: 'ahb_secret',
    });
  });

  it('maps a loopback entry-key probe result', () => {
    expect(mapLocalTokenProbeResult({
      outcome: 'unauthorized',
      httpStatus: 401,
      latencyMs: 9.4,
      upstreamStatus: '  ',
      requestUrl: 'http://127.0.0.1:8123/v1/chat/completions',
      requestMethod: 'POST',
      requestBody: '{"model":"kimi"}',
      responseBody: '{"error":"invalid_api_key"}',
      errorMessage: '  ',
    })).toEqual({
      outcome: 'unauthorized',
      httpStatus: 401,
      latencyMs: 9,
      upstreamStatus: null,
      requestUrl: 'http://127.0.0.1:8123/v1/chat/completions',
      requestMethod: 'POST',
      requestBody: '{"model":"kimi"}',
      responseBody: '{"error":"invalid_api_key"}',
      errorMessage: null,
    });
    expect(mapLocalTokenProbeResult({}).outcome).toBe('unreachable');
  });
});
