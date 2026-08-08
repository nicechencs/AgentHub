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

