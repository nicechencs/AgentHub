import { describe, expect, it } from 'vitest';
import { kimiProviderTypeForUrl } from './kimi-provider-type';

describe('kimiProviderTypeForUrl', () => {
  it('maps Messages, Responses, official Kimi, and Chat Completions roots', () => {
    expect(kimiProviderTypeForUrl('')).toBe('openai');
    expect(kimiProviderTypeForUrl('https://api.z.ai/api/anthropic')).toBe('anthropic');
    expect(kimiProviderTypeForUrl('http://127.0.0.1:9/v1/messages')).toBe('anthropic');
    expect(kimiProviderTypeForUrl('http://127.0.0.1:9/v1/responses')).toBe('openai_responses');
    expect(kimiProviderTypeForUrl('https://api.moonshot.cn/v1')).toBe('kimi');
    expect(kimiProviderTypeForUrl('https://api.moonshot.ai/v1')).toBe('kimi');
    expect(kimiProviderTypeForUrl('https://api.kimi.com/coding/v1')).toBe('openai');
    expect(kimiProviderTypeForUrl('https://relay.example.com/v1')).toBe('openai');
  });
});
