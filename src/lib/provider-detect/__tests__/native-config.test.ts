import { describe, expect, it } from 'vitest';
import { getConfigTextError } from '@/pages/providers/ProviderEditDialog';
import {
  tomlSyntaxIssue,
  validateNativeConfigText,
} from '../native-config';
import { defaultConfigScaffold } from '../scaffold';

describe('validateNativeConfigText', () => {
  it('accepts Claude settings.json with env strings', () => {
    const text = JSON.stringify(
      {
        env: {
          ANTHROPIC_BASE_URL: 'https://openrouter.ai/api/v1',
          ANTHROPIC_AUTH_TOKEN: 'sk-or-test',
        },
      },
      null,
      2,
    );
    expect(validateNativeConfigText('claude', text, 'json')).toBeNull();
    expect(getConfigTextError('claude', text, 'json')).toBeNull();
  });

  it('rejects OpenAI-compat aliases in Claude JSON', () => {
    const text = JSON.stringify({
      baseURL: 'https://openrouter.ai/api/v1',
      baseUrl: 'https://openrouter.ai/api/v1',
    });
    const issue = validateNativeConfigText('claude', text, 'json');
    expect(issue?.code).toBe('claude_foreign_keys');
    expect(issue?.keys).toEqual(expect.arrayContaining(['baseURL', 'baseUrl']));
    expect(getConfigTextError('claude', text, 'json')).toMatch(/ANTHROPIC_BASE_URL/);
  });

  it('rejects non-string Claude env values', () => {
    const text = JSON.stringify({ env: { CLAUDE_CODE_MAX_CONTEXT_TOKENS: 1048576 } });
    expect(validateNativeConfigText('claude', text, 'json')?.code).toBe('claude_env_string');
  });

  it('rejects a JSON object for Codex TOML', () => {
    expect(validateNativeConfigText('codex', '{"baseURL":"https://x"}', 'toml')?.code).toBe(
      'expect_toml',
    );
    expect(getConfigTextError('codex', '{"baseURL":"https://x"}', 'toml')).toMatch(/TOML/);
  });

  it('accepts generated Codex / Kimi / Grok scaffolds', () => {
    for (const agent of ['codex', 'kimi', 'grok'] as const) {
      const scaffold = defaultConfigScaffold(agent);
      expect(validateNativeConfigText(agent, scaffold.text, scaffold.format)).toBeNull();
      expect(tomlSyntaxIssue(scaffold.text)).toBeNull();
    }
  });

  it('rejects a TOML line that is not a key or table', () => {
    expect(tomlSyntaxIssue('this is not toml\n')).toBe('1');
    expect(validateNativeConfigText('grok', 'this is not toml\n', 'toml')?.code).toBe('toml_parse');
  });
});
