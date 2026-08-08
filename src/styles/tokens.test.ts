import { describe, expect, it } from 'vitest';
import {
  AGENT_COLORS,
  AGENT_COLOR_VARS,
  AGENT_CSS_VAR_TO_HEX_LIGHT,
  TOKEN_AGENT_IDS,
  THEME,
  agentCssVar,
  agentHex,
  buildBootCriticalCss,
  buildDesignTokensCss,
} from './tokens';

describe('design tokens SSOT', () => {
  it('exposes a brand color for every catalog agent', () => {
    for (const id of TOKEN_AGENT_IDS) {
      expect(AGENT_COLORS[id].light).toMatch(/^#[0-9a-fA-F]{6}$/);
      expect(AGENT_COLORS[id].dark).toMatch(/^#[0-9a-fA-F]{6}$/);
      expect(agentCssVar(id)).toBe(`var(--agent-${id})`);
      expect(agentHex(id)).toBe(AGENT_COLORS[id].light);
      expect(agentHex(id, 'dark')).toBe(AGENT_COLORS[id].dark);
    }
  });

  it('maps CSS vars to light hex without a second palette', () => {
    expect(AGENT_COLOR_VARS).toHaveLength(TOKEN_AGENT_IDS.length);
    for (const id of TOKEN_AGENT_IDS) {
      expect(AGENT_CSS_VAR_TO_HEX_LIGHT[agentCssVar(id)]).toBe(AGENT_COLORS[id].light);
    }
  });

  it('builds :root / .dark CSS from THEME + AGENT_COLORS', () => {
    const css = buildDesignTokensCss();
    expect(css).toContain(':root {');
    expect(css).toContain('.dark {');
    expect(css).toContain(`--bg-canvas: ${THEME.light['bg-canvas']};`);
    expect(css).toContain(`--bg-canvas: ${THEME.dark['bg-canvas']};`);
    expect(css).toContain(`--agent-grok: ${AGENT_COLORS.grok.light};`);
    expect(css).toContain(`--agent-kimi: ${AGENT_COLORS.kimi.dark};`);
    expect(css).toContain('--radius-sm:');
    expect(css).toContain('--shadow-md:');
  });

  it('builds boot-critical CSS for index.html injection', () => {
    const boot = buildBootCriticalCss();
    expect(boot).toContain(':root {');
    expect(boot).toContain('html.dark {');
    expect(boot).toContain(`--accent: ${THEME.light.accent};`);
    expect(boot).toContain(`--agent-claude: ${AGENT_COLORS.claude.light};`);
    expect(boot).toContain(`--agent-claude: ${AGENT_COLORS.claude.dark};`);
  });
});
