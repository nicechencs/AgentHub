import type { TranslateFn } from '@/lib/i18n';
import type { AgentProcessView } from '@/lib/chat-process';
import type { ChatMessage } from '@/lib/types';

export type TurnGroup = {
  turn: number;
  user?: ChatMessage;
  agents: ChatMessage[];
};

export function formatStepInput(input: unknown): string | null {
  if (input == null) return null;
  try {
    const s = typeof input === 'string' ? input : JSON.stringify(input);
    return s.length > 240 ? `${s.slice(0, 240)}…` : s;
  } catch {
    return String(input);
  }
}

export function formatDurationMs(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const m = Math.floor(ms / 60_000);
  const s = Math.round((ms % 60_000) / 1000);
  return `${m}m ${s}s`;
}

/** Chrome for a thinking episode — live timer vs collapsed “thought for”. */
export function thinkingChromeLabel(done: boolean, elapsedMs: number, t: TranslateFn): string {
  if (!done) return t('chat.process.thinkingLive', { duration: formatDurationMs(elapsedMs) });
  if (elapsedMs > 0) return t('chat.process.thinkingFor', { duration: formatDurationMs(elapsedMs) });
  return t('chat.process.thinkingDone');
}

export function isProcessActivePhase(phase: AgentProcessView['phase']): boolean {
  return phase === 'queued' || phase === 'starting' || phase === 'running';
}

export function isProcessErrorPhase(phase: AgentProcessView['phase']): boolean {
  return phase === 'failed' || phase === 'timeout';
}

export function groupByTurn(messages: ChatMessage[]): TurnGroup[] {
  const map = new Map<number, TurnGroup>();
  for (const m of messages) {
    let g = map.get(m.turn);
    if (!g) {
      g = { turn: m.turn, agents: [] };
      map.set(m.turn, g);
    }
    if (m.role === 'user') g.user = m;
    else g.agents.push(m);
  }
  return [...map.values()].sort((a, b) => a.turn - b.turn);
}

/** 从 provider 配置文本里尽量抽出 model 名 */
export function extractModel(configText: string): string | null {
  const toml = configText.match(/(?:^|\n)\s*(?:model|default_model)\s*=\s*"([^"]+)"/m);
  if (toml?.[1]) return toml[1];
  const json = configText.match(/"(?:model|default_model|defaultModel)"\s*:\s*"([^"]+)"/);
  return json?.[1] ?? null;
}

const RETIRED_OPENROUTER_BACKUP = /^stealth\/ox(?:-alpha)?$/i;

export function isRetiredChatModel(model: string): boolean {
  return RETIRED_OPENROUTER_BACKUP.test(model.trim());
}

/** Chat model options: keep first-seen order, drop empty and retired stealth backups. */
export function chatModelOptions(ids: readonly string[], current?: string | null): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const raw of ids) {
    const id = raw.trim();
    if (!id || seen.has(id) || isRetiredChatModel(id)) continue;
    seen.add(id);
    out.push(id);
  }
  if (out.length > 0) return out;
  const fallback = (current ?? '').trim();
  if (fallback && !isRetiredChatModel(fallback)) return [fallback];
  return [];
}

/** Surface a Chinese failure instead of the raw English provider dump. Never include the user prompt. */
export function localizeChatFailure(text: string): string {
  const hay = text.toLowerCase();
  if (hay.includes('missing environment variable')) {
    return '这份登录还在用另一份 API Key 配置，没法发。请点重试。';
  }
  if (hay.includes('is not supported by any configured account') || hay.includes('model_unavailable')) {
    return '这个模型当前登录用不了。请换一个模型后重试。';
  }
  if (
    hay.includes('stealth/ox')
    || hay.includes('stealth ox')
    || ((hay.includes('"code":404') || hay.includes('"code": 404') || hay.includes(' 404:'))
      && (hay.includes('model') || hay.includes('retired') || hay.includes('glm-5.3') || hay.includes('stealth')))
  ) {
    return '这个模型已经下架或当前登录用不了。请换一个模型后重试。';
  }
  return text;
}

export function relativeTime(iso: string, t: TranslateFn): string {
  const parsed = Date.parse(iso.includes('T') ? iso : iso.replace(' ', 'T') + 'Z');
  if (Number.isNaN(parsed)) return '';
  const diff = Date.now() - parsed;
  const m = Math.floor(diff / 60000);
  if (m < 1) return t('common.relativeJustNow');
  if (m < 60) return t('common.relativeMinutes', { n: m });
  const h = Math.floor(m / 60);
  if (h < 24) return t('common.relativeHours', { n: h });
  const d = Math.floor(h / 24);
  if (d < 7) return t('common.relativeDays', { n: d });
  return new Date(parsed).toLocaleDateString();
}
