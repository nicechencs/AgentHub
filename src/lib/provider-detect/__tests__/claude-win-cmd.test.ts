import { describe, expect, it } from 'vitest';
import { applySmartPaste, smartDetectUrlAndKey } from '../index';
import { CLAUDE_CODE_UI_WIN_CMD_DUAL } from './fixtures/claude-code-samples';

describe('Windows CMD dual-block', () => {
  it('detects from set + settings.json with UI chrome', () => {
    const r = smartDetectUrlAndKey(CLAUDE_CODE_UI_WIN_CMD_DUAL);
    expect(r.baseUrl).toMatch(/^https:\/\//);
    expect(r.apiKey).toMatch(/^sk-/);
    expect(r.extraEnv?.CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC).toMatch(/^1$/);
  });

  it('applySmartPaste writes env', () => {
    const r = applySmartPaste('claude', CLAUDE_CODE_UI_WIN_CMD_DUAL);
    const env = JSON.parse(r.configText).env as Record<string, string>;
    expect(env.ANTHROPIC_BASE_URL).toMatch(/^https:\/\//);
    expect(env.ANTHROPIC_AUTH_TOKEN).toMatch(/^sk-/);
    expect(env.CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC).toBe('1');
  });
});
