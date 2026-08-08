/**
 * Design tokens — **single source of truth** for theme colors, agent brand
 * colors, radii, and shadows.
 *
 * Runtime CSS variables are generated from this module:
 * - full set → `virtual:agenthub-design-tokens.css` (Vite plugin)
 * - boot-critical subset → `index.html` via `transformIndexHtml`
 *
 * Consumers:
 * - CSS / Tailwind → `var(--…)` (see `tailwind.config.ts`)
 * - TS that needs hex (contrast, charts) → `agentHex()` / `THEME`
 * - Agent meta → `agentCssVar(id)` for style bindings
 *
 * Docs: `docs/ui-design.md` §2
 */

export type ThemeScheme = 'light' | 'dark';

/** Agent ids that own a brand color (keep in sync with `AgentId`). */
export const TOKEN_AGENT_IDS = [
  'claude',
  'codex',
  'kimi',
  'grok',
  'pi',
  'workbuddy',
  'cursor',
] as const;

export type TokenAgentId = (typeof TOKEN_AGENT_IDS)[number];

/** Semantic theme colors. Keys map to CSS vars `--{key}`. */
export const THEME = {
  light: {
    'bg-canvas': '#f7f7f8',
    'bg-panel': '#ffffff',
    'bg-subtle': '#f1f1f3',
    'bg-hover': '#ebebed',
    'bg-active': '#e4e4e7',
    border: '#e6e6e9',
    'border-strong': '#d6d6da',
    'text-primary': '#18181b',
    'text-secondary': '#55555d',
    'text-muted': '#70707a',
    'text-disabled': '#a1a1aa',
    accent: '#4f46e5',
    success: '#16a34a',
    warning: '#c2740c',
    danger: '#dc2626',
    info: '#2563eb',
  },
  dark: {
    'bg-canvas': '#0a0a0b',
    'bg-panel': '#121214',
    'bg-subtle': '#1a1a1d',
    'bg-hover': '#1e1e22',
    'bg-active': '#2c2c31',
    border: '#27272a',
    'border-strong': '#3f3f46',
    'text-primary': '#fafafa',
    'text-secondary': '#a1a1aa',
    'text-muted': '#8b8b96',
    'text-disabled': '#52525b',
    accent: '#6366f1',
    success: '#22c55e',
    warning: '#f59e0b',
    danger: '#ef4444',
    info: '#3b82f6',
  },
} as const satisfies Record<ThemeScheme, Record<string, string>>;

/**
 * Agent brand colors (logo dots, chart series, accents).
 * Edit here only — light/dark both flow into CSS vars and TS helpers.
 */
export const AGENT_COLORS = {
  claude: { light: '#d97757', dark: '#d97757' },
  codex: { light: '#10a37f', dark: '#10a37f' },
  kimi: { light: '#7c6cff', dark: '#8b7cff' },
  grok: { light: '#000000', dark: '#000000' },
  pi: { light: '#0ea5e9', dark: '#38bdf8' },
  workbuddy: { light: '#0052d9', dark: '#3b82f6' },
  cursor: { light: '#f54e00', dark: '#ff6b2c' },
} as const satisfies Record<TokenAgentId, { light: string; dark: string }>;

/** Radii → `--radius-sm` / `--radius` / `--radius-lg` */
export const RADIUS = {
  sm: '6px',
  DEFAULT: '8px',
  lg: '12px',
} as const;

export const SHADOWS = {
  light: {
    xs: '0 1px 2px rgba(0, 0, 0, 0.04)',
    sm: '0 1px 3px rgba(0, 0, 0, 0.06), 0 1px 2px rgba(0, 0, 0, 0.03)',
    md: '0 4px 12px rgba(0, 0, 0, 0.08)',
    lg: '0 16px 48px rgba(0, 0, 0, 0.16)',
  },
  dark: {
    xs: '0 1px 2px rgba(0, 0, 0, 0.2)',
    sm: '0 1px 3px rgba(0, 0, 0, 0.28), 0 1px 2px rgba(0, 0, 0, 0.18)',
    md: '0 4px 12px rgba(0, 0, 0, 0.4)',
    lg: '0 16px 48px rgba(0, 0, 0, 0.55)',
  },
} as const satisfies Record<ThemeScheme, Record<string, string>>;

/** CSS custom property for an agent brand color. */
export function agentCssVar(id: TokenAgentId): `var(--agent-${TokenAgentId})` {
  return `var(--agent-${id})`;
}

/** Resolved hex for charts / contrast (defaults to light scheme). */
export function agentHex(id: TokenAgentId, scheme: ThemeScheme = 'light'): string {
  return AGENT_COLORS[id][scheme];
}

/** All agent CSS vars in catalog order (BootSplash dots, etc.). */
export const AGENT_COLOR_VARS: ReadonlyArray<`var(--agent-${TokenAgentId})`> = TOKEN_AGENT_IDS.map(
  (id) => agentCssVar(id),
);

/** Map `var(--agent-*)` → light hex (contrast fallbacks before computed style). */
export const AGENT_CSS_VAR_TO_HEX_LIGHT: Readonly<Record<string, string>> = Object.fromEntries(
  TOKEN_AGENT_IDS.map((id) => [agentCssVar(id), AGENT_COLORS[id].light]),
);

function cssDecls(lines: string[]): string {
  return lines.map((line) => `  ${line}`).join('\n');
}

function themeDecls(scheme: ThemeScheme): string[] {
  const theme = THEME[scheme];
  const shadows = SHADOWS[scheme];
  const lines: string[] = [];

  for (const [key, value] of Object.entries(theme)) {
    lines.push(`--${key}: ${value};`);
  }
  for (const id of TOKEN_AGENT_IDS) {
    lines.push(`--agent-${id}: ${AGENT_COLORS[id][scheme]};`);
  }
  if (scheme === 'light') {
    lines.push(`--radius-sm: ${RADIUS.sm};`);
    lines.push(`--radius: ${RADIUS.DEFAULT};`);
    lines.push(`--radius-lg: ${RADIUS.lg};`);
  }
  for (const [key, value] of Object.entries(shadows)) {
    lines.push(`--shadow-${key}: ${value};`);
  }
  return lines;
}

/** Full design-token CSS for the app bundle (`:root` + `.dark`). */
export function buildDesignTokensCss(): string {
  return [
    '/* AUTO-GENERATED from src/styles/tokens.ts — edit tokens.ts only */',
    ':root {',
    cssDecls(themeDecls('light')),
    '}',
    '',
    '.dark {',
    cssDecls(themeDecls('dark')),
    '}',
    '',
  ].join('\n');
}

/**
 * Minimal vars for `index.html` boot splash (paint before app CSS).
 * Keep in sync with BootSplash / #boot-fallback needs.
 */
export function buildBootCriticalCss(): string {
  const lightKeys = [
    'bg-canvas',
    'bg-subtle',
    'text-primary',
    'text-muted',
    'border',
    'accent',
  ] as const;
  const darkKeys = lightKeys;

  const lightLines = [
    ...lightKeys.map((k) => `--${k}: ${THEME.light[k]};`),
    ...TOKEN_AGENT_IDS.map((id) => `--agent-${id}: ${AGENT_COLORS[id].light};`),
  ];
  const darkLines = [
    ...darkKeys.map((k) => `--${k}: ${THEME.dark[k]};`),
    ...TOKEN_AGENT_IDS.map((id) => `--agent-${id}: ${AGENT_COLORS[id].dark};`),
  ];

  return [
    ':root {',
    cssDecls(lightLines),
    '}',
    'html.dark {',
    cssDecls(darkLines),
    '}',
  ].join('\n');
}
