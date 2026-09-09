import { agentDisplayName } from '@/config/agents';
import {
  clipPreviewText,
  formatJsonPayload,
  looksLikeJsonObject,
  tryPrettyJson,
} from '@/lib/source-preview';
import { formatSessionRecordText } from '@/lib/session-record-text';
import type { MessageKey, TranslateFn } from '@/lib/i18n';

const CHAT_FAILURE_KEY = {
  missingEnv: 'chat.failure.missingEnv',
  modelUnavailable: 'chat.failure.modelUnavailable',
  loginExpired: 'chat.failure.loginExpired',
  modelRetired: 'chat.failure.modelRetired',
  thinkingUnsupported: 'chat.failure.thinkingUnsupported',
  usageLimit: 'chat.failure.usageLimit',
  sendFailed: 'chat.failure.sendFailed',
  garbledOutput: 'chat.failure.garbledOutput',
  interrupted: 'chat.turnOutcome.interruptedHint',
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

const PROCESS_TEXT_LIMIT = 4000;

/** Pin a process/thinking overflow pane to the newest line. */
export function pinElementScrollToBottom(
  el: { scrollTop: number; scrollHeight: number } | null,
): void {
  if (!el) return;
  el.scrollTop = el.scrollHeight;
}

/** Keep the newest process/thinking text when the log is too long. */
export function clipProcessTail(text: string, limit = PROCESS_TEXT_LIMIT): string {
  if (text.length <= limit) return text;
  return `…${text.slice(-limit)}`;
}

/** Drop CSI/OSC/cursor sequences so headless CLI chrome is not shown as 乱码. */
export function stripTerminalEscapes(text: string): string {
  let out = '';
  for (let i = 0; i < text.length; i += 1) {
    const code = text.charCodeAt(i);
    if (code === 0x1b) {
      const next = text[i + 1];
      if (next === '[') {
        i += 2;
        while (i < text.length) {
          const ch = text.charCodeAt(i);
          i += 1;
          if (ch >= 0x40 && ch <= 0x7e) break;
        }
        i -= 1;
        continue;
      }
      if (next === ']') {
        i += 2;
        while (i < text.length) {
          if (text.charCodeAt(i) === 0x07) {
            i += 1;
            break;
          }
          if (text.charCodeAt(i) === 0x1b && text[i + 1] === '\\') {
            i += 2;
            break;
          }
          i += 1;
        }
        i -= 1;
        continue;
      }
      i += next ? 1 : 0;
      continue;
    }
    if (code === 0x9b) {
      i += 1;
      while (i < text.length) {
        const ch = text.charCodeAt(i);
        i += 1;
        if (ch >= 0x40 && ch <= 0x7e) break;
      }
      i -= 1;
      continue;
    }
    if (code < 0x20 && code !== 9 && code !== 10 && code !== 13) continue;
    out += text[i];
  }
  return out;
}

/**
 * Headless Kiro (and similar TUI CLIs) prefix a colored `>` and cursor restore.
 * Same bytes on Windows / macOS / Linux; TERM=dumb does not always stop color.
 */
export function sanitizeCliChatText(text: string): string {
  const hadEsc = /[\u001b\u009b]/.test(text);
  let out = stripTerminalEscapes(text)
    .replace(/^\uFEFF/, '')
    .replace(/\r\n/g, '\n')
    .replace(/\r/g, '\n');
  if (hadEsc) out = out.replace(/^>\s?/, '');
  return out.replace(/^\n+|\n+$/g, '');
}

export function formatChatSessionRecord(turns: TurnGroup[], userLabel: string): string {
  const lines = [];
  for (const g of turns) {
    const user = g.user?.content?.trim();
    if (user) lines.push({ speaker: userLabel, text: user });
    for (const m of g.agents) {
      const text = sanitizeCliChatText(m.content ?? '').trim();
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

/** Live settings.json is the current Pi model. Envelope leftover must not win. */
export function resolvePiChatCurrentModel(
  liveChatModel: string | null | undefined,
): string | null {
  const id = liveChatModel?.trim() || null;
  if (!id || isRetiredChatModel(id)) return null;
  return id;
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

const PROTOCOL_EVENT_TYPES = new Set([
  'session',
  'agent_start',
  'turn_start',
  'message_start',
  'message_update',
  'message_end',
  'agent_end',
  'turn_end',
  'agent_settled',
]);

type ChatToolDump = {
  title: string;
  purpose?: string;
  command?: string;
  path?: string;
};

const TOOL_DUMP_HINT =
  /"_tool_use_purpose"\s*:|"oldStr"\s*:|"newStr"\s*:|"command"\s*:\s*"strReplace"/;

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

function jsonStringField(record: Record<string, unknown>, key: string): string | undefined {
  const value = record[key];
  return typeof value === 'string' && value.trim() ? value.trim() : undefined;
}

function extractJsonStringField(source: string, key: string): string | undefined {
  const match = source.match(new RegExp(`"${key}"\\s*:\\s*"((?:\\\\.|[^"\\\\])*)"`));
  if (!match) return undefined;
  try {
    const parsed = JSON.parse(`"${match[1]}"`) as unknown;
    return typeof parsed === 'string' && parsed.trim() ? parsed.trim() : undefined;
  } catch {
    const fallback = match[1].replace(/\\n/g, '\n').replace(/\\t/g, '\t').trim();
    return fallback || undefined;
  }
}

function isToolUseObject(record: Record<string, unknown>): boolean {
  if (jsonStringField(record, '_tool_use_purpose')) return true;
  const command = jsonStringField(record, 'command') ?? jsonStringField(record, 'name');
  return Boolean(
    command &&
      (jsonStringField(record, 'path') ||
        jsonStringField(record, 'filePath') ||
        jsonStringField(record, 'file_path') ||
        jsonStringField(record, 'oldStr') ||
        jsonStringField(record, 'newStr')),
  );
}

function splitTitleAndJson(text: string): { title: string; json: string } | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  if (looksLikeJsonObject(trimmed) && /^[[{][\s\n]*"/.test(trimmed)) {
    return { title: '', json: trimmed };
  }
  const jsonAt = trimmed.search(/\{\s*"/);
  if (jsonAt <= 0) return null;
  const title = trimmed.slice(0, jsonAt).trim();
  const json = trimmed.slice(jsonAt).trim();
  if (!title || title.length > 200) return null;
  if (title.startsWith('#') || title.startsWith('```') || title.startsWith('|')) return null;
  if (!looksLikeJsonObject(json) || json.length < 20) return null;
  return { title, json };
}

function tryParseObject(text: string): Record<string, unknown> | null {
  try {
    return asRecord(JSON.parse(text));
  } catch {
    return null;
  }
}

function parseChatToolDump(text: string): ChatToolDump | null {
  const split = splitTitleAndJson(text);
  if (!split) return null;
  const parsed = tryParseObject(split.json);
  if (parsed && isToolUseObject(parsed)) {
    return {
      title: split.title,
      purpose: jsonStringField(parsed, '_tool_use_purpose'),
      command: jsonStringField(parsed, 'command') ?? jsonStringField(parsed, 'name'),
      path:
        jsonStringField(parsed, 'path') ??
        jsonStringField(parsed, 'filePath') ??
        jsonStringField(parsed, 'file_path'),
    };
  }
  if (!TOOL_DUMP_HINT.test(split.json)) return null;
  return {
    title: split.title,
    purpose: extractJsonStringField(split.json, '_tool_use_purpose'),
    command:
      extractJsonStringField(split.json, 'command') ?? extractJsonStringField(split.json, 'name'),
    path:
      extractJsonStringField(split.json, 'path') ??
      extractJsonStringField(split.json, 'filePath') ??
      extractJsonStringField(split.json, 'file_path'),
  };
}

function markdownFence(lang: string, body: string): string {
  let ticks = '```';
  while (body.includes(ticks)) ticks += '`';
  return `${ticks}${lang}\n${body}\n${ticks}`;
}

function formatToolDumpMarkdown(dump: ChatToolDump): string {
  const parts: string[] = [];
  if (dump.title) parts.push(`**${dump.title.replace(/\*/g, '')}**`);
  if (dump.purpose) parts.push(dump.purpose);
  const meta = [dump.command, dump.path]
    .filter((item): item is string => Boolean(item))
    .map((item) => `\`${item}\``)
    .join(' ');
  if (meta) parts.push(meta);
  return parts.join('\n\n');
}

/**
 * Kiro (and similar) sometimes writes a file-edit JSON blob into the assistant
 * body. Keep the bubble as a short readable summary instead of a wall of `\n`.
 */
export function formatChatDisplayContent(text: string): string {
  const dump = parseChatToolDump(text);
  if (dump && (dump.title || dump.purpose || dump.command || dump.path)) {
    return formatToolDumpMarkdown(dump);
  }
  const pretty = tryPrettyJson(text.trim());
  if (pretty) return markdownFence('json', clipPreviewText(pretty));
  return text;
}

/** True when assistant content is a Pi/Grok NDJSON dump, not a reply. */
export function looksLikeChatProtocolDump(text: string): boolean {
  const first = text
    .trim()
    .split(/\r?\n/)
    .find((line) => line.trim().length > 0);
  if (!first?.startsWith('{')) return false;
  try {
    const value = JSON.parse(first) as { type?: unknown; jsonrpc?: unknown; method?: unknown };
    if (typeof value.type === 'string' && PROTOCOL_EVENT_TYPES.has(value.type)) return true;
    return Boolean(value.jsonrpc && value.method);
  } catch {
    return false;
  }
}

/** Surface a localized failure instead of the raw provider dump. Never include the user prompt. */
export function localizeChatFailure(text: string, t?: TranslateFn): string {
  const hay = text.toLowerCase();
  const copy = (key: keyof typeof CHAT_FAILURE_KEY, zh: string) =>
    t ? t(CHAT_FAILURE_KEY[key]) : zh;
  if (
    hay.includes('runtime interrupted')
    || hay.includes('chat.runtime.interrupted')
    || hay.includes('codex process stopped')
    || hay.includes('codex thread is unavailable')
  ) {
    return copy('interrupted', 'Codex 进程或线程不可用，请开新一轮继续。');
  }
  if (hay.includes('missing environment variable')) {
    return copy('missingEnv', '这份登录还在用另一份 API Key 配置，没法发。请点重试。');
  }
  if (
    hay.includes('is not supported by any configured account')
    || hay.includes('model_unavailable')
    || (hay.includes('not supported') && hay.includes('chatgpt account'))
  ) {
    return copy('modelUnavailable', '这个模型当前登录用不了。请换一个模型后重试。');
  }
  if (
    hay.includes('usagelimitexceeded')
    || hay.includes('usage limit')
    || hay.includes('hit your usage limit')
  ) {
    return copy('usageLimit', '这份登录暂时没法继续，请稍后再试。');
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
  if (
    hay.includes('unrecognized')
    || hay.includes('无法识别的输出')
    || hay.includes('non-json line')
    || hay.includes('非 json')
  ) {
    return copy('garbledOutput', '有一段输出没法展示。可以重试。');
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
