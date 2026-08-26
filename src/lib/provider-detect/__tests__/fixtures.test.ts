import { describe, expect, it } from 'vitest';
import { applySmartPaste, smartDetectUrlAndKey } from '../index';
import { CLAUDE_CODE_SAMPLES } from './fixtures/claude-code-samples';
import { CODEX_SAMPLES } from './fixtures/codex-samples';

describe('CLAUDE_CODE_SAMPLES batch (shape only; values from fixtures)', () => {
  for (const sample of CLAUDE_CODE_SAMPLES) {
    it(`detect: ${sample.id} — ${sample.description}`, () => {
      const r = smartDetectUrlAndKey(sample.text);
      expect(r.baseUrl).toBe(sample.expect.baseUrl);
      expect(r.apiKey?.startsWith(sample.expect.apiKeyPrefix)).toBe(true);
      expect(r.claudeAuthEnv).toBe('ANTHROPIC_AUTH_TOKEN');
      // 域名随粘贴内容变化，不写死某一中转
      expect(r.suggestedName).toBeTruthy();

      if (sample.expect.model) {
        expect(r.model).toBe(sample.expect.model);
      }
      if (sample.expect.hasExtraFlags) {
        expect(r.extraEnv?.CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC).toMatch(/^1$/);
        expect(r.extraEnv?.CLAUDE_CODE_ATTRIBUTION_HEADER).toMatch(/^0$/);
      }
      if (sample.expect.hasModelMap) {
        expect(r.extraEnv?.ANTHROPIC_DEFAULT_SONNET_MODEL).toBe(sample.expect.model);
        expect(r.extraEnv?.CLAUDE_CODE_SUBAGENT_MODEL).toBe(sample.expect.model);
      }
    });

    it(`applySmartPaste claude: ${sample.id}`, () => {
      const r = applySmartPaste('claude', sample.text);
      const parsed = JSON.parse(r.configText) as {
        $schema?: string;
        model?: string;
        env: Record<string, string>;
      };
      const env = parsed.env;
      expect(env.ANTHROPIC_BASE_URL).toBe(sample.expect.baseUrl);
      expect(env.ANTHROPIC_AUTH_TOKEN?.startsWith(sample.expect.apiKeyPrefix)).toBe(
        true,
      );
      if (sample.expect.hasExtraFlags) {
        expect(env.CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC).toMatch(/^1$/);
      }
      if (sample.expect.hasModelMap && sample.expect.model) {
        expect(env.ANTHROPIC_MODEL).toBe(sample.expect.model);
        expect(env.ANTHROPIC_DEFAULT_OPUS_MODEL).toBe(sample.expect.model);
        expect(env.CLAUDE_CODE_SUBAGENT_MODEL).toBe(sample.expect.model);
      } else {
        expect(parsed.model).toBeUndefined();
        expect(env.ANTHROPIC_MODEL).toBeUndefined();
      }
      if (sample.id.startsWith('settings-json')) {
        expect(parsed.$schema).toBe(
          'https://json.schemastore.org/claude-code-settings.json',
        );
      }
    });
  }
});

describe('CODEX_SAMPLES batch', () => {
  for (const sample of CODEX_SAMPLES) {
    it(`detect: ${sample.id} — ${sample.description}`, () => {
      const r = smartDetectUrlAndKey(sample.text);
      if (sample.expect.authOnly) {
        expect(r.apiKey?.startsWith(sample.expect.apiKeyPrefix!)).toBe(true);
        expect(r.matchedDetectors).toContain('codex-auth-json');
        return;
      }
      expect(r.baseUrl).toBe(sample.expect.baseUrl);
      expect(r.model).toBe(sample.expect.model);
      expect(r.providerSlug).toBe(sample.expect.providerSlug);
      if (sample.expect.reasoningEffort) {
        expect(r.reasoningEffort).toBe(sample.expect.reasoningEffort);
      }
      if (sample.expect.wireApi) {
        expect(r.wireApi).toBe(sample.expect.wireApi);
      }
      if (sample.expect.apiKeyPrefix) {
        expect(r.apiKey?.startsWith(sample.expect.apiKeyPrefix)).toBe(true);
      }
      expect(r.rawConfigText).toBeTruthy();
      expect(r.matchedDetectors).toContain('codex-config-toml');
    });

    it(`applySmartPaste codex: ${sample.id}`, () => {
      const r = applySmartPaste('codex', sample.text);
      expect(r.configFormat).toBe('toml');
      if (sample.expect.authOnly) {
        expect(r.vars.apiKey?.startsWith(sample.expect.apiKeyPrefix!)).toBe(true);
        // 仅 key：不把 OPENAI_API_KEY 写进 config.toml
        expect(r.configText).not.toMatch(/OPENAI_API_KEY/);
        return;
      }
      expect(r.vars.baseUrl).toBe(sample.expect.baseUrl);
      expect(r.vars.model).toBe(sample.expect.model);
      expect(r.vars.providerSlug).toBe(sample.expect.providerSlug);
      expect(r.configText).toContain(
        `[model_providers.${sample.expect.providerSlug}]`,
      );
      expect(r.configText).toContain(`base_url = "${sample.expect.baseUrl}"`);
      for (const snip of sample.expect.preserveSnippets ?? []) {
        expect(r.configText).toContain(snip);
      }
      if (sample.expect.apiKeyPrefix) {
        expect(r.vars.apiKey?.startsWith(sample.expect.apiKeyPrefix)).toBe(true);
        expect(r.configText).not.toMatch(/OPENAI_API_KEY\s*=/);
      }
    });
  }
});

