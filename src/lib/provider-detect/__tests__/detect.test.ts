import { describe, expect, it } from 'vitest';
import { defaultConfigScaffold, smartDetectUrlAndKey } from '../index';
import { CLAUDE_CODE_BASH_EXPORT_BASIC } from './fixtures/claude-code-samples';

describe('smartDetectUrlAndKey', () => {
  it('detects plain url and key (any host / any key shape)', () => {
    expect(smartDetectUrlAndKey('https://any-host.example/api').baseUrl).toBe(
      'https://any-host.example/api',
    );
    expect(smartDetectUrlAndKey('sk-live-abcdefghijklmnopqrst').apiKey).toMatch(
      /^sk-/,
    );
  });

  it('detects from claude settings json with arbitrary values', () => {
    const raw = JSON.stringify(
      {
        env: {
          ANTHROPIC_BASE_URL: 'https://other.example/anthropic',
          ANTHROPIC_AUTH_TOKEN: 'cr_deadbeefdeadbeefdeadbeefdeadbeef',
        },
        model: 'opus',
      },
      null,
      2,
    );
    const r = smartDetectUrlAndKey(raw);
    expect(r.baseUrl).toBe('https://other.example/anthropic');
    expect(r.apiKey).toMatch(/^cr_/);
    expect(r.model).toBe('opus');
    expect(r.suggestedName).toBe('other.example');
  });

  it('detects from codex-like toml + free text', () => {
    const raw = `
model_provider = "crs"
base_url = "https://other.example/openai"
OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwxyz012345
`;
    const r = smartDetectUrlAndKey(raw);
    expect(r.baseUrl).toContain('other.example');
    expect(r.apiKey).toMatch(/^sk-/);
  });

  it('detects mixed paste', () => {
    const r = smartDetectUrlAndKey(
      '中转 https://relay.example.com/v1\nkey: sk-ant-api03-abcdefghijklmnopqrst',
    );
    expect(r.baseUrl).toBe('https://relay.example.com/v1');
    expect(r.apiKey).toMatch(/^sk-ant-/);
  });

  it('does not treat compact json blob as plain key', () => {
    const raw = JSON.stringify({
      env: {
        ANTHROPIC_BASE_URL: 'https://a.example/api',
        ANTHROPIC_AUTH_TOKEN: 'cr_deadbeefdeadbeefdeadbeefdeadbeef',
      },
    });
    const r = smartDetectUrlAndKey(raw);
    expect(r.baseUrl).toBe('https://a.example/api');
    expect(r.apiKey).toMatch(/^cr_/);
  });
});

describe('defaultConfigScaffold', () => {
  it('returns json for claude and toml for codex', () => {
    expect(defaultConfigScaffold('claude').format).toBe('json');
    expect(defaultConfigScaffold('codex').format).toBe('toml');
    expect(defaultConfigScaffold('codex').text).toContain('model_providers');
  });

  it('uses native model configuration shapes for Pi and WorkBuddy', () => {
    const pi = JSON.parse(defaultConfigScaffold('pi').text) as { providers: object };
    const workbuddy = JSON.parse(defaultConfigScaffold('workbuddy').text) as {
      models: unknown[];
      availableModels: string[];
    };
    expect(pi.providers).toBeDefined();
    expect(workbuddy.models).toHaveLength(1);
    expect(workbuddy.availableModels).toEqual(['custom-model']);
  });
});

describe('claude-code bash export (smoke from test fixtures only)', () => {
  it('detects basic bash export shape', () => {
    const r = smartDetectUrlAndKey(CLAUDE_CODE_BASH_EXPORT_BASIC);
    expect(r.baseUrl).toBeTruthy();
    expect(r.apiKey).toMatch(/^sk-/);
    expect(r.matchedDetectors?.some((id) => id.startsWith('claude-'))).toBe(true);
  });
});
