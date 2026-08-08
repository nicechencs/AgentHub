import { describe, expect, it } from 'vitest';
import { applySmartPaste, smartDetectUrlAndKey } from '../index';
import { CLAUDE_CODE_UI_DUAL_BLOCK } from './fixtures/claude-code-samples';

describe('claude dual-block UI paste', () => {
  it('detects url key flags from messy dual paste', () => {
    const r = smartDetectUrlAndKey(CLAUDE_CODE_UI_DUAL_BLOCK);
    expect(r.baseUrl).toMatch(/^https:\/\//);
    expect(r.apiKey).toMatch(/^sk-/);
    expect(r.claudeAuthEnv).toBe('ANTHROPIC_AUTH_TOKEN');
    expect(r.extraEnv?.CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC).toMatch(/^1$/);
    expect(r.extraEnv?.CLAUDE_CODE_ATTRIBUTION_HEADER).toMatch(/^0$/);
    expect(r.suggestedName).toBeTruthy();
  });

  it('applySmartPaste writes settings env', () => {
    const r = applySmartPaste('claude', CLAUDE_CODE_UI_DUAL_BLOCK);
    const env = JSON.parse(r.configText).env as Record<string, string>;
    expect(env.ANTHROPIC_BASE_URL).toMatch(/^https:\/\//);
    expect(env.ANTHROPIC_AUTH_TOKEN).toMatch(/^sk-/);
    expect(env.CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC).toBe('1');
    expect(env.CLAUDE_CODE_ATTRIBUTION_HEADER).toBe('0');
  });
});
