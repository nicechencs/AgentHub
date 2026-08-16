import { describe, expect, it } from 'vitest';
import {
  AGENT_COLORS,
  AGENT_COLOR_VARS,
  AGENT_CSS_VAR_TO_HEX_LIGHT,
  TOKEN_AGENT_IDS,
  THEME,
  TYPE_SCALE,
  TYPE_SCALE_ALIASES,
  agentCssVar,
  agentHex,
  buildBootCriticalCss,
  buildDesignTokensCss,
  buildTailwindFontSize,
  typeScaleTw,
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
    expect(css).toContain(`--font-body-size: ${TYPE_SCALE.body.size};`);
    expect(css).toContain(`--font-title-leading: ${TYPE_SCALE.title.lineHeight};`);
  });

  it('builds boot-critical CSS for index.html injection', () => {
    const boot = buildBootCriticalCss();
    expect(boot).toContain(':root {');
    expect(boot).toContain('html.dark {');
    expect(boot).toContain(`--accent: ${THEME.light.accent};`);
    expect(boot).toContain(`--agent-claude: ${AGENT_COLORS.claude.light};`);
    expect(boot).toContain(`--agent-claude: ${AGENT_COLORS.claude.dark};`);
    expect(boot).toContain(`--font-meta-size: ${TYPE_SCALE.meta.size};`);
  });
});

describe('TYPE_SCALE (docs/ui-design.md §2)', () => {
  it('keeps exactly three distinct pixel sizes', () => {
    const roles = Object.keys(TYPE_SCALE);
    expect(roles).toEqual(['title', 'body', 'meta']);
    const sizes = new Set(Object.values(TYPE_SCALE).map((spec) => spec.size));
    expect(sizes).toEqual(new Set(['16px', '13px', '12px']));
  });

  it('maps legacy Tailwind names onto the three standards', () => {
    expect(TYPE_SCALE_ALIASES).toEqual({
      lg: 'title',
      xl: 'title',
      sm: 'body',
      base: 'body',
      xs: 'meta',
      '2xs': 'meta',
    });
    const fontSize = buildTailwindFontSize();
    expect(fontSize.title).toEqual(typeScaleTw('title'));
    expect(fontSize.lg).toEqual(fontSize.title);
    expect(fontSize.xl).toEqual(fontSize.title);
    expect(fontSize.sm).toEqual(fontSize.body);
    expect(fontSize.base).toEqual(fontSize.body);
    expect(fontSize.xs).toEqual(fontSize.meta);
    expect(fontSize['2xs']).toEqual(fontSize.meta);
    const distinctPx = new Set(Object.values(fontSize).map(([size]) => size));
    expect(distinctPx).toEqual(new Set(['16px', '13px', '12px']));
  });
});
