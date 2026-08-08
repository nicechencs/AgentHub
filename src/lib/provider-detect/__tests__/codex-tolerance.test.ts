import { describe, expect, it } from 'vitest';
import { applySmartPaste, smartDetectUrlAndKey } from '../index';
import {
  CODEX_AUTH_JSON,
  CODEX_DUAL_BLOCK,
  CODEX_DUAL_ENV_KEY,
  CODEX_TOML_OPENAI_PROVIDER,
} from './fixtures/codex-samples';

describe('Codex paste 容错', () => {
  it('tolerates UI chrome mixed into dual-block paste', () => {
    const messy = `
使用 API 密钥
~/.codex/config.toml
复制
${CODEX_TOML_OPENAI_PROVIDER}
~/.codex/auth.json
复制
${CODEX_AUTH_JSON}
请确保配置目录存在
`;
    const r = smartDetectUrlAndKey(messy);
    expect(r.baseUrl).toMatch(/^https:\/\//);
    expect(r.apiKey).toMatch(/^sk-/);
    expect(r.providerSlug).toBe('OpenAI');
    expect(r.rawConfigText).toContain('supports_websockets');
    expect(r.rawConfigText).not.toContain('OPENAI_API_KEY');
    expect(r.rawConfigText).not.toContain('使用 API 密钥');
  });

  it('tolerates auth.json before config.toml order', () => {
    const reversed = `
${CODEX_AUTH_JSON}

${CODEX_TOML_OPENAI_PROVIDER}
`;
    const r = smartDetectUrlAndKey(reversed);
    expect(r.baseUrl).toMatch(/^https:\/\//);
    expect(r.apiKey).toMatch(/^sk-/);
    expect(r.rawConfigText).toContain('[model_providers.OpenAI]');
  });

  it('tolerates extra blank lines and CRLF', () => {
    const crlf = CODEX_DUAL_BLOCK.replace(/\n/g, '\r\n\r\n');
    const r = applySmartPaste('codex', crlf);
    expect(r.vars.baseUrl).toMatch(/^https:\/\//);
    expect(r.vars.apiKey).toMatch(/^sk-/);
    expect(r.configText).toContain('goals = true');
  });

  it('tolerates Sub2API dual block with tip text', () => {
    const messy = `
Codex CLI
macOS / Linux
如已有 config.toml，请先备份再合并此服务商配置。
~/.codex/config.toml
${CODEX_DUAL_ENV_KEY.split('export')[0]}
Terminal
export SUB2API_API_KEY="sk-test-sample-codex-key-zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
将 config.toml 保存到 ~/.codex
`;
    const r = applySmartPaste('codex', messy);
    expect(r.vars.baseUrl).toMatch(/\/v1$/);
    expect(r.vars.providerSlug).toBe('sub2api_grok');
    expect(r.vars.apiKey).toMatch(/^sk-/);
    expect(r.configText).toContain('env_key = "SUB2API_API_KEY"');
    expect(r.configText).toContain('name = "Sub2API Grok"');
    expect(r.configText).not.toContain('export');
  });

  it('auth-only paste still works without toml', () => {
    const r = smartDetectUrlAndKey(CODEX_AUTH_JSON);
    expect(r.apiKey).toMatch(/^sk-/);
    expect(r.baseUrl).toBeUndefined();
  });
});
