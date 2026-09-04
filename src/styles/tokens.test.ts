import { describe, expect, it } from 'vitest';
import {
  ACCENT_PALETTES,
  ACCENT_IDS,
  CANVAS_PALETTES,
  CANVAS_IDS,
  DEFAULT_CANVAS_ID,
  buildCanvasOverrideCss,
  AGENT_COLORS,
  AGENT_COLOR_VARS,
  AGENT_CSS_VAR_TO_HEX_LIGHT,
  DEFAULT_ACCENT_ID,
  TOKEN_AGENT_IDS,
  THEME,
  TYPE_SCALE,
  TYPE_SCALE_ALIASES,
  agentCssVar,
  agentHex,
  buildAccentOverrideCss,
  resolveChartColor,
  buildBootCriticalCss,
  buildDesignTokensCss,
  buildTailwindFontSize,
  BUTTON,
  RADIUS,
  TOOLTIP,
  typeScalePx,
  typeScaleTw,
} from './tokens';

describe('design tokens SSOT', () => {
  it('derives brand-color keys from AGENT_COLORS, not a second product list', () => {
    expect(TOKEN_AGENT_IDS).toEqual(Object.keys(AGENT_COLORS));
  });

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

  it('resolves chart colors from agent CSS vars to scheme hex', () => {
    for (const id of TOKEN_AGENT_IDS) {
      expect(resolveChartColor(agentCssVar(id))).toBe(AGENT_COLORS[id].light);
      expect(resolveChartColor(agentCssVar(id), 'dark')).toBe(AGENT_COLORS[id].dark);
    }
    expect(resolveChartColor('var(--text-muted)', 'dark')).toBe(THEME.dark['text-muted']);
    expect(resolveChartColor('#d97757')).toBe('#d97757');
  });

  it('keeps page canvas quieter than card panel so one THEME change restyles the app', () => {
    expect(THEME.light['bg-panel']).toBe('#ffffff');
    expect(THEME.light['bg-canvas']).not.toBe(THEME.light['bg-panel']);
    expect(THEME.light['bg-subtle']).not.toBe(THEME.light['bg-canvas']);
    expect(THEME.dark['bg-canvas']).not.toBe(THEME.dark['bg-panel']);
  });

  it('keeps THEME.accent aligned with the default indigo palette', () => {
    expect(THEME.light.accent).toBe(ACCENT_PALETTES[DEFAULT_ACCENT_ID].light);
    expect(THEME.dark.accent).toBe(ACCENT_PALETTES[DEFAULT_ACCENT_ID].dark);
  });

  it('emits light-only data-canvas overrides and keeps the default aligned', () => {
    expect(CANVAS_PALETTES[DEFAULT_CANVAS_ID].canvas).toBe(THEME.light['bg-canvas']);
    expect(CANVAS_PALETTES[DEFAULT_CANVAS_ID].subtle).toBe(THEME.light['bg-subtle']);
    const css = buildCanvasOverrideCss();
    for (const id of CANVAS_IDS) {
      expect(css).toContain(`:root[data-canvas="${id}"]`);
      expect(css).toContain(`--bg-canvas: ${CANVAS_PALETTES[id].canvas};`);
    }
    expect(css).not.toContain('html.dark[data-canvas');
  });

  it('emits data-accent overrides for every palette', () => {
    const css = buildAccentOverrideCss();
    for (const id of ACCENT_IDS) {
      expect(css).toContain(`:root[data-accent="${id}"]`);
      expect(css).toContain(`--accent: ${ACCENT_PALETTES[id].light};`);
      expect(css).toContain(`--accent: ${ACCENT_PALETTES[id].dark};`);
    }
  });

  it('builds :root / .dark CSS from THEME + AGENT_COLORS', () => {
    const css = buildDesignTokensCss();
    expect(css).toContain(':root {');
    expect(css).toContain('.dark {');
    expect(css).toContain(':root[data-accent="teal"]');
    expect(css).toContain(`--bg-canvas: ${THEME.light['bg-canvas']};`);
    expect(css).toContain(`--bg-canvas: ${THEME.dark['bg-canvas']};`);
    expect(css).toContain(`--agent-grok: ${AGENT_COLORS.grok.light};`);
    expect(css).toContain(`--agent-kimi: ${AGENT_COLORS.kimi.dark};`);
    expect(css).toContain('--radius-sm:');
    expect(css).toContain('--radius-mark:');
    expect(css).toContain('--shadow-md:');
    expect(css).toContain(`--font-body-size: ${TYPE_SCALE.body.size};`);
    expect(css).toContain(`--font-title-leading: ${TYPE_SCALE.title.lineHeight};`);
    expect(css).toContain(`--tooltip-max-width: ${TOOLTIP.maxWidth};`);
    expect(css).toContain(`--tooltip-max-height: ${TOOLTIP.maxHeight};`);
    expect(css).toContain(`--tooltip-pad-x: ${TOOLTIP.paddingX};`);
    expect(css).toContain(`--tooltip-pad-y: ${TOOLTIP.paddingY};`);
  });

  it('builds boot-critical CSS for index.html injection', () => {
    const boot = buildBootCriticalCss();
    expect(boot).toContain(':root {');
    expect(boot).toContain('html.dark {');
    expect(boot).toContain(`--accent: ${THEME.light.accent};`);
    expect(boot).toContain(':root[data-accent="blue"]');
    expect(boot).toContain(`--agent-claude: ${AGENT_COLORS.claude.light};`);
    expect(boot).toContain(`--agent-claude: ${AGENT_COLORS.claude.dark};`);
    expect(boot).toContain(`--font-meta-size: ${TYPE_SCALE.meta.size};`);
    expect(boot).toContain(`--radius-mark: ${RADIUS.mark};`);
  });
});

describe('RADIUS (docs/ui-design.md §2)', () => {
  it('keeps three px steps plus the product-mark squircle', () => {
    expect(RADIUS).toEqual({
      sm: '6px',
      DEFAULT: '8px',
      lg: '12px',
      mark: '22%',
    });
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
    expect(typeScalePx('title')).toBe(16);
    expect(typeScalePx('body')).toBe(13);
    expect(typeScalePx('meta')).toBe(12);
  });
});

describe('BUTTON (docs/ui-design.md §2)', () => {
  it('keeps two heights, 4px-ladder padding, and no hover shadow', () => {
    expect(BUTTON.height.default).toBe(28);
    expect(BUTTON.height.lg).toBe(32);
    expect(BUTTON.padX.sm % 4).toBe(0);
    expect(BUTTON.padX.default % 4).toBe(0);
    expect(BUTTON.padX.lg % 4).toBe(0);
    expect(BUTTON.hoverShadow).toBe('none');
    expect(BUTTON.radius).toBe('6px');
  });
});
