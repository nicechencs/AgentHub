import { describe, expect, it } from 'vitest';
import { applySmartPaste, initFormFromConfig } from '../index';

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

