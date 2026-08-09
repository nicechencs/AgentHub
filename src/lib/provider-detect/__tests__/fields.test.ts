import { describe, expect, it } from 'vitest';
import {
  applyFormVars,
  extractFormVars,
  formFieldVisibility,
  looksRedactedOrPlaceholder,
  REDACTED_MARKER,
} from '../index';

describe('provider-detect fields', () => {
  it('detects redacted secrets and backend marker', () => {
    expect(looksRedactedOrPlaceholder('sk-••••9f2a')).toBe(true);
    expect(looksRedactedOrPlaceholder(REDACTED_MARKER)).toBe(true);
    expect(looksRedactedOrPlaceholder('sk-live-abc123xyz')).toBe(false);
  });

  it('extracts Claude env + top-level model', () => {
    const src = JSON.stringify(
      {
        env: {
          ANTHROPIC_BASE_URL: 'https://relay.example.com',
          ANTHROPIC_AUTH_TOKEN: 'sk-live-secret',
        },
        model: 'opus',
      },
      null,
      2,
    );
    const vars = extractFormVars('claude', src, 'json');
    expect(vars.baseUrl).toBe('https://relay.example.com');
    expect(vars.apiKey).toBe('sk-live-secret');
    expect(vars.model).toBe('opus');
    expect(vars.claudeAuthEnv).toBe('ANTHROPIC_AUTH_TOKEN');
  });

  it('applies Claude fields and writes *** when apiKey empty', () => {
    const src = JSON.stringify(
      {
        env: {
          ANTHROPIC_BASE_URL: 'https://old.example.com',
          ANTHROPIC_AUTH_TOKEN: 'sk-keep-me',
        },
        model: 'sonnet',
      },
      null,
      2,
    );
    const out = applyFormVars('claude', src, 'json', {
      ...{
        baseUrl: 'https://new.example.com',
        apiKey: '',
        model: 'opus',
        modelOpus: '',
        modelSonnet: '',
        modelHaiku: '',
        modelFable: '',
        modelSubagent: '',
        claudeAuthEnv: 'ANTHROPIC_AUTH_TOKEN' as const,
        reasoningEffort: '',
        wireApi: '',
        providerSlug: 'custom',
      },
    });
    const parsed = JSON.parse(out) as {
      env: Record<string, string>;
      model: string;
    };
    expect(parsed.env.ANTHROPIC_BASE_URL).toBe('https://new.example.com');
    expect(parsed.env.ANTHROPIC_AUTH_TOKEN).toBe(REDACTED_MARKER);
    expect(parsed.model).toBe('opus');
  });

  it('extracts and applies Codex model_providers (key not in TOML)', () => {
    const toml = [
      'model_provider = "crs"',
      'model = "gpt-5"',
      'model_reasoning_effort = "high"',
      '',
      '[model_providers.crs]',
      'name = "crs"',
      'base_url = "https://cc.chenshi.io/openai"',
      'wire_api = "responses"',
      '',
    ].join('\n');

    const vars = extractFormVars('codex', toml, 'toml');
    expect(vars.model).toBe('gpt-5');
    expect(vars.baseUrl).toBe('https://cc.chenshi.io/openai');
    expect(vars.apiKey).toBe('');

    const next = applyFormVars('codex', toml, 'toml', {
      ...vars,
      baseUrl: 'https://new.example.com/openai',
      model: 'gpt-5.1-codex',
      apiKey: 'sk-should-not-land-in-toml',
    });
    expect(next).toContain('model = "gpt-5.1-codex"');
    expect(next).toContain('base_url = "https://new.example.com/openai"');
    expect(next).not.toContain('sk-should-not-land-in-toml');
  });

  it('extracts Grok Build fields from the active nested model table', () => {
    const toml = [
      '[models]',
      'default = "grok"',
      'web_search = "grok"',
      '',
      '[model."grok"]',
      'model = "grok-4.5"',
      'base_url = "https://relay.example.com/v1"',
      'api_key = "sk-grok-test-abcdefghijklmnop"',
      'api_backend = "responses"',
      'context_window = 1000000',
      'supports_backend_search = true',
      '',
    ].join('\n');

    const vars = extractFormVars('grok', toml, 'toml');
    expect(vars.model).toBe('grok-4.5');
    expect(vars.baseUrl).toBe('https://relay.example.com/v1');
    expect(vars.apiKey).toBe('sk-grok-test-abcdefghijklmnop');

    const next = applyFormVars('grok', toml, 'toml', {
      ...vars,
      model: 'grok-4.5-latest',
      baseUrl: 'https://new-relay.example.com/v1',
      apiKey: '',
    });
    expect(next).toContain('[models]');
    expect(next).toContain('[model."grok"]');
    expect(next).toContain('model = "grok-4.5-latest"');
    expect(next).toContain('base_url = "https://new-relay.example.com/v1"');
    expect(next).toContain('api_backend = "responses"');
    expect(next).toContain('context_window = 1000000');
    expect(next).toContain('supports_backend_search = true');
    expect(next).toContain('api_key = "***"');
  });

  it('extracts and applies Pi provider fields without falling back to Claude env', () => {
    const source = JSON.stringify({
      providers: {
        custom: {
          baseUrl: 'https://old.example.com/v1',
          api: 'openai-responses',
          apiKey: 'sk-pi-secret',
          models: [{ id: 'old-model', name: 'Old Model' }],
        },
        keep: { baseUrl: 'https://keep.example.com', models: [{ id: 'keep' }] },
      },
    });
    const vars = extractFormVars('pi', source, 'json');
    expect(vars.baseUrl).toBe('https://old.example.com/v1');
    expect(vars.apiKey).toBe('sk-pi-secret');
    expect(vars.model).toBe('old-model');

    const next = applyFormVars('pi', source, 'json', {
      ...vars,
      baseUrl: 'https://new.example.com/v1',
      model: 'new-model',
      apiKey: '',
    });
    const parsed = JSON.parse(next) as {
      providers: Record<string, { baseUrl?: string; apiKey?: string; models: { id: string }[] }>;
    };
    expect(parsed.providers.custom.baseUrl).toBe('https://new.example.com/v1');
    expect(parsed.providers.custom.apiKey).toBe(REDACTED_MARKER);
    expect(parsed.providers.custom.models[0]?.id).toBe('new-model');
    expect(parsed.providers.keep.models[0]?.id).toBe('keep');
  });

  it('keeps Pi live-config envelope metadata while editing nested models', () => {
    const source = JSON.stringify({
      settings: { defaultProvider: 'custom' },
      models: { providers: { custom: { models: [{ id: 'old' }] } } },
      paths: { models: 'models.json' },
    });
    const next = applyFormVars('pi', source, 'json', {
      ...extractFormVars('pi', source, 'json'),
      model: 'new',
      apiKey: 'sk-new',
    });
    const parsed = JSON.parse(next) as {
      settings: { defaultProvider: string };
      models: { providers: { custom: { models: { id: string }[] } } };
      paths: { models: string };
    };
    expect(parsed.settings.defaultProvider).toBe('custom');
    expect(parsed.models.providers.custom.models[0]?.id).toBe('new');
    expect(parsed.paths.models).toBe('models.json');
  });

  it('extracts and applies WorkBuddy models.json fields', () => {
    const source = JSON.stringify({
      models: [
        {
          id: 'old-model',
          name: 'Old Model',
          url: 'https://old.example.com/v1/chat/completions',
          apiKey: 'sk-workbuddy-secret',
        },
      ],
      availableModels: ['old-model'],
    });
    const vars = extractFormVars('workbuddy', source, 'json');
    expect(vars.baseUrl).toBe('https://old.example.com/v1/chat/completions');
    expect(vars.apiKey).toBe('sk-workbuddy-secret');
    expect(vars.model).toBe('old-model');

    const next = applyFormVars('workbuddy', source, 'json', {
      ...vars,
      baseUrl: 'https://new.example.com/v1/chat/completions',
      model: 'new-model',
      apiKey: '',
    });
    const parsed = JSON.parse(next) as {
      models: { url?: string; apiKey?: string; id: string }[];
      availableModels: string[];
    };
    expect(parsed.models[0]?.url).toBe('https://new.example.com/v1/chat/completions');
    expect(parsed.models[0]?.apiKey).toBe(REDACTED_MARKER);
    expect(parsed.models[0]?.id).toBe('new-model');
    expect(parsed.availableModels).toEqual(['new-model']);
  });

  it('keeps WorkBuddy live-config envelope metadata while editing nested models', () => {
    const source = JSON.stringify({
      settings: { sandbox: true },
      models: { models: [{ id: 'old', url: 'https://old.example.com' }] },
      mcp: { servers: {} },
    });
    const next = applyFormVars('workbuddy', source, 'json', {
      ...extractFormVars('workbuddy', source, 'json'),
      model: 'new',
      apiKey: 'sk-new',
    });
    const parsed = JSON.parse(next) as {
      settings: { sandbox: boolean };
      models: { models: { id: string }[] };
      mcp: { servers: object };
    };
    expect(parsed.settings.sandbox).toBe(true);
    expect(parsed.models.models[0]?.id).toBe('new');
    expect(parsed.mcp.servers).toEqual({});
  });

  it('keeps *** for untouched opaque TOML content', () => {
    const out = applyFormVars('codex', REDACTED_MARKER, 'toml', {
      baseUrl: '',
      apiKey: '',
      model: '',
      modelOpus: '',
      modelSonnet: '',
      modelHaiku: '',
      modelFable: '',
      modelSubagent: '',
      claudeAuthEnv: 'ANTHROPIC_AUTH_TOKEN',
      reasoningEffort: '',
      wireApi: '',
      providerSlug: 'custom',
    });
    expect(out).toBe(REDACTED_MARKER);
  });

  it('reads and writes Claude opus/sonnet/haiku/fable model slots', () => {
    const src = JSON.stringify(
      {
        env: {
          ANTHROPIC_BASE_URL: 'https://relay.example.com',
          ANTHROPIC_AUTH_TOKEN: 'sk-test',
          ANTHROPIC_MODEL: 'main-id',
          ANTHROPIC_DEFAULT_OPUS_MODEL: 'opus-id',
          ANTHROPIC_DEFAULT_SONNET_MODEL: 'sonnet-id',
          ANTHROPIC_DEFAULT_HAIKU_MODEL: 'haiku-id',
          ANTHROPIC_DEFAULT_FABLE_MODEL: 'fable-id',
          CLAUDE_CODE_SUBAGENT_MODEL: 'sub-id',
        },
      },
      null,
      2,
    );
    const vars = extractFormVars('claude', src, 'json');
    expect(vars.model).toBe('main-id');
    expect(vars.modelOpus).toBe('opus-id');
    expect(vars.modelSonnet).toBe('sonnet-id');
    expect(vars.modelHaiku).toBe('haiku-id');
    expect(vars.modelFable).toBe('fable-id');
    expect(vars.modelSubagent).toBe('sub-id');

    const out = applyFormVars('claude', '{}', 'json', {
      ...vars,
      modelOpus: 'new-opus',
      modelSonnet: 'new-sonnet',
    });
    const env = JSON.parse(out).env as Record<string, string>;
    expect(env.ANTHROPIC_DEFAULT_OPUS_MODEL).toBe('new-opus');
    expect(env.ANTHROPIC_DEFAULT_SONNET_MODEL).toBe('new-sonnet');
    expect(env.ANTHROPIC_DEFAULT_HAIKU_MODEL).toBe('haiku-id');
    expect(env.CLAUDE_CODE_SUBAGENT_MODEL).toBe('sub-id');
  });

  it('shows core fields for all agents', () => {
    for (const id of ['claude', 'codex', 'kimi', 'grok'] as const) {
      const v = formFieldVisibility(id);
      expect(v.baseUrl).toBe(true);
      expect(v.apiKey).toBe(true);
      expect(v.model).toBe(true);
    }
  });
});

