import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  chatModelOptions,
  extractModel,
  formatDurationMs,
  isRetiredChatModel,
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
  });
});
