import { describe, expect, it } from 'vitest';
import { mapCoreProvider, toCoreInput } from './provider-map';

describe('provider-map codex dual shape', () => {
  it('maps auth.OPENAI_API_KEY and toml content', () => {
    const p = mapCoreProvider({
      id: 'c1',
      agentId: 'codex',
      name: 'Relay',
      settingsConfig: {
        format: 'toml',
        content: 'model = "gpt-5"\n',
        auth: { OPENAI_API_KEY: '***' },
      },
      meta: { preset: 'openai-compatible' },
      isCurrent: false,
    });
    expect(p.configFormat).toBe('toml');
    expect(p.configText).toContain('gpt-5');
    expect(p.authApiKey).toBe('***');
    expect(p.preset).toBe('openai-compatible');
    expect(p.secretTail).toBeUndefined();
  });

  it('maps redacted API key tail from meta', () => {
    const p = mapCoreProvider({
      id: 'k2',
      agentId: 'kimi',
      name: 'Relay',
      settingsConfig: { api_key: '***' },
      meta: { secretTail: '**JF6Q' },
      isCurrent: false,
    });
    expect(p.secretTail).toBe('**JF6Q');
  });

  it('maps secretHash from meta without treating it as a secret', () => {
    const p = mapCoreProvider({
      id: 'k3',
      agentId: 'codex',
      name: 'OpenAI',
      settingsConfig: { api_key: '***' },
      meta: { secretHash: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' },
      isCurrent: false,
    });
    expect(p.secretHash).toBe('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
  });

  it('accepts dual-shape config alias', () => {
    const p = mapCoreProvider({
      id: 'c2',
      agentId: 'codex',
      name: 'CC',
      settingsConfig: {
        config: 'model = "from-alias"\n',
        auth: { OPENAI_API_KEY: 'sk-x' },
      },
      meta: {},
      isCurrent: true,
    });
    expect(p.configText).toContain('from-alias');
    expect(p.authApiKey).toBe('sk-x');
  });

  it('does not invent kimi-code-membership when meta.preset is missing', () => {
    const p = mapCoreProvider({
      id: 'k1',
      agentId: 'kimi',
      name: 'Imported',
      settingsConfig: { format: 'toml', content: 'api_key = "x"\n' },
      meta: {},
      isCurrent: false,
    });
    expect(p.preset).toBe('custom');
  });

  it('toCoreInput writes auth for codex and marker on empty', () => {
    const withKey = toCoreInput({
      id: 'c3',
      agentId: 'codex',
      name: 'N',
      preset: 'openai-compatible',
      configText: 'model = "gpt-5"\n',
      configFormat: 'toml',
      authApiKey: 'sk-new',
      isCurrent: false,
    });
    expect(withKey.settingsConfig).toMatchObject({
      format: 'toml',
      content: 'model = "gpt-5"\n',
      auth: { OPENAI_API_KEY: 'sk-new' },
    });

    const keep = toCoreInput({
      id: 'c3',
      agentId: 'codex',
      name: 'N',
      preset: 'openai-compatible',
      configText: 'model = "gpt-5"\n',
      configFormat: 'toml',
      authApiKey: '',
      isCurrent: false,
    });
    expect(keep.settingsConfig.auth).toEqual({ OPENAI_API_KEY: '***' });
  });

  it('toCoreInput omits auth when undefined (new without key)', () => {
    const input = toCoreInput({
      id: 'c4',
      agentId: 'codex',
      name: 'N',
      preset: 'openai',
      configText: 'model = "gpt-5"\n',
      configFormat: 'toml',
      isCurrent: false,
    });
    expect(input.settingsConfig.auth).toBeUndefined();
  });
});
