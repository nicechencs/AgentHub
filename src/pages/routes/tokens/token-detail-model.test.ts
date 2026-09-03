import { describe, expect, it } from 'vitest';
import {
  buildTokenDetailCopyRows,
  formatTokenRelative,
  localTokenEntryRunning,
  localTokenTestGate,
  localTokenTestInputText,
  localTokenTestModels,
  localTokenTestOutputText,
  localTokenTestResultLabel,
  localTokenTestResultTone,
  localTokenTestWindowSummary,
  tokenEndpointParts,
  tokenLastPageDisplay,
  tokenUsageDisplay,
} from './token-detail-model';
import type { LocalTokenRow } from './tokens-model';

function row(partial: Partial<LocalTokenRow> = {}): LocalTokenRow {
  return {
    id: 'pool-kimi',
    poolBacked: true,
    profileId: 'bridge-1',
    name: 'kimi · /v1/chat/completions',
    kind: 'chat_completions',
    path: '/v1/chat/completions',
    endpoint: '127.0.0.1:8123',
    state: 'running',
    token: 'ahb_secret',
    maskedToken: 'ahb_••••cret',
    unavailable: false,
    targetAgentId: 'kimi',
    profileIds: ['bridge-1'],
    lastPath: '/v1/models',
    lastRequestAt: new Date().toISOString(),
    usage: {
      requestCount: 3,
      inputTokens: 1500,
      outputTokens: 200,
      cachedInputTokens: 0,
    },
    listedModels: ['kimi-k2', 'gpt-4o'],
    ...partial,
  };
}

describe('token-detail-model', () => {
  it('exposes a copyable endpoint URL and a masked token until revealed', () => {
    const masked = buildTokenDetailCopyRows(row(), false);
    expect(tokenEndpointParts(row()).href).toBe('http://127.0.0.1:8123/v1/chat/completions');
    expect(masked.find((item) => item.id === 'type')).toMatchObject({
      display: 'Chat Completions',
      copyValue: null,
    });
    expect(masked.find((item) => item.id === 'endpoint')).toMatchObject({
      copyValue: 'http://127.0.0.1:8123/v1/chat/completions',
      pending: false,
    });
    expect(masked.find((item) => item.id === 'token')).toMatchObject({
      display: 'ahb_••••cret',
      copyValue: 'ahb_secret',
    });
    const revealed = buildTokenDetailCopyRows(row(), true);
    expect(revealed.find((item) => item.id === 'token')?.display).toBe('ahb_secret');
  });

  it('leaves the token field empty when no key exists yet', () => {
    const copies = buildTokenDetailCopyRows(row({
      token: null,
      maskedToken: null,
    }), false);
    expect(copies.find((item) => item.id === 'token')).toMatchObject({
      display: '',
      copyValue: null,
      pending: true,
    });
  });

  it('withholds the token when the runtime is unavailable', () => {
    const copies = buildTokenDetailCopyRows(row({
      unavailable: true,
      token: 'ahb_secret',
      maskedToken: 'ahb_••••cret',
    }), true);
    expect(copies.find((item) => item.id === 'token')?.copyValue).toBeNull();
  });

  it('shows last visited page and token usage',
    () => {
      expect(tokenLastPageDisplay(row())).toBe('/v1/models');
      expect(tokenLastPageDisplay(row({ lastPath: null }))).toBe('');
      expect(tokenUsageDisplay(row().usage)).toBe('1.5K in / 200 out');
      expect(tokenUsageDisplay({
        requestCount: 0,
        inputTokens: 0,
        outputTokens: 0,
        cachedInputTokens: 0,
      })).toBe('');
      expect(formatTokenRelative(new Date().toISOString())).toBe('刚刚');
    });

  it('enables the one-click test only when the entry key and port are ready', () => {
    expect(localTokenTestGate(row()).enabled).toBe(true);
    expect(localTokenTestGate(row({ token: null })).enabled).toBe(false);
    expect(localTokenTestGate(row({ endpoint: null })).reason).toBe('本机转发还没启动');
    expect(localTokenTestGate(row({ unavailable: true })).reason).toBe('状态不可用');
  });

  it('labels probe outcomes without exposing the key', () => {
    expect(localTokenTestResultLabel({ outcome: 'ok', latencyMs: 12 })).toBe('连上模型了 · 12ms');
    expect(localTokenTestResultLabel({ outcome: 'unauthorized', latencyMs: 8 })).toBe('入口 Key 无效');
    expect(localTokenTestResultLabel({ outcome: 'unreachable', latencyMs: 30 })).toBe('端点连不上');
    expect(localTokenTestResultLabel({ outcome: 'rejected', latencyMs: 40 })).toBe('模型没通');
    expect(localTokenTestResultTone('ok')).toBe('success');
    expect(localTokenTestResultTone('unauthorized')).toBe('danger');
  });

  it('shows request input and connection errors in the test window', () => {
    expect(localTokenEntryRunning(row())).toBe(true);
    expect(localTokenEntryRunning(row({ state: 'stopped' }))).toBe(false);
    expect(localTokenTestInputText(row())).toContain('POST http://127.0.0.1:8123/v1/chat/completions');
    expect(localTokenTestInputText(row())).toContain('ahb_••••cret');
    expect(localTokenTestInputText(row())).not.toContain('ahb_secret');
    expect(localTokenTestInputText(row(), {
      requestUrl: 'http://127.0.0.1:8123/v1/chat/completions',
      requestMethod: 'POST',
      requestBody: '{"model":"kimi-k2","messages":[{"role":"user","content":"ping"}]}',
    })).toContain('"content":"ping"');
    expect(localTokenTestOutputText({
      outcome: 'unreachable',
      httpStatus: null,
      latencyMs: 8,
      upstreamStatus: null,
      requestUrl: 'http://127.0.0.1:8123/v1/chat/completions',
      requestMethod: 'POST',
      requestBody: null,
      responseBody: null,
      errorMessage: 'Connection refused',
    }, { running: false, testing: false })).toBe('本机转发还没启动\nConnection refused');
    expect(localTokenTestOutputText({
      outcome: 'ok',
      httpStatus: 200,
      latencyMs: 1,
      upstreamStatus: null,
      requestUrl: 'http://127.0.0.1:8123/v1/chat/completions',
      requestMethod: 'POST',
      requestBody: '{}',
      responseBody: '{"choices":[{"message":{"content":"ok"}}]}',
      errorMessage: null,
    }, { running: false, testing: false })).toBe('HTTP 200\n{"choices":[{"message":{"content":"ok"}}]}');
    expect(localTokenTestWindowSummary({ outcome: 'ok', latencyMs: 1 }, false)).toBe('连上模型了 · 1ms');
    expect(localTokenTestWindowSummary({ outcome: 'unreachable', latencyMs: 2030 }, false))
      .toBe('端点连不上 · 耗时 2030ms');
  });

  it('lists models for the test dropdown',
    () => {
      expect(localTokenTestModels(row())).toEqual(['kimi-k2', 'gpt-4o']);
      expect(localTokenTestModels(row({ listedModels: [' ', 'kimi-k2', ''] }))).toEqual(['kimi-k2']);
    });
});
