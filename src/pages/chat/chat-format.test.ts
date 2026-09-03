import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import type { ChatMessage } from '@/lib/types';
import {
  chatModelOptions,
  extractModel,
  extractPiDefaultProvider,
  extractPiSlotModels,
  formatChatSessionRecord,
  formatDurationMs,
  formatStepInput,
  isRetiredChatModel,
  localizeChatFailure,
  officialPiModelsBaseUrl,
  piChatModelOptions,
  shouldFetchChatRemoteModels,
  thinkingChromeLabel,
} from './chat-format';

const t = createTranslator('zh');

function chatMsg(
  partial: Partial<ChatMessage> & Pick<ChatMessage, 'id' | 'role' | 'content'>,
): ChatMessage {
  return {
    conversationId: 'c1',
    turn: 1,
    status: 'ok',
    durationMs: 0,
    createdAt: '2026-08-16T00:00:00.000Z',
    ...partial,
  };
}

describe('formatChatSessionRecord', () => {
  it('formats user and agent turns as copyable text', () => {
    expect(
      formatChatSessionRecord(
        [
          {
            turn: 1,
            user: chatMsg({ id: 'u1', role: 'user', content: '修登录' }),
            agents: [
              chatMsg({
                id: 'a1',
                role: 'assistant',
                agentId: 'claude',
                content: '先看 token。',
              }),
            ],
          },
        ],
        '你',
      ),
    ).toBe('你\n修登录\n\nClaude Code\n先看 token。');
  });
});

describe('chat-format tool payload', () => {
  it('pretty-prints JSON tool input and does not mask session payloads', () => {
    expect(formatStepInput({ path: '/tmp', token: 'sk-live-not-masked' })).toContain(
      'sk-live-not-masked',
    );
    expect(formatStepInput('{"a":1,"b":{"c":2}}')).toBe(
      '{\n  "a": 1,\n  "b": {\n    "c": 2\n  }\n}',
    );
    expect(formatStepInput('plain text')).toBe('plain text');
    expect(formatStepInput(null)).toBeNull();
  });
});

describe('chat-format thinking chrome', () => {
  it('formatDurationMs uses ms / s / m s', () => {
    expect(formatDurationMs(12)).toBe('12ms');
    expect(formatDurationMs(1500)).toBe('1.5s');
    expect(formatDurationMs(65_000)).toBe('1m 5s');
  });

  it('thinkingChromeLabel matches live / thought-for / done copy', () => {
    expect(thinkingChromeLabel(false, 0, t)).toBe('思考中 · 0ms');
    expect(thinkingChromeLabel(false, 3200, t)).toBe('思考中 · 3.2s');
    expect(thinkingChromeLabel(true, 3200, t)).toBe('思考了 3.2s');
    expect(thinkingChromeLabel(true, 0, t)).toBe('思考完成');
  });
});

describe('chat model options', () => {
  it('drops retired stealth backup and keeps a live current model', () => {
    expect(isRetiredChatModel('stealth/ox-alpha')).toBe(true);
    expect(extractModel('{"defaultModel":"stealth/ox-alpha"}')).toBe('stealth/ox-alpha');
    expect(chatModelOptions(['stealth/ox-alpha', 'openrouter/auto'], 'stealth/ox-alpha')).toEqual([
      'openrouter/auto',
    ]);
    expect(chatModelOptions(['grok-4.5', 'gpt-4o'], 'kimi-k2')).toEqual(['grok-4.5', 'gpt-4o']);
  });

  it('localizes leftover API Key and retired-model failures without dumping English', () => {
    expect(localizeChatFailure('Missing environment variable: `OPENROUTER_API_KEY`.', t)).toContain('重试');
    expect(localizeChatFailure('Missing environment variable: `OPENROUTER_API_KEY`.', t)).not.toContain('OPENROUTER');
    expect(
      localizeChatFailure('error: Model "kimi-k2" is not supported by any configured account in this group', t),
    ).toContain('换一个模型');
    expect(
      localizeChatFailure('404: {"message":"Stealth Ox Alpha testing period","code":404}', t),
    ).toContain('下架');
    expect(
      localizeChatFailure(
        'OAuth refresh failed for xai: xAI OAuth token refresh failed (HTTP 400): invalid_grant: Invalid or unknown refresh token',
        t,
      ),
    ).toBe('这份登录已失效，请重新登录后重试。');
    expect(
      localizeChatFailure(
        'OpenAI API error (400): 400 "Model grok-code-fast-1 does not support parameter reasoningEffort."',
        t,
      ),
    ).toBe('这个模型不支持当前思考设置。请点重试。');
    const tEn = createTranslator('en');
    expect(
      localizeChatFailure('Missing environment variable: `OPENROUTER_API_KEY`.', tEn),
    ).not.toMatch(/[\u4e00-\u9fff]/);
    expect(
      localizeChatFailure(
        'OAuth refresh failed for xai: invalid_grant: Invalid or unknown refresh token',
        tEn,
      ),
    ).toBe('This login has expired. Sign in again, then retry.');
  });

  it('reads Pi slot models from the current defaultProvider, not a leftover URL slot', () => {
    const text = JSON.stringify({
      settings: { defaultProvider: 'xai' },
      models: {
        providers: {
          openrouter: {
            baseUrl: 'https://openrouter.ai/api/v1',
            models: [{ id: 'openrouter/auto' }],
          },
          xai: {
            models: [{ id: 'grok-4' }, { id: 'grok-code-fast-1' }, { id: 'stealth/ox-alpha' }],
          },
        },
      },
    });
    expect(extractPiSlotModels(text)).toEqual(['grok-4', 'grok-code-fast-1']);
  });

  it('does not skip the remote catalog fetch for a Pi official xAI login', () => {
    expect(extractPiDefaultProvider('{"settings":{"defaultProvider":"xai"}}')).toBe('xai');
    expect(officialPiModelsBaseUrl('xai')).toBe('https://api.x.ai/v1');
    expect(officialPiModelsBaseUrl('openrouter')).toBe('');
    expect(shouldFetchChatRemoteModels('prov-pi', 'https://api.x.ai/v1')).toBe(true);
    expect(shouldFetchChatRemoteModels('prov-pi', '')).toBe(false);
  });

  it('uses the official xAI remote catalog for Pi 换模型, not leftover defaultModel', () => {
    const official = ['grok-4.3', 'grok-4.5', 'grok-4.6', 'grok-build-0.1'];
    expect(
      piChatModelOptions({
        remoteModels: official,
        liveModels: ['grok-code-fast-1'],
        envelopeModels: ['grok-code-fast-1'],
        currentModel: 'grok-code-fast-1',
      }),
    ).toEqual(official);
    expect(
      piChatModelOptions({
        remoteModels: official,
        liveModels: [],
        envelopeModels: ['grok-4', 'grok-code-fast-1'],
        currentModel: 'grok-code-fast-1',
      }),
    ).not.toContain('grok-code-fast-1');
  });
});
