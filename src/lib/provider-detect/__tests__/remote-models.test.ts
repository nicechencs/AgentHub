import { describe, expect, it } from 'vitest';
import { applyFormVars, EMPTY_FORM_VARS, REDACTED_MARKER } from '../index';
import {
  defaultModelForAgent,
  FALLBACK_CUSTOM_MODEL,
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
    expect(defaultModelForAgent('grok')).toBe('grok-code-fast-1');
    expect(defaultModelForAgent('pi')).toBe(FALLBACK_CUSTOM_MODEL);
    expect(defaultModelForAgent('workbuddy')).toBe(FALLBACK_CUSTOM_MODEL);
    expect(defaultModelForAgent('cursor')).toBe(FALLBACK_CUSTOM_MODEL);
  });

  it('keeps a typed model on custom save and fills empty', () => {
    expect(resolveModelForSave('claude', 'opus', false)).toBe('opus');
    expect(resolveModelForSave('claude', '  ', false)).toBe('sonnet');
    expect(resolveModelForSave('claude', 'opus', true)).toBe('sonnet');
    expect(resolveModelForSave('pi', '', false)).toBe(FALLBACK_CUSTOM_MODEL);
  });

  it('empty model + withDefaultModel + applyFormVars writes the default (claude json)', () => {
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
    expect(vars.model).toBe('sonnet');
    const text = applyFormVars('claude', '{"env":{}}', 'json', vars);
    const parsed = JSON.parse(text) as { model: string; env: Record<string, string> };
    expect(parsed.model).toBe('sonnet');
    expect(parsed.env.ANTHROPIC_MODEL).toBe('sonnet');
  });

  it('empty model + withDefaultModel + applyFormVars writes the default (kimi toml)', () => {
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
    expect(vars.model).toBe('kimi-k2');
    const text = applyFormVars(
      'kimi',
      ['default_model = ""', '[providers.moonshot]', 'base_url = "https://mytokens.cc/v1"', ''].join(
        '\n',
      ),
      'toml',
      vars,
    );
    expect(text).toContain('default_model = "kimi-k2"');
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
      }),
    ).toBe(true);
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl: 'https://openrouter.ai/api/v1',
        apiKey: '',
        hasStoredSecret: true,
      }),
    ).toBe(true);
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl: 'https://mytokens.cc',
        apiKey: '**abcd',
        hasStoredSecret: true,
      }),
    ).toBe(true);
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
