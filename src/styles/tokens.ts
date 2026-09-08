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
 * - Page background → `--bg-canvas` / `--bg-subtle` from `CANVAS_PALETTES` + `html[data-canvas]` (light theme only)
 * - TS that needs hex (contrast, charts) → `agentHex()` / `THEME`
 * - Agent meta, dots, logos, endpoint paths → `agentCssVar(id)`
 *   (surfaces pick an Agent id; they do not copy hex)
 *
 * Docs: `docs/ui-design.md` §2
 */

export type ThemeScheme = 'light' | 'dark';

/**
 * Switchable product accent fill. Pages should use `--accent` plus the
 * hover / pressed / foreground / subtle / text companions in `ACCENT_STATES`.
 * Packaged installer / OS icons stay the bundled PNG (not the live accent).
 */
export const ACCENT_PALETTES = {
  indigo: { light: '#4f46e5', dark: '#a5b4fc' },
  blue: { light: '#2563eb', dark: '#93c5fd' },
  teal: { light: '#0f766e', dark: '#5eead4' },
  rose: { light: '#e11d48', dark: '#fda4af' },
  amber: { light: '#c2410c', dark: '#fdba74' },
} as const;

export type AccentId = keyof typeof ACCENT_PALETTES;
export const DEFAULT_ACCENT_ID = 'blue' as const satisfies AccentId;
export const ACCENT_IDS = Object.keys(ACCENT_PALETTES) as AccentId[];

export function isAccentId(value: string): value is AccentId {
  return (ACCENT_IDS as readonly string[]).includes(value);
}

/** Full accent ramp for buttons, links, focus, and selected tints. */
export type AccentTone = {
  fill: string;
  hover: string;
  pressed: string;
  foreground: string;
  subtle: string;
  text: string;
};

export const ACCENT_STATES: Record<AccentId, Record<ThemeScheme, AccentTone>> = {
  indigo: {
    light: {
      fill: '#4f46e5',
      hover: '#4338ca',
      pressed: '#3730a3',
      foreground: '#ffffff',
      subtle: '#eef2ff',
      text: '#4338ca',
    },
    dark: {
      fill: '#a5b4fc',
      hover: '#c7d2fe',
      pressed: '#818cf8',
      foreground: '#1e1b4b',
      subtle: '#252343',
      text: '#c7d2fe',
    },
  },
  blue: {
    light: {
      fill: '#2563eb',
      hover: '#1d4ed8',
      pressed: '#1e40af',
      foreground: '#ffffff',
      subtle: '#eff6ff',
      text: '#1d4ed8',
    },
    dark: {
      fill: '#93c5fd',
      hover: '#bfdbfe',
      pressed: '#60a5fa',
      foreground: '#172554',
      subtle: '#17263b',
      text: '#bfdbfe',
    },
  },
  teal: {
    light: {
      fill: '#0f766e',
      hover: '#115e59',
      pressed: '#134e4a',
      foreground: '#ffffff',
      subtle: '#f0fdfa',
      text: '#115e59',
    },
    dark: {
      fill: '#5eead4',
      hover: '#99f6e4',
      pressed: '#2dd4bf',
      foreground: '#042f2e',
      subtle: '#102e2b',
      text: '#99f6e4',
    },
  },
  rose: {
    light: {
      fill: '#e11d48',
      hover: '#be123c',
      pressed: '#9f1239',
      foreground: '#ffffff',
      subtle: '#fff1f2',
      text: '#be123c',
    },
    dark: {
      fill: '#fda4af',
      hover: '#fecdd3',
      pressed: '#fb7185',
      foreground: '#4c0519',
      subtle: '#351c2a',
      text: '#fecdd3',
    },
  },
  amber: {
    light: {
      fill: '#c2410c',
      hover: '#9a3412',
      pressed: '#7c2d12',
      foreground: '#ffffff',
      subtle: '#fff7ed',
      text: '#9a3412',
    },
    dark: {
      fill: '#fdba74',
      hover: '#fed7aa',
      pressed: '#fb923c',
      foreground: '#431407',
      subtle: '#332315',
      text: '#fed7aa',
    },
  },
};

/**
 * Light page backgrounds. Never used as a dark-theme override.
 * `canvas` is the page; `subtle` is a slightly deeper strip of the same tint.
 */
export const CANVAS_PALETTES = {
  gray: { canvas: '#f3f3f5', subtle: '#ececef' },
  white: { canvas: '#fafafa', subtle: '#f0f0f0' },
  paper: { canvas: '#f6f3ee', subtle: '#eee8e0' },
  mist: { canvas: '#eef2f6', subtle: '#e4eaf0' },
  sky: { canvas: '#eef5fb', subtle: '#e3eef8' },
  mint: { canvas: '#eef8f3', subtle: '#e1f0e8' },
  sand: { canvas: '#f6f0e6', subtle: '#eee6d8' },
  lilac: { canvas: '#f4f1f8', subtle: '#ebe6f2' },
} as const;

export type CanvasId = keyof typeof CANVAS_PALETTES;
export const DEFAULT_CANVAS_ID = 'gray' as const satisfies CanvasId;
export const CANVAS_IDS = Object.keys(CANVAS_PALETTES) as CanvasId[];

export function isCanvasId(value: string): value is CanvasId {
  return (CANVAS_IDS as readonly string[]).includes(value);
}

/**
 * Semantic theme colors. Keys map to CSS vars `--{key}`.
 * Surfaces (change here, every page follows):
 * - `bg-canvas` — window chrome: sidebar, session rail, top bar
 * - `bg-panel` — main stage, cards, dialogs
 * - `bg-subtle` — inset strips, table heads (not a second page color)
 */
export const THEME = {
  light: {
    'bg-canvas': '#f3f3f5',
    'bg-panel': '#ffffff',
    'bg-subtle': '#ececef',
    'bg-hover': '#ebebed',
    'bg-active': '#e4e4e7',
    border: '#e6e6e9',
    'border-strong': '#d6d6da',
    'text-primary': '#18181b',
    'text-secondary': '#55555d',
    'text-muted': '#62626c',
    'text-disabled': '#a1a1aa',
    'border-control': '#85858f',
    accent: ACCENT_STATES[DEFAULT_ACCENT_ID].light.fill,
    'accent-hover': ACCENT_STATES[DEFAULT_ACCENT_ID].light.hover,
    'accent-pressed': ACCENT_STATES[DEFAULT_ACCENT_ID].light.pressed,
    'accent-foreground': ACCENT_STATES[DEFAULT_ACCENT_ID].light.foreground,
    'accent-subtle': ACCENT_STATES[DEFAULT_ACCENT_ID].light.subtle,
    'accent-text': ACCENT_STATES[DEFAULT_ACCENT_ID].light.text,
    success: '#15803d',
    'success-subtle': '#f0fdf4',
    warning: '#92400e',
    'warning-subtle': '#fffbeb',
    danger: '#b91c1c',
    'danger-subtle': '#fef2f2',
    info: '#1d4ed8',
    'info-subtle': '#eff6ff',
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
    'text-muted': '#a1a1aa',
    'text-disabled': '#52525b',
    'border-control': '#71717a',
    accent: ACCENT_STATES[DEFAULT_ACCENT_ID].dark.fill,
    'accent-hover': ACCENT_STATES[DEFAULT_ACCENT_ID].dark.hover,
    'accent-pressed': ACCENT_STATES[DEFAULT_ACCENT_ID].dark.pressed,
    'accent-foreground': ACCENT_STATES[DEFAULT_ACCENT_ID].dark.foreground,
    'accent-subtle': ACCENT_STATES[DEFAULT_ACCENT_ID].dark.subtle,
    'accent-text': ACCENT_STATES[DEFAULT_ACCENT_ID].dark.text,
    success: '#86efac',
    'success-subtle': '#142a20',
    warning: '#fcd34d',
    'warning-subtle': '#302510',
    danger: '#fca5a5',
    'danger-subtle': '#321b22',
    info: '#93c5fd',
    'info-subtle': '#17263b',
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
  /** Kiro app icon fill `#9046FF`. */
  kiro: { light: '#9046ff', dark: '#a78bfa' },
} as const;

export type TokenAgentId = keyof typeof AGENT_COLORS;

/** Brand-color keys derived from {@link AGENT_COLORS} — not a product set. */
export const TOKEN_AGENT_IDS = Object.keys(AGENT_COLORS) as TokenAgentId[];

/** Radii → `--radius-sm` / `--radius` / `--radius-lg` / `--radius-mark` */
export const RADIUS = {
  sm: '8px',
  DEFAULT: '12px',
  lg: '16px',
  /** App-icon squircle (AppLogo, AgentLogo). Not a layout radius. */
  mark: '22%',
} as const;

/**
 * UI 字号五档。
 *
 * | 标准 | class | 像素 | 用途 |
 * | display | `text-display` | 22 | 对话空态主句、首次引导 |
 * | title | `text-title` | 18 | 对话框标题；壳顶栏页名走 `pageRhythm.pageTitle`（headline） |
 * | headline | `text-headline` | 15 | 分区标题、总览关键数字 |
 * | body | `text-body` | 14 | 正文、按钮、列表名、表单值 |
 * | meta | `text-meta` | 12 | 表头、时间、路径、角标、说明 |
 */
export const TYPE_SCALE = {
  display: { size: '22px', lineHeight: '1.27' },
  title: { size: '18px', lineHeight: '1.33' },
  headline: { size: '15px', lineHeight: '1.47' },
  body: { size: '14px', lineHeight: '1.57' },
  meta: { size: '12px', lineHeight: '1.5' },
} as const;

export type TypeScaleRole = keyof typeof TYPE_SCALE;

/**
 * 旧 Tailwind 名 → 三档标准。像素与标准相同，不是额外字号。
 * 新代码优先写 `text-display` / `text-title` / `text-headline` / `text-body` / `text-meta`。
 * `cn()` 已把这些档注册为 font-size，避免和 `text-primary` 互斥。
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
  radius: '8px',
  hoverShadow: 'none',
} as const;

/**
 * Lucide sizes. Nav keeps a slightly thinner stroke so 18px marks stay sharp.
 * Chrome = top bar / send / icon-only tools. Inline = chevrons, row actions, status.
 */
export const ICON = {
  nav: { px: 18, stroke: 1.6 },
  chrome: { px: 16, stroke: 1.75, className: 'h-4 w-4' },
  inline: { px: 14, stroke: 1.75, className: 'h-3.5 w-3.5' },
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

function accentOverrideDecls(tone: AccentTone): string[] {
  return [
    `--accent: ${tone.fill};`,
    `--accent-hover: ${tone.hover};`,
    `--accent-pressed: ${tone.pressed};`,
    `--accent-foreground: ${tone.foreground};`,
    `--accent-subtle: ${tone.subtle};`,
    `--accent-text: ${tone.text};`,
  ];
}

/** `[data-accent]` overrides for the accent ramp. Default blue is already in `:root` / `.dark`. */
export function buildAccentOverrideCss(): string {
  return ACCENT_IDS.flatMap((id) => [
    `:root[data-accent="${id}"] {`,
    cssDecls(accentOverrideDecls(ACCENT_STATES[id].light)),
    '}',
    `html.dark[data-accent="${id}"], .dark[data-accent="${id}"] {`,
    cssDecls(accentOverrideDecls(ACCENT_STATES[id].dark)),
    '}',
  ]).join('\n');
}

/** `[data-canvas]` overrides page gray in light theme only. */
export function buildCanvasOverrideCss(): string {
  return CANVAS_IDS.flatMap((id) => {
    const swatch = CANVAS_PALETTES[id];
    return [
      `:root[data-canvas="${id}"] {`,
      `  --bg-canvas: ${swatch.canvas};`,
      `  --bg-subtle: ${swatch.subtle};`,
      '}',
    ];
  }).join('\n');
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
    buildCanvasOverrideCss(),
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
    buildCanvasOverrideCss(),
  ].join('\n');
}
