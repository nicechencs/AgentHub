import { describe, expect, it } from 'vitest';
import {
  mapAdapterApplyResult,
  mapAdapterBridgeStatusDto,
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
      profile: { route: 'local_bridge', localPort: 43123 },
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
    });
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
});
