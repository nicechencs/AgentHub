import { describe, expect, it } from 'vitest';
import {
  authFileName,
  configFileName,
  defaultLivePathForFile,
  extractAccountCredentialFiles,
  extractProviderCredentialFiles,
  resolveCredentialFilePath,
} from './credential-files';

describe('credential file names', () => {
  it('maps each agent to its login and config files', () => {
    expect(authFileName('grok')).toBe('auth.json');
    expect(authFileName('claude')).toBe('.credentials.json');
    expect(authFileName('kimi')).toBe('kimi-code.json');
    expect(configFileName('grok')).toBe('config.toml');
    expect(configFileName('claude')).toBe('settings.json');
    expect(configFileName('pi')).toBe('settings.json');
    expect(configFileName('workbuddy')).toBe('models.json');
    expect(configFileName('zcode')).toBe('config.json');
    expect(authFileName('zcode')).toBe('config.json');
  });

  it('maps WorkBuddy live API Key files to models.json, not settings.json', () => {
    expect(defaultLivePathForFile('workbuddy', 'models.json')).toBe(
      '~/.workbuddy/models.json',
    );
    expect(defaultLivePathForFile('workbuddy', configFileName('workbuddy'))).toBe(
      '~/.workbuddy/models.json',
    );
  });

  it('maps ZCode live files to v2/config.json, not config.toml', () => {
    expect(defaultLivePathForFile('zcode', 'config.json')).toBe(
      '~/.zcode/v2/config.json',
    );
    expect(defaultLivePathForFile('zcode', configFileName('zcode'))).toBe(
      '~/.zcode/v2/config.json',
    );
  });
});

describe('extractAccountCredentialFiles', () => {
  it('shows auth.json from an official Grok login body', () => {
    const files = extractAccountCredentialFiles({
      agentId: 'grok',
      kind: 'oauth',
      format: 'auth_json',
      source: 'auth.json',
      credentials: {
        format: 'auth_json',
        body: {
          'https://auth.x.ai::b1a00492-073a-47ea-816f-4c329264a828': {
            email: 'a@example.com',
            refresh_token: 'rt-secret-should-not-leak',
            access_token: 'at-secret-should-not-leak',
          },
        },
      },
    });
    expect(files).toHaveLength(1);
    expect(files[0]!.name).toBe('auth.json');
    expect(files[0]!.content).toContain('a@example.com');
    expect(files[0]!.content).toContain('refresh_token');
    expect(files[0]!.content).toContain('rt-secret-should-not-leak');
  });

  it('shows config.toml from a Grok API Key snapshot as stored', () => {
    const files = extractAccountCredentialFiles({
      agentId: 'grok',
      kind: 'apikey',
      format: 'api_key',
      source: 'config.toml',
      credentials: {
        format: 'api_key',
        api_key: 'xai-file-key-12345678',
        content: '[model."grok"]\napi_key = "xai-file-key-12345678"\n',
      },
    });
    expect(files).toHaveLength(1);
    expect(files[0]!.name).toBe('config.toml');
    expect(files[0]!.content).toContain('api_key = "xai-file-key-12345678"');
  });

  it('shows both files for a mixed Grok snapshot', () => {
    const files = extractAccountCredentialFiles({
      agentId: 'grok',
      kind: 'apikey',
      format: 'grok_bundle',
      source: 'config.toml+auth.json',
      credentials: {
        format: 'grok_bundle',
        api_key: 'xai-file-key-12345678',
        content: 'api_key = "xai-file-key-12345678"\n',
        auth: {
          slot: { refresh_token: 'rt-oauth-secret', email: 'b@example.com' },
        },
      },
    });
    expect(files.map((file) => file.name)).toEqual(['auth.json', 'config.toml']);
    expect(files[0]!.content).toContain('b@example.com');
    expect(files[0]!.content).toContain('rt-oauth-secret');
    expect(files[1]!.content).toContain('xai-file-key-12345678');
  });

  it('uses .credentials.json for Claude official login', () => {
    const files = extractAccountCredentialFiles({
      agentId: 'claude',
      kind: 'oauth',
      format: 'credentials_json',
      source: '.credentials.json',
      credentials: {
        format: 'credentials_json',
        body: { claudeAiOauth: { accessToken: 'claude-at-secret', refreshToken: 'claude-rt-secret' } },
      },
    });
    expect(files[0]!.name).toBe('.credentials.json');
    expect(files[0]!.content).toContain('claude-at-secret');
  });

  it('falls back to a keyword snapshot when no file body was stored', () => {
    const files = extractAccountCredentialFiles({
      agentId: 'claude',
      kind: 'apikey',
      format: 'api_key',
      source: 'manual',
      credentials: {
        format: 'api_key',
        api_key: 'sk-ant-secret-12345678',
        env_key: 'ANTHROPIC_AUTH_TOKEN',
      },
    });
    expect(files).toHaveLength(1);
    expect(files[0]!.name).toBe('settings.json');
    expect(files[0]!.content).toContain('ANTHROPIC_AUTH_TOKEN');
    expect(files[0]!.content).toContain('sk-ant-secret-12345678');
  });

  it('rebuilds ZCode config.json as a catalog row with provider name and URL', () => {
    const files = extractAccountCredentialFiles({
      agentId: 'zcode',
      kind: 'apikey',
      format: 'api_key',
      source: 'live',
      credentials: {
        format: 'api_key',
        api_key: 'sk-custom-secret-12345678',
        provider: 'zcode',
        provider_id: '03954ae6-61b1-4d42-97e3-42cee3ab2be1',
        provider_name: 'grok',
        kind: 'openai',
        base_url: 'https://api.qooo.io/v1',
        models: { 'grok-4.6': { limit: { context: 500000 } } },
      },
    });
    expect(files).toHaveLength(1);
    expect(files[0]!.name).toBe('config.json');
    expect(files[0]!.content).toContain('"name": "grok"');
    expect(files[0]!.content).toContain('"baseURL": "https://api.qooo.io/v1"');
    expect(files[0]!.content).toContain('"kind": "openai"');
    expect(files[0]!.content).toContain('grok-4.6');
    expect(files[0]!.content).toContain('sk-custom-secret-12345678');
    expect(files[0]!.content).toContain('"apiKey": "sk-custom-secret-12345678"');
  });

  it('rebuilds WorkBuddy models.json as a native catalog row with name and URL', () => {
    const files = extractAccountCredentialFiles({
      agentId: 'workbuddy',
      kind: 'apikey',
      format: 'api_key',
      source: 'live',
      credentials: {
        format: 'api_key',
        api_key: 'sk-custom-secret-12345678',
        provider: 'workbuddy',
        model_id: 'grok-4.6',
        id: 'grok-4.6',
        name: 'grok-4.6',
        vendor: 'Custom',
        url: 'https://api.qooo.io/v1/chat/completions',
        base_url: 'https://api.qooo.io/v1/chat/completions',
        supportsToolCall: true,
      },
    });
    expect(files).toHaveLength(1);
    expect(files[0]!.name).toBe('models.json');
    expect(files[0]!.content).toContain('"name": "grok-4.6"');
    expect(files[0]!.content).toContain('"url": "https://api.qooo.io/v1/chat/completions"');
    expect(files[0]!.content).toContain('"vendor": "Custom"');
    expect(files[0]!.content).toContain('sk-custom-secret-12345678');
    expect(files[0]!.content).toContain('"apiKey": "sk-custom-secret-12345678"');
    expect(files[0]!.content.trim().startsWith('[')).toBe(true);
  });

  it('prefers the original ZCode catalog row over flattened Hub fields', () => {
    const files = extractAccountCredentialFiles({
      agentId: 'zcode',
      kind: 'apikey',
      format: 'api_key',
      credentials: {
        format: 'api_key',
        api_key: '***',
        provider_id: 'aabbcc',
        provider_name: 'should-not-win',
        catalog_row: {
          name: 'grok',
          kind: 'openai',
          options: {
            apiKey: 'sk-live-secret-should-not-leak',
            baseURL: 'https://example.test/v1',
            apiKeyRequired: true,
          },
          source: 'custom',
          models: { 'grok-4.6': { zcode: { priority: 1 } } },
        },
      },
    });
    expect(files[0]!.content).toContain('"name": "grok"');
    expect(files[0]!.content).not.toContain('should-not-win');
    expect(files[0]!.content).toContain('apiKeyRequired');
    expect(files[0]!.content).toContain('"priority": 1');
    expect(files[0]!.content).toContain('sk-live-secret-should-not-leak');
  });
});

describe('extractProviderCredentialFiles', () => {
  it('shows the provider config file as stored', () => {
    const files = extractProviderCredentialFiles({
      agentId: 'claude',
      configFormat: 'json',
      configText: JSON.stringify({ env: { ANTHROPIC_AUTH_TOKEN: 'sk-ant-secret-12345678' } }, null, 2),
    });
    expect(files[0]!.name).toBe('settings.json');
    expect(files[0]!.content).toContain('ANTHROPIC_AUTH_TOKEN');
    expect(files[0]!.content).toContain('sk-ant-secret-12345678');
  });
});

describe('resolveCredentialFilePath', () => {
  it('prefers the matching live path, then a default ~/ path', () => {
    expect(
      resolveCredentialFilePath(
        'auth.json',
        { config: '~/.grok/config.toml', auth: '~/.grok/auth.json', extra: [], openDir: '~/.grok' },
        'grok',
      ),
    ).toBe('~/.grok/auth.json');
    expect(resolveCredentialFilePath('kimi-code.json', null, 'kimi')).toBe(
      '~/.kimi-code/credentials/kimi-code.json',
    );
  });
});
