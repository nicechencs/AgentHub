import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  chatModelOptions,
  extractModel,
  extractPiSlotModels,
  formatDurationMs,
  isRetiredChatModel,
  localizeChatFailure,
  thinkingChromeLabel,
} from './chat-format';

const t = createTranslator('zh');

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
    expect(localizeChatFailure('Missing environment variable: `OPENROUTER_API_KEY`.')).toContain('重试');
    expect(localizeChatFailure('Missing environment variable: `OPENROUTER_API_KEY`.')).not.toContain('OPENROUTER');
    expect(
      localizeChatFailure('error: Model "kimi-k2" is not supported by any configured account in this group'),
    ).toContain('换一个模型');
    expect(
      localizeChatFailure('404: {"message":"Stealth Ox Alpha testing period","code":404}'),
    ).toContain('下架');
    expect(
      localizeChatFailure(
        'OAuth refresh failed for xai: xAI OAuth token refresh failed (HTTP 400): invalid_grant: Invalid or unknown refresh token',
      ),
    ).toBe('这份登录已失效，请重新登录后重试。');
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
});
