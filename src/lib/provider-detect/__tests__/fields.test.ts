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
    expect(vars.contextWindow).toBe('auto');
  });

  it('reads and writes Claude compact window without inferring from the model id', () => {
    const src = JSON.stringify(
      {
        env: {
          ANTHROPIC_BASE_URL: 'https://openrouter.ai/api/v1',
          ANTHROPIC_AUTH_TOKEN: 'sk-test',
          ANTHROPIC_MODEL: 'stealth/ox-alpha',
          CLAUDE_CODE_MAX_CONTEXT_TOKENS: '1048576',
          CLAUDE_CODE_AUTO_COMPACT_WINDOW: '1048576',
        },
        model: 'stealth/ox-alpha',
      },
      null,
      2,
    );
    const vars = extractFormVars('claude', src, 'json');
    expect(vars.model).toBe('stealth/ox-alpha');
    expect(vars.contextWindow).toBe('1048576');

    const written = JSON.parse(applyFormVars('claude', '{}', 'json', vars)).env as Record<string, string>;
    expect(written.CLAUDE_CODE_MAX_CONTEXT_TOKENS).toBe('1048576');
    expect(written.CLAUDE_CODE_AUTO_COMPACT_WINDOW).toBe('1048576');

    const omitted = JSON.parse(applyFormVars('claude', src, 'json', {
      ...vars,
      contextWindow: 'auto',
    })).env as Record<string, string>;
    expect(omitted.CLAUDE_CODE_MAX_CONTEXT_TOKENS).toBeUndefined();
    expect(omitted.CLAUDE_CODE_AUTO_COMPACT_WINDOW).toBeUndefined();
    expect(omitted.ANTHROPIC_MODEL).toBe('stealth/ox-alpha');
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
        contextWindow: '',
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
      models: {
        providers: Record<string, { baseUrl?: string; apiKey?: string; models: { id: string }[] }>;
      };
    };
    expect(parsed.models.providers.custom.baseUrl).toBe('https://new.example.com/v1');
    expect(parsed.models.providers.custom.apiKey).toBe(REDACTED_MARKER);
    expect(parsed.models.providers.custom.models[0]?.id).toBe('new-model');
    expect(parsed.models.providers.keep.models[0]?.id).toBe('keep');
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

  it('fails closed for invalid JSON and preserves the exact intermediate text', () => {
    const malformed = '{"providers":{"custom":{"baseUrl":"https://old.example.com"}';
    const out = applyFormVars('pi', malformed, 'json', {
      ...extractFormVars('pi', malformed, 'json'),
      baseUrl: 'https://new.example.com',
      model: 'new-model',
    });
    expect(out).toBe(malformed);
  });

  it('preserves unknown JSON fields while updating structured fields', () => {
    const source = JSON.stringify({
      env: { ANTHROPIC_AUTH_TOKEN: '***', CUSTOM_FLAG: 'keep-me' },
      unknownNested: { enabled: true },
    });
    const out = applyFormVars('claude', source, 'json', {
      ...extractFormVars('claude', source, 'json'),
      baseUrl: 'https://new.example.com',
      model: 'new-model',
    });
    const parsed = JSON.parse(out) as {
      env: Record<string, unknown>;
      unknownNested: { enabled: boolean };
      model: string;
    };
    expect(parsed.env.CUSTOM_FLAG).toBe('keep-me');
    expect(parsed.unknownNested).toEqual({ enabled: true });
    expect(parsed.env.ANTHROPIC_BASE_URL).toBe('https://new.example.com');
    expect(parsed.model).toBe('new-model');
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
      contextWindow: '',
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
      expect(v.providerSlug).toBe(false);
    }
  });

  it('shows providerSlug only for Pi', () => {
    expect(formFieldVisibility('pi').providerSlug).toBe(true);
    expect(formFieldVisibility('claude').providerSlug).toBe(false);
  });

  it('writes Pi auth.json only for an official slot without relay URL', () => {
    const out = applyFormVars('pi', '{}', 'json', {
      baseUrl: '',
      apiKey: 'sk-openai-test',
      model: '',
      modelOpus: '',
      modelSonnet: '',
      modelHaiku: '',
      modelFable: '',
      modelSubagent: '',
      contextWindow: '',
      claudeAuthEnv: 'ANTHROPIC_AUTH_TOKEN',
      reasoningEffort: '',
      wireApi: '',
      providerSlug: 'openai',
    });
    const parsed = JSON.parse(out) as {
      auth: { openai: { type: string; key: string } };
      models?: unknown;
      providers?: unknown;
    };
    expect(parsed.auth.openai).toEqual({ type: 'api_key', key: 'sk-openai-test' });
    expect(parsed.models).toBeUndefined();
    expect(parsed.providers).toBeUndefined();
  });

  it('writes Pi models.json + auth.json when an official slot has a relay URL', () => {
    const out = applyFormVars('pi', '{}', 'json', {
      baseUrl: 'https://api.openai.com/v1',
      apiKey: 'sk-openai-test',
      model: 'gpt-4o',
      modelOpus: '',
      modelSonnet: '',
      modelHaiku: '',
      modelFable: '',
      modelSubagent: '',
      contextWindow: '',
      claudeAuthEnv: 'ANTHROPIC_AUTH_TOKEN',
      reasoningEffort: '',
      wireApi: '',
      providerSlug: 'openai',
    });
    const parsed = JSON.parse(out) as {
      auth: { openai: { type: string; key: string } };
      models: { providers: { openai: { apiKey?: string; api?: string; baseUrl?: string } } };
      providers?: unknown;
    };
    expect(parsed.auth.openai.type).toBe('api_key');
    expect(parsed.auth.openai.key).toBe('sk-openai-test');
    expect(parsed.models.providers.openai.apiKey).toBe('sk-openai-test');
    expect(parsed.models.providers.openai.api).toBe('openai-completions');
    expect(parsed.models.providers.openai.baseUrl).toBe('https://api.openai.com/v1');
    expect(parsed.providers).toBeUndefined();
  });

  it('does not write Pi auth.json for custom or models.json bind slots', () => {
    const out = applyFormVars('pi', '{}', 'json', {
      baseUrl: 'https://relay.example.com/v1',
      apiKey: 'sk-custom',
      model: 'custom-model',
      modelOpus: '',
      modelSonnet: '',
      modelHaiku: '',
      modelFable: '',
      modelSubagent: '',
      contextWindow: '',
      claudeAuthEnv: 'ANTHROPIC_AUTH_TOKEN',
      reasoningEffort: '',
      wireApi: '',
      providerSlug: 'custom',
    });
    const parsed = JSON.parse(out) as {
      auth?: unknown;
      models: { providers: { custom: { apiKey?: string; baseUrl?: string } } };
      providers?: unknown;
    };
    expect(parsed.auth).toBeUndefined();
    expect(parsed.providers).toBeUndefined();
    expect(parsed.models.providers.custom.apiKey).toBe('sk-custom');
    expect(parsed.models.providers.custom.baseUrl).toBe('https://relay.example.com/v1');
  });

  it('renames the scaffold custom slot when switching to an official auth slot', () => {
    const scaffold = JSON.stringify({
      providers: {
        custom: {
          baseUrl: 'https://your-relay.example.com/v1',
          api: 'openai-completions',
          apiKey: '',
          models: [{ id: 'custom-model', name: 'Custom Model' }],
        },
      },
    });
    const out = applyFormVars('pi', scaffold, 'json', {
      ...extractFormVars('pi', scaffold, 'json'),
      providerSlug: 'anthropic',
      baseUrl: '',
      apiKey: 'sk-ant-test',
      model: '',
    });
    const parsed = JSON.parse(out) as {
      auth: { anthropic: { type: string; key: string } };
      models?: { providers?: Record<string, unknown> };
      providers?: unknown;
    };
    expect(parsed.auth.anthropic).toEqual({ type: 'api_key', key: 'sk-ant-test' });
    expect(parsed.providers).toBeUndefined();
    expect(parsed.models?.providers).toBeUndefined();
  });

  it('keeps Pi auth.json on official-slot edit when apiKey is left empty', () => {
    const source = JSON.stringify({
      auth: { openai: { type: 'api_key', key: 'sk-keep' } },
    });
    const out = applyFormVars('pi', source, 'json', {
      ...extractFormVars('pi', source, 'json'),
      apiKey: '',
      providerSlug: 'openai',
    });
    const parsed = JSON.parse(out) as {
      auth: { openai: { type: string; key: string } };
    };
    expect(parsed.auth.openai).toEqual({ type: 'api_key', key: 'sk-keep' });
  });

  it('extracts Pi official-slot key from auth.json-only envelope', () => {
    const vars = extractFormVars(
      'pi',
      JSON.stringify({
        auth: { openai: { type: 'api_key', key: 'sk-from-auth' } },
      }),
      'json',
    );
    expect(vars.providerSlug).toBe('openai');
    expect(vars.apiKey).toBe('sk-from-auth');
    expect(vars.baseUrl).toBe('');
  });
});

