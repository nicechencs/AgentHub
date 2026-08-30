import { describe, expect, it } from 'vitest';
import {
  applySmartPaste,
  defaultConfigScaffold,
  EMPTY_FORM_VARS,
  isLiveFilePath,
  liveConfigPaths,
  smartDetectUrlAndKey,
} from '../index';
import { createTranslator } from '@/lib/i18n';
import { CLAUDE_CODE_BASH_EXPORT_BASIC } from './fixtures/claude-code-samples';

describe('smartDetectUrlAndKey', () => {
  it('detects plain url and key (any host / any key shape)', () => {
    expect(smartDetectUrlAndKey('https://any-host.example/api').baseUrl).toBe(
      'https://any-host.example/api',
    );
    expect(smartDetectUrlAndKey('sk-live-abcdefghijklmnopqrst').apiKey).toMatch(
      /^sk-/,
    );
    expect(smartDetectUrlAndKey('https://any-host.example/api').hints.join(' ')).toMatch(/服务地址/);
    expect(smartDetectUrlAndKey('https://any-host.example/api').hints.join(' ')).not.toMatch(
      /Endpoint|env_key|Provider：/,
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

  it('detects OpenRouter-style JSON camelCase baseURL', () => {
    const r = smartDetectUrlAndKey(
      JSON.stringify({
        baseURL: 'https://openrouter.ai/api/v1',
        model: 'stealth/ox-alpha',
      }),
    );
    expect(r.baseUrl).toBe('https://openrouter.ai/api/v1');
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
    expect(defaultConfigScaffold('kimi').text).toContain('[models."kimi-k2"]');
    expect(defaultConfigScaffold('kimi').text).toContain('max_context_size = 131072');
    expect(defaultConfigScaffold('kimi').text).toContain('type = "openai"');
  });

  it('does not use Claude env JSON for Cursor', () => {
    const scaffold = defaultConfigScaffold('cursor');
    expect(scaffold.format).toBe('json');
    const parsed = JSON.parse(scaffold.text) as { env?: unknown; note?: string };
    expect(parsed.env).toBeUndefined();
    expect(parsed.note).toBe('cursor-pool-only');
    expect(scaffold.text).not.toContain('ANTHROPIC');
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

describe('liveConfigPaths', () => {
  it('treats only real file paths as auth rows', () => {
    expect(isLiveFilePath('~/.codex/auth.json')).toBe(true);
    expect(isLiveFilePath('C:\\Users\\demo\\.grok\\auth.json')).toBe(true);
    expect(isLiveFilePath('/home/demo/.pi/agent/auth.json')).toBe(true);
    expect(isLiveFilePath('官方登录态 / 文件型凭据（以 detect 为准）')).toBe(false);
    expect(isLiveFilePath('CURSOR_API_KEY 或 agent login')).toBe(false);
  });

  it('keeps Claude chrome to settings.json without detect jargon', () => {
    const paths = liveConfigPaths('claude');
    expect(paths.config).toBe('~/.claude/settings.json');
    expect(paths.auth).toBe('~/.claude/.credentials.json');
    expect(paths.hint).toMatch(/settings\.json|Claude/);
    expect(`${paths.auth ?? ''} ${paths.hint}`).not.toMatch(
      /detect|文件型凭据|环境变量|未必在单一文件|Endpoint|schema/i,
    );
  });

  it('translates path hints when a translator is passed', () => {
    const tEn = createTranslator('en');
    const paths = liveConfigPaths('claude', tEn);
    expect(paths.hint).not.toMatch(/[\u4e00-\u9fff]/);
    expect(liveConfigPaths('cursor', tEn).config).toBe('No stable provider config file');
    expect(liveConfigPaths('pi', tEn).openDir).toBe('~/.pi/agent (or PI_CODING_AGENT_DIR)');
  });

  it('only exposes auth when it is a live file path', () => {
    for (const agentId of ['claude', 'codex', 'kimi', 'grok', 'pi', 'workbuddy', 'cursor', 'zcode', 'unknown']) {
      const auth = liveConfigPaths(agentId).auth;
      if (auth) expect(isLiveFilePath(auth)).toBe(true);
    }
  });

  it('points ZCode at v2/config.json, not config.toml', () => {
    const paths = liveConfigPaths('zcode');
    expect(paths.config).toBe('~/.zcode/v2/config.json');
    expect(paths.auth).toBe('~/.zcode/v2/config.json');
    expect(paths.config).not.toMatch(/config\.toml/);
  });
});

describe('ZCode catalog scaffold', () => {
  it('defaults official BigModel slot with a model list', () => {
    const parsed = JSON.parse(defaultConfigScaffold('zcode').text) as {
      providerId: string;
      baseURL: string;
      models: string[];
    };
    expect(parsed.providerId).toBe('builtin:bigmodel');
    expect(parsed.baseURL).toContain('open.bigmodel.cn');
    expect(parsed.models.length).toBeGreaterThan(0);
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

describe('OpenRouter env-style paste', () => {
  it('honors MODEL= / model= and API_KEY= alongside a URL', () => {
    const r = smartDetectUrlAndKey(
      'https://openrouter.ai/api/v1\nMODEL=stealth/ox-alpha\nAPI_KEY=sk-or-v1-abcdefghijklmnopqrstuv',
    );
    expect(r.baseUrl).toBe('https://openrouter.ai/api/v1');
    expect(r.model).toBe('stealth/ox-alpha');
    expect(r.apiKey).toMatch(/^sk-or-v1-/);
  });

  it('reads lowercase model= without overwriting an existing key on merge', async () => {
    const r = applySmartPaste('codex', 'https://openrouter.ai/api/v1\nmodel=stealth/ox-alpha', {
      vars: { ...EMPTY_FORM_VARS, apiKey: 'sk-or-v1-already-pasted-abcdefgh' },
    });
    expect(r.vars.baseUrl).toBe('https://openrouter.ai/api/v1');
    expect(r.vars.model).toBe('stealth/ox-alpha');
    expect(r.vars.apiKey).toBe('sk-or-v1-already-pasted-abcdefgh');
  });

  it('does not require MODEL= to recognize URL+key (empty model is OK)', () => {
    const detect = smartDetectUrlAndKey(
      'https://openrouter.ai/api/v1\nAPI_KEY=sk-or-v1-abcdefghijklmnopqrstuv',
    );
    expect(detect.baseUrl).toBe('https://openrouter.ai/api/v1');
    expect(detect.apiKey).toMatch(/^sk-or-v1-/);
    expect(detect.model).toBeUndefined();
    const r = applySmartPaste('codex', 'https://openrouter.ai/api/v1\nAPI_KEY=sk-or-v1-abcdefghijklmnopqrstuv');
    expect(Boolean(r.vars.apiKey.trim())).toBe(true);
  });
});

