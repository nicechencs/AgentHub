import { describe, expect, it } from 'vitest';
import { applyFormVars, EMPTY_FORM_VARS, REDACTED_MARKER } from '../index';
import {
  defaultModelForAgent,
  FALLBACK_CUSTOM_MODEL,
  filterRemoteModelsForAgent,
  looksLikeGrokModel,
  maskApiKeyLast4,
  openaiModelsUrl,
  parseOpenAiModelList,
  resolveModelForSave,
  resolveUpstreamBaseUrl,
  isLivePastedApiKey,
  looksLikeLast4Mask,
  shouldFetchRemoteModels,
  withDefaultModel,
} from '../remote-models';

describe('openaiModelsUrl', () => {
  it('normalizes trailing slashes and a terminal /v1', () => {
    expect(openaiModelsUrl('https://mytokens.cc')).toBe('https://mytokens.cc/v1/models');
    expect(openaiModelsUrl('https://mytokens.cc/')).toBe('https://mytokens.cc/v1/models');
    expect(openaiModelsUrl('https://mytokens.cc/v1')).toBe('https://mytokens.cc/v1/models');
    expect(openaiModelsUrl('https://mytokens.cc/v1/')).toBe('https://mytokens.cc/v1/models');
    expect(openaiModelsUrl('https://openrouter.ai/api/v1')).toBe(
      'https://openrouter.ai/api/v1/models',
    );
    expect(openaiModelsUrl('')).toBe('');
    expect(openaiModelsUrl('   ')).toBe('');
  });

  it('treats /V1 as /v1 (case-insensitive)', () => {
    expect(openaiModelsUrl('https://mytokens.cc/V1/')).toBe('https://mytokens.cc/V1/models');
  });

  it('uses DeepSeek official /models and strips /anthropic', () => {
    expect(openaiModelsUrl('https://api.deepseek.com')).toBe('https://api.deepseek.com/models');
    expect(openaiModelsUrl('https://api.deepseek.com/anthropic')).toBe(
      'https://api.deepseek.com/models',
    );
  });
});

describe('filterRemoteModelsForAgent', () => {
  it('drops grok models from Claude and Kimi lists', () => {
    const ids = ['grok-4.5', 'kimi-k2', 'claude-sonnet-4'];
    expect(filterRemoteModelsForAgent('kimi', ids)).toEqual(['kimi-k2']);
    expect(filterRemoteModelsForAgent('claude', ids)).toEqual(['claude-sonnet-4']);
    expect(filterRemoteModelsForAgent('grok', ids)).toEqual(['grok-4.5']);
  });

  it('treats relay xai/grok-* ids as grok and does not dump them onto Kimi', () => {
    const ids = [
      'xai/grok-4.6',
      'xai/grok-code-fast-1',
      'xai/grok-latest',
    ];
    expect(ids.every((id) => looksLikeGrokModel(id))).toBe(true);
    expect(filterRemoteModelsForAgent('kimi', ids)).toEqual([]);
    expect(filterRemoteModelsForAgent('claude', ids)).toEqual([]);
    expect(filterRemoteModelsForAgent('grok', ids)).toEqual(ids);
  });

  it('keeps moonshot/kimi names when a relay mixes them with xai/grok', () => {
    const ids = ['xai/grok-4.6', 'moonshot-v1-128k', 'kimi-latest'];
    expect(filterRemoteModelsForAgent('kimi', ids)).toEqual(['moonshot-v1-128k', 'kimi-latest']);
  });
});

describe('parseOpenAiModelList', () => {
  it('accepts data object ids, string arrays, models, and a top-level array', () => {
    expect(parseOpenAiModelList({ data: [{ id: 'gpt-4' }, { id: 'gpt-4o-mini' }] })).toEqual([
      'gpt-4',
      'gpt-4o-mini',
    ]);
    expect(parseOpenAiModelList({ data: ['a', 'b'] })).toEqual(['a', 'b']);
    expect(parseOpenAiModelList({ models: ['m1', 'm2'] })).toEqual(['m1', 'm2']);
    expect(parseOpenAiModelList({ models: [{ id: 'x' }, { id: 'y' }] })).toEqual(['x', 'y']);
    expect(parseOpenAiModelList(['one', { id: 'two' }])).toEqual(['one', 'two']);
  });

  it('dedupes first-seen and ignores blanks / missing id', () => {
    expect(
      parseOpenAiModelList({
        data: [{ id: 'keep' }, { id: 'keep' }, { id: '  ' }, { name: 'no-id' }, { id: 'next' }],
      }),
    ).toEqual(['keep', 'next']);
    expect(parseOpenAiModelList({ data: [null, 1, { id: 3 }, ''] })).toEqual([]);
    expect(parseOpenAiModelList(null)).toEqual([]);
    expect(parseOpenAiModelList({})).toEqual([]);
  });
});

describe('maskApiKeyLast4', () => {
  it('returns last 4 only and never the full key', () => {
    const key = 'sk-abcdefghijklmnopqrstuvwxyz';
    expect(maskApiKeyLast4(key)).toBe('wxyz');
    expect(maskApiKeyLast4(key)).not.toBe(key);
    expect(maskApiKeyLast4('short')).toBe('');
    expect(maskApiKeyLast4(REDACTED_MARKER)).toBe('');
    expect(maskApiKeyLast4('')).toBe('');
    expect(maskApiKeyLast4('   ')).toBe('');
  });
});

describe('default / resolve / withDefaultModel', () => {
  it('uses official templates and custom-model fallback', () => {
    expect(defaultModelForAgent('claude')).toBe('sonnet');
    expect(defaultModelForAgent('codex')).toBe('gpt-5.1-codex');
    expect(defaultModelForAgent('kimi')).toBe('kimi-k2');
    expect(defaultModelForAgent('grok')).toBe('grok-4.5');
    expect(defaultModelForAgent('pi')).toBe(FALLBACK_CUSTOM_MODEL);
    expect(defaultModelForAgent('workbuddy')).toBe(FALLBACK_CUSTOM_MODEL);
    expect(defaultModelForAgent('cursor')).toBe(FALLBACK_CUSTOM_MODEL);
  });

  it('keeps a typed model on custom save and leaves empty blank', () => {
    expect(resolveModelForSave('claude', 'opus', false)).toBe('opus');
    expect(resolveModelForSave('claude', '  ', false)).toBe('');
    expect(resolveModelForSave('claude', 'opus', true)).toBe('sonnet');
    expect(resolveModelForSave('pi', '', false)).toBe('');
  });

  it('empty custom model + withDefaultModel does not invent a Claude model id', () => {
    const vars = withDefaultModel(
      'claude',
      {
        ...EMPTY_FORM_VARS,
        baseUrl: 'https://mytokens.cc',
        apiKey: 'sk-test-key',
        model: '',
      },
      false,
    );
    expect(vars.model).toBe('');
    const text = applyFormVars('claude', '{"env":{}}', 'json', vars);
    const parsed = JSON.parse(text) as { model?: string; env: Record<string, string> };
    expect(parsed.model).toBeUndefined();
    expect(parsed.env.ANTHROPIC_MODEL).toBeUndefined();
  });

  it('empty custom model + withDefaultModel does not invent a Kimi default_model', () => {
    const vars = withDefaultModel(
      'kimi',
      {
        ...EMPTY_FORM_VARS,
        baseUrl: 'https://mytokens.cc/v1',
        apiKey: 'sk-test-key',
        model: '',
      },
      false,
    );
    expect(vars.model).toBe('');
    const text = applyFormVars(
      'kimi',
      [
        'default_model = "kimi-k2"',
        '[providers.moonshot]',
        'base_url = "https://mytokens.cc/v1"',
        '',
      ].join('\n'),
      'toml',
      vars,
    );
    expect(text).not.toContain('default_model = "kimi-k2"');
    expect(text).not.toMatch(/default_model\s*=/);
  });

  it('official mode still locks the official model', () => {
    const vars = withDefaultModel(
      'claude',
      { ...EMPTY_FORM_VARS, model: 'opus' },
      true,
    );
    expect(vars.model).toBe('sonnet');
  });
});

const OPENROUTER_JSON_CAMEL = JSON.stringify(
  {
    baseURL: 'https://openrouter.ai/api/v1',
    model: 'stealth/ox-alpha',
  },
  null,
  2,
);

const OPENROUTER_JSON_SNAKE = JSON.stringify(
  {
    base_url: 'https://openrouter.ai/api/v1',
    model: 'stealth/ox-alpha',
  },
  null,
  2,
);

const ANTHROPIC_ENV_JSON = JSON.stringify(
  {
    env: {
      ANTHROPIC_BASE_URL: 'https://relay.example/anthropic',
    },
    model: 'opus',
  },
  null,
  2,
);

const CODEX_TOML_PROVIDERS = [
  'model_provider = "openrouter"',
  'model = "stealth/ox-alpha"',
  '',
  '[model_providers.openrouter]',
  'base_url = "https://openrouter.ai/api/v1"',
  'wire_api = "chat"',
  '',
].join('\n');

describe('resolveUpstreamBaseUrl', () => {
  it('reads camelCase JSON baseURL when the simple field is empty', () => {
    expect(
      resolveUpstreamBaseUrl({
        formBaseUrl: '',
        configText: OPENROUTER_JSON_CAMEL,
        configFormat: 'json',
        agentId: 'codex',
      }),
    ).toBe('https://openrouter.ai/api/v1');
  });

  it('reads snake_case JSON base_url', () => {
    expect(
      resolveUpstreamBaseUrl({
        formBaseUrl: '  ',
        configText: OPENROUTER_JSON_SNAKE,
        configFormat: 'json',
        agentId: 'claude',
      }),
    ).toBe('https://openrouter.ai/api/v1');
  });

  it('reads env ANTHROPIC_BASE_URL from JSON', () => {
    expect(
      resolveUpstreamBaseUrl({
        formBaseUrl: '',
        configText: ANTHROPIC_ENV_JSON,
        configFormat: 'json',
        agentId: 'claude',
      }),
    ).toBe('https://relay.example/anthropic');
  });

  it('reads TOML model_providers base_url', () => {
    expect(
      resolveUpstreamBaseUrl({
        formBaseUrl: '',
        configText: CODEX_TOML_PROVIDERS,
        configFormat: 'toml',
        agentId: 'codex',
      }),
    ).toBe('https://openrouter.ai/api/v1');
  });

  it('lets a real form http(s) field win over advanced config', () => {
    expect(
      resolveUpstreamBaseUrl({
        formBaseUrl: 'https://form.example/v1',
        configText: OPENROUTER_JSON_CAMEL,
        configFormat: 'json',
        agentId: 'codex',
      }),
    ).toBe('https://form.example/v1');
  });

  it('falls through a non-http form field to advanced config', () => {
    expect(
      resolveUpstreamBaseUrl({
        formBaseUrl: 'openrouter.ai',
        configText: OPENROUTER_JSON_CAMEL,
        configFormat: 'json',
        agentId: 'codex',
      }),
    ).toBe('https://openrouter.ai/api/v1');
  });

  it('returns empty when nothing real is present (does not invent a placeholder)', () => {
    expect(
      resolveUpstreamBaseUrl({
        formBaseUrl: '',
        configText: '{}',
        configFormat: 'json',
        agentId: 'codex',
      }),
    ).toBe('');
    expect(
      resolveUpstreamBaseUrl({
        formBaseUrl: 'not-a-url',
        configText: '',
        configFormat: 'toml',
        agentId: 'codex',
      }),
    ).toBe('');
  });
});

describe('shouldFetchRemoteModels', () => {
  it('fetches when the URL lives only in advanced JSON (empty form field)', () => {
    const baseUrl = resolveUpstreamBaseUrl({
      formBaseUrl: '',
      configText: OPENROUTER_JSON_CAMEL,
      configFormat: 'json',
      agentId: 'codex',
    });
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl,
        apiKey: REDACTED_MARKER,
        hasStoredSecret: true,
        savedBaseUrl: baseUrl,
      }),
    ).toBe(true);
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl,
        apiKey: 'sk-live-abcdefgh',
      }),
    ).toBe(true);
    expect(
      shouldFetchRemoteModels({
        useOfficial: true,
        baseUrl,
        apiKey: REDACTED_MARKER,
        hasStoredSecret: true,
      }),
    ).toBe(false);
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl,
        apiKey: '',
      }),
    ).toBe(false);
  });

  it('reads OPENAI_BASE_URL from env-style JSON', () => {
    expect(
      resolveUpstreamBaseUrl({
        formBaseUrl: '',
        configText: JSON.stringify({
          env: { OPENAI_BASE_URL: 'https://openrouter.ai/api/v1' },
        }),
        configFormat: 'json',
        agentId: 'codex',
      }),
    ).toBe('https://openrouter.ai/api/v1');
  });

  it('fetches for a live pasted key on a custom http(s) URL', () => {
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl: 'https://mytokens.cc',
        apiKey: 'sk-live-abcdefgh',
      }),
    ).toBe(true);
  });

  it('is true for an existing custom login with a redacted or empty key', () => {
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl: 'https://mytokens.cc',
        apiKey: REDACTED_MARKER,
        hasStoredSecret: true,
        savedBaseUrl: 'https://mytokens.cc',
      }),
    ).toBe(true);
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl: 'https://openrouter.ai/api/v1',
        apiKey: '',
        hasStoredSecret: true,
        savedBaseUrl: 'https://openrouter.ai/api/v1',
      }),
    ).toBe(true);
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl: 'https://mytokens.cc',
        apiKey: '**abcd',
        hasStoredSecret: true,
        savedBaseUrl: 'https://mytokens.cc',
      }),
    ).toBe(true);
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl: 'https://evil.example/v1',
        apiKey: REDACTED_MARKER,
        hasStoredSecret: true,
        savedBaseUrl: 'https://mytokens.cc',
      }),
    ).toBe(false);
  });

  it('stays false for official, non-http URL, and add-mode empty key', () => {
    expect(
      shouldFetchRemoteModels({
        useOfficial: true,
        baseUrl: 'https://mytokens.cc',
        apiKey: 'sk-live-abcdefgh',
        hasStoredSecret: true,
      }),
    ).toBe(false);
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl: 'mytokens.cc',
        apiKey: 'sk-live-abcdefgh',
      }),
    ).toBe(false);
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl: 'https://mytokens.cc',
        apiKey: REDACTED_MARKER,
      }),
    ).toBe(false);
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl: 'https://mytokens.cc',
        apiKey: '',
      }),
    ).toBe(false);
  });

  it('treats last4 masks as redacted, not a live key', () => {
    expect(looksLikeLast4Mask('**abcd')).toBe(true);
    expect(looksLikeLast4Mask('****wxyz')).toBe(true);
    expect(looksLikeLast4Mask('sk--••••wxyz')).toBe(true);
    expect(isLivePastedApiKey('**abcd')).toBe(false);
    expect(isLivePastedApiKey('sk-live-abcdefgh')).toBe(true);
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl: 'https://mytokens.cc',
        apiKey: '****wxyz',
      }),
    ).toBe(false);
  });
});
