/**
 * Design tokens — **single source of truth** for theme colors, agent brand
 * colors, radii, shadows, and the three UI type sizes.
 *
 * Runtime CSS variables are generated from this module:
 * - full set → `virtual:agenthub-design-tokens.css` (Vite plugin)
 * - boot-critical subset → `index.html` via `transformIndexHtml`
 *
 * Consumers:
 * - CSS / Tailwind → `var(--…)` (see `tailwind.config.ts`)
 * - Product accent → `--accent` from `ACCENT_PALETTES` + `html[data-accent]`
 * - TS that needs hex (contrast, charts) → `agentHex()` / `THEME`
 * - Agent meta, dots, logos, endpoint paths → `agentCssVar(id)`
 *   (surfaces pick an Agent id; they do not copy hex)
 *
 * Docs: `docs/ui-design.md` §2
 */

export type ThemeScheme = 'light' | 'dark';

/**
 * Switchable product accent. `--accent` is the only runtime color pages should
 * use for primary actions, checked switches, focus rings, and the in-app mark.
 * Installer / OS icons stay the default indigo PNG.
 */
export const ACCENT_PALETTES = {
  indigo: { light: '#4f46e5', dark: '#6366f1' },
  blue: { light: '#2563eb', dark: '#3b82f6' },
  teal: { light: '#0f766e', dark: '#14b8a6' },
  rose: { light: '#e11d48', dark: '#f43f5e' },
  amber: { light: '#c2410c', dark: '#ea580c' },
} as const;

export type AccentId = keyof typeof ACCENT_PALETTES;
export const DEFAULT_ACCENT_ID = 'indigo' as const satisfies AccentId;
export const ACCENT_IDS = Object.keys(ACCENT_PALETTES) as AccentId[];

export function isAccentId(value: string): value is AccentId {
  return (ACCENT_IDS as readonly string[]).includes(value);
}

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
    accent: ACCENT_PALETTES[DEFAULT_ACCENT_ID].light,
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
    accent: ACCENT_PALETTES[DEFAULT_ACCENT_ID].dark,
    success: '#22c55e',
    warning: '#f59e0b',
    danger: '#ef4444',
    info: '#3b82f6',
  },
} as const satisfies Record<ThemeScheme, Record<string, string>>;

/**
 * Agent brand colors (logo dots, chart series, accents).
 * Edit here only — light/dark both flow into CSS vars and TS helpers.
 * Keys are color slots, not the product catalog.
 */
export const AGENT_COLORS = {
  /** Claude Code SVG fill `#D97757`. */
  claude: { light: '#d97757', dark: '#d97757' },
  /** Codex cloud gradient mid-stop `#7189ff` (lavender → `#3438f5`). */
  codex: { light: '#7189ff', dark: '#8b9bff' },
  /** Kimi K-only brand `#1783FF`. */
  kimi: { light: '#1783ff', dark: '#3d94ff' },
  /** White mark on black; invert in dark so dots stay visible. */
  grok: { light: '#111111', dark: '#f5f5f5' },
  /** Pi mark is black; invert in dark. */
  pi: { light: '#111111', dark: '#f5f5f5' },
  /** WorkBuddy disc gradient `#0EC8A9` → `#01C886`. */
  workbuddy: { light: '#0ec8a9', dark: '#2dd4bf' },
  /** Cursor cube is near-black; SVG fill `#edecec` on dark. */
  cursor: { light: '#171717', dark: '#edecec' },
  /** DeepSeek SVG fill `#4D6BFE`. */
  dsh: { light: '#4d6bfe', dark: '#6b8cff' },
  /** ZCode mark is black; invert in dark. */
  zcode: { light: '#171717', dark: '#e5e5e5' },
} as const;

export type TokenAgentId = keyof typeof AGENT_COLORS;

/** Brand-color keys derived from {@link AGENT_COLORS} — not a product set. */
export const TOKEN_AGENT_IDS = Object.keys(AGENT_COLORS) as TokenAgentId[];

/** Radii → `--radius-sm` / `--radius` / `--radius-lg` / `--radius-mark` */
export const RADIUS = {
  sm: '6px',
  DEFAULT: '8px',
  lg: '12px',
  /** Product-mark squircle (AppLogo). Not a fourth px step. */
  mark: '22%',
} as const;

/**
 * UI 字号只保留三档。不要再加第四个像素值。
 *
 * | 标准 | class | 像素 | 用途 |
 * | title | `text-title` | 16 | 页标题、空态主句、指标数字 |
 * | body | `text-body` | 13 | 正文、按钮、列表名、段标题（加字重） |
 * | meta | `text-meta` | 12 | 次级说明、表头、路径、角标、眉题 |
 */
export const TYPE_SCALE = {
  title: { size: '16px', lineHeight: '1.35' },
  body: { size: '13px', lineHeight: '1.45' },
  meta: { size: '12px', lineHeight: '1.4' },
} as const;

export type TypeScaleRole = keyof typeof TYPE_SCALE;

/**
 * 旧 Tailwind 名 → 三档标准。像素与标准相同，不是额外字号。
 * 新代码优先写 `text-title` / `text-body` / `text-meta`。
 * `cn()` 已把这三档注册为 font-size，避免和 `text-primary` 互斥。
 */
export const TYPE_SCALE_ALIASES = {
  lg: 'title',
  xl: 'title',
  sm: 'body',
  base: 'body',
  xs: 'meta',
  '2xs': 'meta',
} as const satisfies Record<string, TypeScaleRole>;

export function typeScaleTw(role: TypeScaleRole): [string, { lineHeight: string }] {
  const spec = TYPE_SCALE[role];
  return [spec.size, { lineHeight: spec.lineHeight }];
}

/** 给 Recharts 等不能写 Tailwind class 的地方用。 */
export function typeScalePx(role: TypeScaleRole): number {
  return Number.parseInt(TYPE_SCALE[role].size, 10);
}

/** Tailwind `theme.extend.fontSize`：三档标准 + 同像素别名。 */
export function buildTailwindFontSize(): Record<string, [string, { lineHeight: string }]> {
  const fontSize: Record<string, [string, { lineHeight: string }]> = {};
  for (const role of Object.keys(TYPE_SCALE) as TypeScaleRole[]) {
    fontSize[role] = typeScaleTw(role);
  }
  for (const [alias, role] of Object.entries(TYPE_SCALE_ALIASES)) {
    fontSize[alias] = typeScaleTw(role);
  }
  return fontSize;
}

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

/** Hover tooltip geometry. Chrome is locked in `components/ui/tooltip.tsx`. */
export const TOOLTIP = {
  maxWidth: '280px',
  maxHeight: '192px',
  paddingX: '10px',
  paddingY: '6px',
  sideOffset: 8,
  collisionPadding: 8,
  delayMs: 200,
} as const;

/**
 * Action-button rhythm. Hover is fill/color only — never a shadow.
 * Shadows belong to elevation layers (card / tooltip / popover / dialog),
 * segmented *selected* lift, or always-on overlay FABs.
 * Chrome is locked in `components/ui/button.tsx`.
 */
export const BUTTON = {
  height: { default: 28, lg: 32 },
  padX: { sm: 8, default: 12, lg: 16 },
  radius: '6px',
  hoverShadow: 'none',
} as const;

/** CSS custom property for an agent brand color. */
export function agentCssVar(id: TokenAgentId): `var(--agent-${TokenAgentId})` {
  return `var(--agent-${id})`;
}

/** Resolved hex for charts / contrast (defaults to light scheme). */
export function agentHex(id: TokenAgentId, scheme: ThemeScheme = 'light'): string {
  return AGENT_COLORS[id][scheme];
}

function isTokenAgentId(id: string): id is TokenAgentId {
  return (TOKEN_AGENT_IDS as readonly string[]).includes(id);
}

/**
 * SVG stroke / gradient stops cannot reliably paint `var(--agent-*)`.
 * Resolve catalog CSS vars (and the muted fallback) to the scheme hex.
 */
export function resolveChartColor(color: string, scheme: ThemeScheme = 'light'): string {
  const value = color.trim();
  const agentVar = /^var\(--agent-([a-z0-9]+)\)$/.exec(value);
  if (agentVar && isTokenAgentId(agentVar[1])) {
    return AGENT_COLORS[agentVar[1]][scheme];
  }
  if (value === 'var(--text-muted)') {
    return THEME[scheme]['text-muted'];
  }
  return value;
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
    lines.push(`--radius-mark: ${RADIUS.mark};`);
    for (const [role, spec] of Object.entries(TYPE_SCALE)) {
      lines.push(`--font-${role}-size: ${spec.size};`);
      lines.push(`--font-${role}-leading: ${spec.lineHeight};`);
    }
    lines.push(`--tooltip-max-width: ${TOOLTIP.maxWidth};`);
    lines.push(`--tooltip-max-height: ${TOOLTIP.maxHeight};`);
    lines.push(`--tooltip-pad-x: ${TOOLTIP.paddingX};`);
    lines.push(`--tooltip-pad-y: ${TOOLTIP.paddingY};`);
  }
  for (const [key, value] of Object.entries(shadows)) {
    lines.push(`--shadow-${key}: ${value};`);
  }
  return lines;
}

/** `[data-accent]` overrides for `--accent`. Default indigo is already in `:root` / `.dark`. */
export function buildAccentOverrideCss(): string {
  return ACCENT_IDS.flatMap((id) => [
    `:root[data-accent="${id}"] {`,
    `  --accent: ${ACCENT_PALETTES[id].light};`,
    '}',
    `html.dark[data-accent="${id}"], .dark[data-accent="${id}"] {`,
    `  --accent: ${ACCENT_PALETTES[id].dark};`,
    '}',
  ]).join('\n');
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
    buildAccentOverrideCss(),
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

  const typeScaleLines = Object.entries(TYPE_SCALE).flatMap(([role, spec]) => [
    `--font-${role}-size: ${spec.size};`,
    `--font-${role}-leading: ${spec.lineHeight};`,
  ]);

  const lightLines = [
    ...lightKeys.map((k) => `--${k}: ${THEME.light[k]};`),
    ...TOKEN_AGENT_IDS.map((id) => `--agent-${id}: ${AGENT_COLORS[id].light};`),
    ...typeScaleLines,
    `--radius-mark: ${RADIUS.mark};`,
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
    buildAccentOverrideCss(),
  ].join('\n');
}
