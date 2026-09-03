import { agentDisplayName } from '@/config/agents';
import { formatJsonPayload } from '@/lib/source-preview';
import { formatSessionRecordText } from '@/lib/session-record-text';
import type { MessageKey, TranslateFn } from '@/lib/i18n';

const CHAT_FAILURE_KEY = {
  missingEnv: 'chat.failure.missingEnv',
  modelUnavailable: 'chat.failure.modelUnavailable',
  loginExpired: 'chat.failure.loginExpired',
  modelRetired: 'chat.failure.modelRetired',
  thinkingUnsupported: 'chat.failure.thinkingUnsupported',
  sendFailed: 'chat.failure.sendFailed',
} as const satisfies Record<string, MessageKey>;
import type { AgentProcessView } from '@/lib/chat-process';
import type { ChatMessage } from '@/lib/types';

export type TurnGroup = {
  turn: number;
  user?: ChatMessage;
  agents: ChatMessage[];
};

export function formatStepInput(input: unknown): string | null {
  return formatJsonPayload(input);
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

export function formatChatSessionRecord(turns: TurnGroup[], userLabel: string): string {
  const lines = [];
  for (const g of turns) {
    const user = g.user?.content?.trim();
    if (user) lines.push({ speaker: userLabel, text: user });
    for (const m of g.agents) {
      const text = m.content?.trim();
      if (!text) continue;
      lines.push({
        speaker: m.agentId ? agentDisplayName(m.agentId) : '',
        text,
      });
    }
  }
  return formatSessionRecordText(lines);
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

function objectValue(value: unknown): Record<string, unknown> | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
  return value as Record<string, unknown>;
}

function modelIdFromEntry(entry: unknown): string | null {
  if (typeof entry === 'string') {
    const id = entry.trim();
    return id || null;
  }
  const obj = objectValue(entry);
  if (typeof obj?.id === 'string') {
    const id = obj.id.trim();
    return id || null;
  }
  return null;
}

/**
 * Models for the current Pi slot from a live envelope.
 * Uses `settings.defaultProvider` — never the first provider that happens to have a URL.
 */
export function extractPiSlotModels(configText: string): string[] {
  try {
    const root = objectValue(JSON.parse(configText));
    if (!root) return [];
    const modelsObject = objectValue(root.models);
    const providers = objectValue(modelsObject?.providers) ?? objectValue(root.providers);
    if (!providers) return [];
    const settings = objectValue(root.settings);
    const slot =
      typeof settings?.defaultProvider === 'string' ? settings.defaultProvider.trim() : '';
    const chosen = (slot && objectValue(providers[slot]) ? slot : '') || Object.keys(providers)[0];
    if (!chosen) return [];
    const provider = objectValue(providers[chosen]);
    const models = Array.isArray(provider?.models) ? provider.models : [];
    const out: string[] = [];
    const seen = new Set<string>();
    for (const entry of models) {
      const id = modelIdFromEntry(entry);
      if (!id || seen.has(id) || isRetiredChatModel(id)) continue;
      seen.add(id);
      out.push(id);
    }
    return out;
  } catch {
    return [];
  }
}

export function extractPiDefaultProvider(configText: string): string {
  try {
    const root = objectValue(JSON.parse(configText));
    const settings = objectValue(root?.settings);
    if (typeof settings?.defaultProvider === 'string') {
      return settings.defaultProvider.trim();
    }
  } catch {
    /* fall through */
  }
  return '';
}

export function extractPiDefaultModel(configText: string): string | null {
  try {
    const root = objectValue(JSON.parse(configText));
    const settings = objectValue(root?.settings);
    if (typeof settings?.defaultModel === 'string') {
      const id = settings.defaultModel.trim();
      return id || null;
    }
  } catch {
    /* fall through */
  }
  return extractModel(configText);
}

/** Official xAI OpenAI-compatible catalog. Same URL `list_remote_openai_models` uses. */
export const OFFICIAL_XAI_MODELS_BASE = 'https://api.x.ai/v1';

export function officialPiModelsBaseUrl(slot: string): string {
  return slot.trim() === 'xai' ? OFFICIAL_XAI_MODELS_BASE : '';
}

/**
 * Chat 换模型 remote fetch gate. Pi is not skipped — official xAI uses
 * GET {base}/v1/models like every other login.
 */
export function shouldFetchChatRemoteModels(
  providerId: string | undefined | null,
  baseUrl: string | undefined | null,
): boolean {
  return Boolean(providerId?.trim() && baseUrl?.trim());
}

/**
 * Prefer the remote official catalog. Do not fall back to leftover defaultModel
 * when that catalog already loaded.
 */
export function piChatModelOptions(input: {
  remoteModels: readonly string[];
  liveModels: readonly string[];
  envelopeModels: readonly string[];
  currentModel?: string | null;
}): string[] {
  const remote = chatModelOptions(input.remoteModels);
  if (remote.length > 0) return remote;
  const live = chatModelOptions(input.liveModels);
  if (live.length > 0) return live;
  return chatModelOptions(input.envelopeModels, input.currentModel);
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

/** Surface a localized failure instead of the raw provider dump. Never include the user prompt. */
export function localizeChatFailure(text: string, t?: TranslateFn): string {
  const hay = text.toLowerCase();
  const copy = (key: keyof typeof CHAT_FAILURE_KEY, zh: string) =>
    t ? t(CHAT_FAILURE_KEY[key]) : zh;
  if (hay.includes('missing environment variable')) {
    return copy('missingEnv', '这份登录还在用另一份 API Key 配置，没法发。请点重试。');
  }
  if (hay.includes('is not supported by any configured account') || hay.includes('model_unavailable')) {
    return copy('modelUnavailable', '这个模型当前登录用不了。请换一个模型后重试。');
  }
  if (
    hay.includes('oauth refresh failed')
    || hay.includes('invalid_grant')
    || hay.includes('invalid or unknown refresh token')
    || hay.includes('token refresh failed')
  ) {
    return copy('loginExpired', '这份登录已失效，请重新登录后重试。');
  }
  if (
    hay.includes('stealth/ox')
    || hay.includes('stealth ox')
    || ((hay.includes('"code":404') || hay.includes('"code": 404') || hay.includes(' 404:'))
      && (hay.includes('model') || hay.includes('retired') || hay.includes('glm-5.3') || hay.includes('stealth')))
  ) {
    return copy('modelRetired', '这个模型已经下架或当前登录用不了。请换一个模型后重试。');
  }
  if (
    hay.includes('openai api error')
    || hay.includes('does not support parameter')
    || hay.includes('reasoningeffort')
    || hay.includes('reasoning_effort')
    || ((hay.includes('(400)') || hay.includes(' 400:') || hay.includes('http 400'))
      && (hay.includes('api error') || hay.includes('parameter') || hay.includes('model ') || hay.includes('unsupported')))
  ) {
    if (
      hay.includes('reasoningeffort')
      || hay.includes('reasoning_effort')
      || hay.includes('does not support parameter')
    ) {
      return copy('thinkingUnsupported', '这个模型不支持当前思考设置。请点重试。');
    }
    return copy('sendFailed', '这次发送没成功。请点重试。');
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
