import { describe, expect, it } from 'vitest';
import { applyFormVars, EMPTY_FORM_VARS, REDACTED_MARKER } from '../index';
import {
  defaultModelForAgent,
  FALLBACK_CUSTOM_MODEL,
  maskApiKeyLast4,
  openaiModelsUrl,
  parseOpenAiModelList,
  resolveModelForSave,
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

describe('shouldFetchRemoteModels', () => {
  it('requires custom mode, http(s) URL, and a real key', () => {
    expect(
      shouldFetchRemoteModels({
        useOfficial: false,
        baseUrl: 'https://mytokens.cc',
        apiKey: 'sk-live-abcdefgh',
      }),
    ).toBe(true);
    expect(
      shouldFetchRemoteModels({
        useOfficial: true,
        baseUrl: 'https://mytokens.cc',
        apiKey: 'sk-live-abcdefgh',
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
});
