import { describe, expect, it } from 'vitest';
import {
  API_VENDORS,
  buildPoolApiSaveItems,
  defaultSelectedApiTypes,
  detectedApiChoiceTypes,
  matchApiVendor,
  poolApiChoices,
  poolApiRecordName,
  poolSurfaceForApiChoice,
  primaryVendorUrl,
  resolveEndpointUrl,
  vendorServiceUrls,
} from './api-access-model';

const ALL_AGENTS = ['claude', 'codex', 'grok'] as const;

describe('matchApiVendor', () => {
  it('matches DeepSeek by its OpenAI-compatible URL and Anthropic URL', () => {
    expect(matchApiVendor('https://api.deepseek.com')?.id).toBe('deepseek');
    expect(matchApiVendor('https://api.deepseek.com/anthropic')?.id).toBe('deepseek');
    expect(matchApiVendor('https://api.deepseek.com/v1')?.id).toBe('deepseek');
  });

  it('distinguishes Qwen regions by host', () => {
    expect(matchApiVendor('https://dashscope.aliyuncs.com/compatible-mode/v1')?.id).toBe('qwen-cn');
    expect(matchApiVendor('https://dashscope-intl.aliyuncs.com/apps/anthropic')?.id).toBe('qwen-sg');
  });

  it('returns null for an unknown host', () => {
    expect(matchApiVendor('https://api.example.com/v1')).toBeNull();
  });
});

describe('vendor endpoint URLs', () => {
  it('uses a different Anthropic URL for DeepSeek messages', () => {
    const vendor = API_VENDORS.find((item) => item.id === 'deepseek');
    expect(vendor).toBeTruthy();
    expect(resolveEndpointUrl(vendor ?? null, 'claudeMessages', 'https://api.deepseek.com')).toBe(
      'https://api.deepseek.com/anthropic',
    );
    expect(resolveEndpointUrl(vendor ?? null, 'openaiChatCompletions', 'https://api.deepseek.com/anthropic')).toBe(
      'https://api.deepseek.com',
    );
  });

  it('falls back to the entered URL when the vendor has no row for that type', () => {
    const vendor = API_VENDORS.find((item) => item.id === 'anthropic') ?? null;
    expect(resolveEndpointUrl(vendor, 'openaiChatCompletions', 'https://api.anthropic.com/v1')).toBe(
      'https://api.anthropic.com/v1',
    );
  });

  it('picks the OpenAI-compatible URL as the vendor default when present', () => {
    const deepseek = API_VENDORS.find((item) => item.id === 'deepseek');
    const anthropic = API_VENDORS.find((item) => item.id === 'anthropic');
    expect(primaryVendorUrl(deepseek!)).toBe('https://api.deepseek.com');
    expect(primaryVendorUrl(anthropic!)).toBe('https://api.anthropic.com');
  });

  it('lists a vendor’s distinct service URLs so the address can be chosen after the vendor', () => {
    expect(vendorServiceUrls(API_VENDORS.find((item) => item.id === 'deepseek')!)).toEqual([
      'https://api.deepseek.com',
      'https://api.deepseek.com/anthropic',
    ]);
    expect(vendorServiceUrls(API_VENDORS.find((item) => item.id === 'anthropic')!)).toEqual([
      'https://api.anthropic.com',
    ]);
  });
});

describe('defaultSelectedApiTypes', () => {
  it('checks every available type the vendor supports', () => {
    const vendor = API_VENDORS.find((item) => item.id === 'deepseek') ?? null;
    const selected = defaultSelectedApiTypes(vendor, poolApiChoices(ALL_AGENTS));
    expect([...selected].sort()).toEqual(['claudeMessages', 'openaiChatCompletions', 'openaiResponses']);
  });

  it('skips types whose Agent is not installed', () => {
    const vendor = API_VENDORS.find((item) => item.id === 'deepseek') ?? null;
    const selected = defaultSelectedApiTypes(vendor, poolApiChoices(['claude']));
    expect([...selected]).toEqual(['claudeMessages']);
  });
});

describe('buildPoolApiSaveItems', () => {
  it('creates one record per checked type with that type’s URL', () => {
    const vendor = API_VENDORS.find((item) => item.id === 'deepseek') ?? null;
    const items = buildPoolApiSaveItems(
      poolApiChoices(ALL_AGENTS),
      new Set(['claudeMessages', 'openaiChatCompletions']),
      vendor,
      'https://api.deepseek.com',
    );
    expect(items.map((item) => [item.choice.type, item.baseUrl])).toEqual([
      ['claudeMessages', 'https://api.deepseek.com/anthropic'],
      ['openaiChatCompletions', 'https://api.deepseek.com'],
    ]);
  });

  it('uses the entered URL for a custom host', () => {
    const items = buildPoolApiSaveItems(
      poolApiChoices(ALL_AGENTS),
      new Set(['openaiChatCompletions', 'claudeMessages']),
      null,
      'https://api.example.com/v1/',
    );
    expect(items.map((item) => item.baseUrl)).toEqual([
      'https://api.example.com/v1',
      'https://api.example.com/v1',
    ]);
  });
});

describe('detectedApiChoiceTypes', () => {
  it('maps probed surfaces onto the matching API choices', () => {
    expect(detectedApiChoiceTypes(['messages', 'chat_completions'])).toEqual([
      'claudeMessages',
      'openaiChatCompletions',
    ]);
  });
});

describe('poolSurfaceForApiChoice', () => {
  it('maps each API endpoint to its local entry surface', () => {
    expect(poolSurfaceForApiChoice({ endpoint: '/v1/messages' })).toBe('messages');
    expect(poolSurfaceForApiChoice({ endpoint: '/v1/responses' })).toBe('responses');
    expect(poolSurfaceForApiChoice({ endpoint: '/v1/chat/completions' })).toBe('chat_completions');
  });
});

describe('poolApiRecordName', () => {
  it('includes the host and endpoint so multiple records stay distinguishable', () => {
    expect(poolApiRecordName('https://api.deepseek.com/anthropic', '/v1/messages')).toBe(
      'api.deepseek.com /v1/messages',
    );
  });
});
