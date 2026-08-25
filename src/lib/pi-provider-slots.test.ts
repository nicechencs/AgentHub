import { describe, expect, it } from 'vitest';
import {
  defaultPiProviderApi,
  isPiAuthJsonSlot,
  isPiPlaceholderBaseUrl,
  PI_AUTH_JSON_SLOTS,
  PI_PLACEHOLDER_BASE_URL,
  PI_PROVIDER_SLOT_OPTIONS,
  piFormRequiresBaseUrl,
  piProviderSlotHint,
} from './pi-provider-slots';

const AUTH_IDS = [
  'anthropic',
  'ant-ling',
  'azure-openai-responses',
  'openai',
  'deepseek',
  'nvidia',
  'google',
  'amazon-bedrock',
] as const;

describe('pi-provider-slots', () => {
  it('lists documented auth.json API-key slots with env vars', () => {
    expect(PI_AUTH_JSON_SLOTS.map((slot) => slot.id)).toEqual([...AUTH_IDS]);
    expect(PI_AUTH_JSON_SLOTS.every((slot) => Boolean(slot.label && slot.envVar))).toBe(true);
  });

  it('treats only the official auth.json table as auth slots', () => {
    for (const id of AUTH_IDS) {
      expect(isPiAuthJsonSlot(id)).toBe(true);
    }
    expect(isPiAuthJsonSlot('  openai  ')).toBe(true);
    expect(isPiAuthJsonSlot('xai')).toBe(false);
    expect(isPiAuthJsonSlot('kimi-for-coding')).toBe(false);
    expect(isPiAuthJsonSlot('custom')).toBe(false);
    expect(isPiAuthJsonSlot('openai-codex')).toBe(false);
  });

  it('builds Select options: auth slots + models.json binds + custom', () => {
    const ids = PI_PROVIDER_SLOT_OPTIONS.map((slot) => slot.id);
    expect(ids).toEqual([...AUTH_IDS, 'xai', 'kimi-for-coding', 'custom']);
    expect(PI_PROVIDER_SLOT_OPTIONS.find((slot) => slot.id === 'xai')?.label).toBe(
      'xAI（自定义）',
    );
    expect(PI_PROVIDER_SLOT_OPTIONS.find((slot) => slot.id === 'kimi-for-coding')?.label).toBe(
      'Kimi For Coding（自定义）',
    );
    expect(PI_PROVIDER_SLOT_OPTIONS.find((slot) => slot.id === 'custom')?.label).toBe(
      '自定义服务',
    );
  });

  it('treats official slots as auth.json-only unless a URL is supplied', () => {
    expect(piFormRequiresBaseUrl('openai')).toBe(false);
    expect(piFormRequiresBaseUrl('custom')).toBe(true);
    expect(piFormRequiresBaseUrl('xai')).toBe(true);
    expect(isPiPlaceholderBaseUrl(PI_PLACEHOLDER_BASE_URL)).toBe(true);
    expect(isPiPlaceholderBaseUrl('https://api.example.com')).toBe(false);
    expect(defaultPiProviderApi('anthropic')).toBe('anthropic-messages');
    expect(defaultPiProviderApi('google')).toBe('google-generative-ai');
    expect(piProviderSlotHint('openai')).toContain('auth.json');
    expect(piProviderSlotHint('custom')).toContain('models.json');
  });
});
