import { describe, expect, it } from 'vitest';
import {
  applySmartPaste,
  EMPTY_FORM_VARS,
  initFormFromConfig,
  isGrokTomlPaste,
} from '../index';
import {
  CLAUDE_CODE_BASH_EXPORT_BASIC,
  CLAUDE_CODE_SETTINGS_JSON_BASIC,
} from './fixtures/claude-code-samples';

const OFFICIAL_CLAUDE_LEFTOVER = {
  ...EMPTY_FORM_VARS,
  model: 'sonnet',
  modelOpus: 'opus',
  modelSonnet: 'sonnet',
  modelHaiku: 'haiku',
  modelFable: 'sonnet',
  modelSubagent: 'haiku',
};

describe('applySmartPaste', () => {
  it('fills claude scaffold from pasted json', () => {
    const paste = JSON.stringify({
      env: {
        ANTHROPIC_BASE_URL: 'https://relay.example.com',
        ANTHROPIC_AUTH_TOKEN: 'sk-abcdefghijklmnopqrst',
      },
    });
    const r = applySmartPaste('claude', paste);
    expect(r.vars.baseUrl).toBe('https://relay.example.com');
    expect(r.vars.apiKey).toMatch(/^sk-/);
    expect(r.configFormat).toBe('json');
    expect(r.configText).toContain('ANTHROPIC_BASE_URL');
    expect(r.suggestedName).toBe('relay.example.com');
    expect(r.configText).not.toContain('"baseURL"');
    expect(r.configText).not.toContain('"baseUrl"');
  });

  it('turns OpenRouter-style JSON into Claude settings.json env', () => {
    const r = applySmartPaste(
      'claude',
      JSON.stringify({
        baseURL: 'https://openrouter.ai/api/v1',
        baseUrl: 'https://openrouter.ai/api/v1',
        apiKey: 'sk-abcdefghijklmnopqrst',
      }),
    );
    const parsed = JSON.parse(r.configText) as {
      env: Record<string, string>;
      baseURL?: unknown;
      baseUrl?: unknown;
      apiKey?: unknown;
    };
    expect(parsed.env.ANTHROPIC_BASE_URL).toBe('https://openrouter.ai/api/v1');
    expect(parsed.env.ANTHROPIC_AUTH_TOKEN).toBe('sk-abcdefghijklmnopqrst');
    expect(parsed.baseURL).toBeUndefined();
    expect(parsed.baseUrl).toBeUndefined();
    expect(parsed.apiKey).toBeUndefined();
  });

  it('does not keep leftover official models when pasting complete settings.json without a model', () => {
    const r = applySmartPaste('claude', CLAUDE_CODE_SETTINGS_JSON_BASIC, {
      vars: OFFICIAL_CLAUDE_LEFTOVER,
      configText: JSON.stringify({ env: {}, model: 'sonnet' }, null, 2),
      configFormat: 'json',
    });
    const parsed = JSON.parse(r.configText) as {
      $schema?: string;
      model?: string;
      env: Record<string, string>;
    };
    expect(parsed.$schema).toBe('https://json.schemastore.org/claude-code-settings.json');
    expect(parsed.model).toBeUndefined();
    expect(parsed.env.ANTHROPIC_MODEL).toBeUndefined();
    expect(parsed.env.ANTHROPIC_DEFAULT_OPUS_MODEL).toBeUndefined();
    expect(parsed.env.ANTHROPIC_DEFAULT_SONNET_MODEL).toBeUndefined();
    expect(parsed.env.ANTHROPIC_DEFAULT_HAIKU_MODEL).toBeUndefined();
    expect(parsed.env.ANTHROPIC_DEFAULT_FABLE_MODEL).toBeUndefined();
    expect(parsed.env.CLAUDE_CODE_SUBAGENT_MODEL).toBeUndefined();
    expect(parsed.env.ANTHROPIC_BASE_URL).toBe('https://relay.example.com');
    expect(parsed.env.CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC).toBe('1');
    expect(parsed.env.CLAUDE_CODE_ATTRIBUTION_HEADER).toBe('0');
    expect(r.vars.model).toBe('');
    expect(r.vars.modelOpus).toBe('');
  });

  it('clears leftover official models from a Claude env export that omits MODEL', () => {
    const r = applySmartPaste('claude', CLAUDE_CODE_BASH_EXPORT_BASIC, {
      vars: OFFICIAL_CLAUDE_LEFTOVER,
      configText: JSON.stringify({ env: {}, model: 'sonnet' }, null, 2),
      configFormat: 'json',
    });
    const env = JSON.parse(r.configText).env as Record<string, string>;
    expect(r.vars.model).toBe('');
    expect(JSON.parse(r.configText).model).toBeUndefined();
    expect(env.ANTHROPIC_MODEL).toBeUndefined();
    expect(env.ANTHROPIC_DEFAULT_OPUS_MODEL).toBeUndefined();
    expect(env.ANTHROPIC_BASE_URL).toMatch(/^https:\/\//);
  });

  it('keeps a typed model when the paste is only a URL and key', () => {
    const r = applySmartPaste(
      'claude',
      'https://relay.example.com\nsk-abcdefghijklmnopqrstuvwxyz012345',
      { vars: { ...EMPTY_FORM_VARS, model: 'opus' } },
    );
    expect(r.vars.model).toBe('opus');
  });

  it('does not overlay leftover Kimi official model onto a providers TOML paste that omitted default_model', () => {
    const paste = [
      '[providers.custom]',
      'base_url = "https://relay.example.com/v1"',
      'api_key = "sk-abcdefghijklmnopqrst"',
      '',
    ].join('\n');
    const r = applySmartPaste('kimi', paste, {
      vars: { ...EMPTY_FORM_VARS, model: 'kimi-k2' },
      configText: 'default_model = "kimi-k2"\n',
      configFormat: 'toml',
    });
    expect(r.vars.model).toBe('');
    expect(r.configText).not.toMatch(/default_model\s*=/);
    expect(r.configText).toContain('base_url = "https://relay.example.com/v1"');
  });

  it('does not overlay leftover Codex official model onto a TOML paste that omitted model', () => {
    const paste = [
      'model_provider = "custom"',
      '[model_providers.custom]',
      'name = "custom"',
      'base_url = "https://relay.example.com/v1"',
      'wire_api = "responses"',
      '',
    ].join('\n');
    const r = applySmartPaste('codex', paste, {
      vars: { ...EMPTY_FORM_VARS, model: 'gpt-5.1-codex' },
      configText: 'model = "gpt-5.1-codex"\n',
      configFormat: 'toml',
    });
    expect(r.vars.model).toBe('');
    expect(r.configText).not.toMatch(/model\s*=\s*"gpt-5.1-codex"/);
    expect(r.configText).toContain('base_url = "https://relay.example.com/v1"');
  });

  it('parses pasted Claude window env onto the form choice', () => {
    const paste = JSON.stringify({
      env: {
        ANTHROPIC_BASE_URL: 'https://openrouter.ai/api/v1',
        ANTHROPIC_AUTH_TOKEN: 'sk-abcdefghijklmnopqrst',
        ANTHROPIC_MODEL: 'stealth/ox-alpha',
        CLAUDE_CODE_MAX_CONTEXT_TOKENS: '1048576',
      },
    });
    const r = applySmartPaste('claude', paste);
    expect(r.vars.model).toBe('stealth/ox-alpha');
    expect(r.vars.contextWindow).toBe('1048576');
    const env = JSON.parse(r.configText).env as Record<string, string>;
    expect(env.CLAUDE_CODE_MAX_CONTEXT_TOKENS).toBe('1048576');
    expect(env.CLAUDE_CODE_AUTO_COMPACT_WINDOW).toBe('1048576');
  });

  it('fills codex toml from mixed paste', () => {
    const r = applySmartPaste(
      'codex',
      'endpoint https://mycoding.cc/openai\nOPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwxyz012345',
    );
    expect(r.vars.baseUrl).toContain('mycoding.cc');
    expect(r.vars.apiKey).toMatch(/^sk-/);
    expect(r.configFormat).toBe('toml');
    expect(r.configText).toContain('model_providers');
    expect(r.configText).not.toContain('sk-abcdefghijklmnopqrstuvwxyz012345');
  });

  it('preserves a complete Grok Build registry TOML paste', () => {
    const r = applySmartPaste(
      'grok',
      [
        '[models]',
        'default = "grok"',
        'web_search = "grok"',
        '',
        '[model."grok"]',
        'model = "grok-4.5"',
        'base_url = "https://relay.example.com/v1"',
        'name = "Grok 4.5"',
        'api_key = "sk-grok-test-abcdefghijklmnop"',
        'api_backend = "responses"',
        'context_window = 1000000',
        'supports_backend_search = true',
        '',
      ].join('\n'),
    );
    expect(r.vars.model).toBe('grok-4.5');
    expect(r.vars.baseUrl).toBe('https://relay.example.com/v1');
    expect(r.vars.apiKey).toBe('sk-grok-test-abcdefghijklmnop');
    expect(r.configFormat).toBe('toml');
    expect(r.configText).toContain('[models]');
    expect(r.configText).toContain('[model."grok"]');
    expect(r.configText).toContain('name = "Grok 4.5"');
    expect(r.configText).toContain('api_backend = "responses"');
    expect(r.configText).toContain('supports_backend_search = true');
  });

  it('keeps grok api_backend, extra models, and endpoints when paste omits base_url', () => {
    const paste = [
      '[models]',
      'default = "grok"',
      'web_search = "grok"',
      '',
      '[model."grok"]',
      'model = "grok-4.5"',
      'api_backend = "responses"',
      'context_window = 1000000',
      'supports_backend_search = true',
      '',
      '[model."fast"]',
      'model = "grok-code-fast-1"',
      'base_url = "https://api.x.ai/v1"',
      '',
      '[endpoints]',
      'chat = "/v1/chat/completions"',
      '',
      '[auth]',
      'mode = "api_key"',
      '',
    ].join('\n');
    expect(isGrokTomlPaste(paste)).toBe(true);
    const r = applySmartPaste('grok', paste);
    expect(r.vars.model).toBe('grok-4.5');
    expect(r.configFormat).toBe('toml');
    expect(r.configText).toContain('api_backend = "responses"');
    expect(r.configText).toContain('context_window = 1000000');
    expect(r.configText).toContain('[model."fast"]');
    expect(r.configText).toContain('grok-code-fast-1');
    expect(r.configText).toContain('[endpoints]');
    expect(r.configText).toContain('chat = "/v1/chat/completions"');
    expect(r.configText).toContain('[auth]');
    expect(r.configText).toContain('mode = "api_key"');
  });
});

describe('initFormFromConfig', () => {
  it('loads codex auth key from authApiKey', () => {
    const vars = initFormFromConfig(
      'codex',
      'model = "gpt-5"\n',
      'toml',
      'sk-from-auth',
    );
    expect(vars.apiKey).toBe('sk-from-auth');
  });
});

