/**
 * Readable Chat model / thinking-effort labels.
 * Catalog ids stay the wire value; menus show these names.
 */
import type { MessageKey, TranslateFn } from '@/lib/i18n';

const TOKEN_LABELS: Record<string, string> = {
  gpt: 'GPT',
  grok: 'Grok',
  claude: 'Claude',
  codex: 'Codex',
  sonnet: 'Sonnet',
  opus: 'Opus',
  haiku: 'Haiku',
  mini: 'Mini',
  spark: 'Spark',
  sol: 'Sol',
  fast: 'Fast',
  code: 'Code',
  nano: 'Nano',
  pro: 'Pro',
  max: 'Max',
  thinking: 'Thinking',
  reasoning: 'Reasoning',
  mock: 'Mock',
};

const EFFORT_COPY: Record<string, { label: MessageKey; hint: MessageKey }> = {
  low: { label: 'chat.runtimeOps.effortLow', hint: 'chat.runtimeOps.effortHintLow' },
  medium: { label: 'chat.runtimeOps.effortMedium', hint: 'chat.runtimeOps.effortHintMedium' },
  high: { label: 'chat.runtimeOps.effortHigh', hint: 'chat.runtimeOps.effortHintHigh' },
  xhigh: { label: 'chat.runtimeOps.effortXhigh', hint: 'chat.runtimeOps.effortHintXhigh' },
  max: { label: 'chat.runtimeOps.effortMax', hint: 'chat.runtimeOps.effortHintMax' },
};

function titleToken(token: string): string {
  const lower = token.toLowerCase();
  if (TOKEN_LABELS[lower]) return TOKEN_LABELS[lower];
  if (/^\d+(\.\d+)*$/.test(token)) return token;
  if (token.length <= 3 && /^[a-z]+\d*$/i.test(token)) return token.toUpperCase();
  return token.charAt(0).toUpperCase() + token.slice(1).toLowerCase();
}

/** Turn `gpt-5.3-codex-spark` into `GPT 5.3 Codex Spark`. `auto` uses i18n when `t` is passed. */
export function chatModelDisplayName(id: string, t?: TranslateFn): string {
  const trimmed = id.trim();
  if (!trimmed) return trimmed;
  if (trimmed.toLowerCase() === 'auto') {
    return t ? t('chat.composer.modelAuto') : trimmed;
  }
  return trimmed.split(/[-_]+/).filter(Boolean).map(titleToken).join(' ');
}

export function chatEffortLabel(effort: string, t: TranslateFn): string {
  const trimmed = effort.trim();
  if (!trimmed) return trimmed;
  const mapped = EFFORT_COPY[trimmed.toLowerCase()];
  if (mapped) return t(mapped.label);
  return chatModelDisplayName(trimmed);
}

/** Short wait-time hint, or null when the effort is not a known scale. */
export function chatEffortHint(effort: string, t: TranslateFn): string | null {
  const mapped = EFFORT_COPY[effort.trim().toLowerCase()];
  return mapped ? t(mapped.hint) : null;
}

/** Cmd/Ctrl+Shift+I opens the model menu (Claude Code desktop). */
export function chatModShiftIShouldOpenModel(input: {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  overlayOpen: boolean;
}): boolean {
  if (input.overlayOpen || input.altKey || !input.shiftKey) return false;
  if (input.key !== 'i' && input.key !== 'I') return false;
  return input.metaKey || input.ctrlKey;
}
