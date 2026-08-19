import { interpolate, type MessageKey, type MessageParams, type TranslateFn } from '@/lib/i18n';
import type { AgentProcessView } from '@/lib/chat-process';
import type { ChatMessage } from '@/lib/types';

function tx(t: TranslateFn | undefined, key: MessageKey, fallback: string, params?: MessageParams): string {
  return t ? t(key, params) : interpolate(fallback, params);
}

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
  const json = configText.match(/"(?:model|default_model)"\s*:\s*"([^"]+)"/);
  return json?.[1] ?? null;
}

export function relativeTime(iso: string, t?: TranslateFn): string {
  const parsed = Date.parse(iso.includes('T') ? iso : iso.replace(' ', 'T') + 'Z');
  if (Number.isNaN(parsed)) return '';
  const diff = Date.now() - parsed;
  const m = Math.floor(diff / 60000);
  if (m < 1) return tx(t, 'common.relativeJustNow', '刚刚');
  if (m < 60) return tx(t, 'common.relativeMinutes', '{n} 分钟前', { n: m });
  const h = Math.floor(m / 60);
  if (h < 24) return tx(t, 'common.relativeHours', '{n} 小时前', { n: h });
  const d = Math.floor(h / 24);
  if (d < 7) return tx(t, 'common.relativeDays', '{n} 天前', { n: d });
  return new Date(parsed).toLocaleDateString();
}
