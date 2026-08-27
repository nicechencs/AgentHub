/**
 * OpenAI-compatible remote model list helpers (pure; no network).
 *
 * URL normalize + list parse are mirrored in
 * `crates/agenthub-core/src/utils/remote_openai_models.rs`.
 */
import type { AgentId } from '@/lib/types';
import { officialApiDefaults } from '@/config/official-api';
import { smartDetectUrlAndKey } from './detect';
import { extractFormVars, looksRedactedOrPlaceholder } from './fields';
import { REDACTED_MARKER, type ProviderFormVars } from './types';

/** Agents without an official template (pi / workbuddy / cursor, …). */
export const FALLBACK_CUSTOM_MODEL = 'custom-model';

/** Build GET URL for `{base}/v1/models`, collapsing a trailing `/v1`. */
export function openaiModelsUrl(baseUrl: string): string {
  const trimmed = baseUrl.trim();
  if (!trimmed) return '';
  const stripped = trimmed.replace(/\/+$/, '').replace(/\/anthropic$/i, '');
  try {
    const host = new URL(stripped).host.toLowerCase();
    if (host === 'api.deepseek.com') return `${stripped}/models`;
  } catch {
    /* fall through */
  }
  if (/\/v1$/i.test(stripped)) return `${stripped}/models`;
  return `${stripped}/v1/models`;
}

/** Drop models that belong to another product so Claude/Kimi do not list grok-*. */
export function filterRemoteModelsForAgent(agentId: AgentId, ids: readonly string[]): string[] {
  const list = ids.map((id) => id.trim()).filter(Boolean);
  if (list.length === 0) return [];
  const grok = (id: string) => /^grok[-_]/i.test(id);
  const kimi = (id: string) => /kimi|moonshot/i.test(id);
  const claude = (id: string) => /claude|anthropic|sonnet|opus|haiku|fable/i.test(id);
  const deepseek = (id: string) => /deepseek/i.test(id);
  let kept: string[] = list;
  if (agentId === 'kimi') kept = list.filter(kimi);
  else if (agentId === 'claude') kept = list.filter(claude);
  else if (agentId === 'dsh') kept = list.filter(deepseek);
  else if (agentId === 'grok') kept = list.filter(grok);
  if (kept.length > 0) return kept;
  if (agentId === 'kimi' || agentId === 'claude') {
    const withoutGrok = list.filter((id) => !grok(id));
    if (withoutGrok.length > 0) return withoutGrok;
  }
  return [...list];
}

function pushModelId(out: string[], seen: Set<string>, raw: unknown): void {
  if (typeof raw !== 'string') return;
  const id = raw.trim();
  if (!id || seen.has(id)) return;
  seen.add(id);
  out.push(id);
}

function pushFromArray(out: string[], seen: Set<string>, items: unknown[]): void {
  for (const item of items) {
    if (typeof item === 'string') {
      pushModelId(out, seen, item);
      continue;
    }
    if (item && typeof item === 'object' && 'id' in item) {
      pushModelId(out, seen, (item as { id: unknown }).id);
    }
  }
}

/**
 * Accept `{data:[{id}]}`, `data: string[]`, `{models:string[]}`,
 * `{models:[{id}]}`, or a top-level array. Dedupe, first-seen order.
 */
export function parseOpenAiModelList(input: unknown): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  if (Array.isArray(input)) {
    pushFromArray(out, seen, input);
    return out;
  }
  if (!input || typeof input !== 'object') return out;
  const obj = input as Record<string, unknown>;
  if (Array.isArray(obj.data)) pushFromArray(out, seen, obj.data);
  if (Array.isArray(obj.models)) pushFromArray(out, seen, obj.models);
  return out;
}

/** Last 4 chars only. Short keys and `***` → empty. Never returns the full key. */
export function maskApiKeyLast4(key: string): string {
  const trimmed = key.trim();
  if (!trimmed || trimmed === REDACTED_MARKER || trimmed.length < 8) return '';
  return trimmed.slice(-4);
}

export function defaultModelForAgent(agentId: AgentId): string {
  return officialApiDefaults(agentId)?.model ?? FALLBACK_CUSTOM_MODEL;
}

export function resolveModelForSave(
  agentId: AgentId,
  model: string,
  useOfficial: boolean,
): string {
  if (useOfficial) {
    const official = officialApiDefaults(agentId);
    if (official) return official.model;
  }
  return model.trim();
}

/**
 * Lock the official model when that switch is on.
 * Custom connections keep an empty model empty — do not invent a default id.
 */
export function withDefaultModel(
  agentId: AgentId,
  vars: ProviderFormVars,
  useOfficial: boolean,
): ProviderFormVars {
  const model = resolveModelForSave(agentId, vars.model, useOfficial);
  if (model === vars.model) return vars;
  return { ...vars, model };
}

/** Last-4 display masks are not a live key (`**abcd` / `****wxyz` / `sk--••••wxyz`). */
export function looksLikeLast4Mask(value: string): boolean {
  const t = value.trim();
  if (!t) return false;
  if (/^\*{2,}[A-Za-z0-9]{4}$/.test(t)) return true;
  if (/--[•…]{2,}[A-Za-z0-9]{4}$/.test(t)) return true;
  return /[•…]/.test(t) && /[A-Za-z0-9]{4}$/.test(t);
}

/** True when the form holds a newly pasted secret, not `***` / last4 / empty. */
export function isLivePastedApiKey(apiKey: string): boolean {
  const key = apiKey.trim();
  if (!key || key === REDACTED_MARKER) return false;
  if (looksRedactedOrPlaceholder(key) || looksLikeLast4Mask(key)) return false;
  return true;
}

function isHttpUrl(value: string): boolean {
  return /^https?:\/\//i.test(value.trim());
}

/** Named URL keys in advanced JSON / env / TOML (never invents a placeholder). */
const ADVANCED_URL_RES: RegExp[] = [
  /"baseURL"\s*:\s*"(https?:\/\/[^"]+)"/,
  /"baseUrl"\s*:\s*"(https?:\/\/[^"]+)"/,
  /"base_url"\s*:\s*"(https?:\/\/[^"]+)"/i,
  /(?:ANTHROPIC_BASE_URL|OPENAI_BASE_URL)\s*["']?\s*[:=]\s*["']?(https?:\/\/[^\s"',}\\]+)/i,
  /^\s*base_url\s*=\s*"(https?:\/\/[^"]+)"/im,
];

function scanAdvancedConfigForBaseUrl(configText: string): string {
  for (const re of ADVANCED_URL_RES) {
    const match = configText.match(re);
    const hit = match?.[1]?.trim() ?? '';
    if (hit && isHttpUrl(hit)) return hit;
  }
  return '';
}

/**
 * Upstream URL for a custom login: simple service-address field, else advanced config.
 * Empty simple field is OK when JSON `baseURL` / TOML `base_url` / env already has http(s).
 */
export function resolveUpstreamBaseUrl(args: {
  formBaseUrl: string;
  configText: string;
  configFormat: 'json' | 'toml';
  agentId: AgentId;
}): string {
  const fromForm = args.formBaseUrl.trim();
  if (isHttpUrl(fromForm)) return fromForm;

  const extracted = extractFormVars(args.agentId, args.configText, args.configFormat).baseUrl.trim();
  if (isHttpUrl(extracted)) return extracted;

  const detected = (smartDetectUrlAndKey(args.configText).baseUrl ?? '').trim();
  if (isHttpUrl(detected)) return detected;

  return scanAdvancedConfigForBaseUrl(args.configText);
}

export function shouldFetchRemoteModels(args: {
  useOfficial: boolean;
  baseUrl: string;
  apiKey: string;
  /** Edit mode: saved provider id is present (secret stays on the hub). */
  hasStoredSecret?: boolean;
}): boolean {
  if (args.useOfficial) return false;
  const baseUrl = args.baseUrl.trim();
  if (!baseUrl || !isHttpUrl(baseUrl)) return false;
  if (isLivePastedApiKey(args.apiKey)) return true;
  return Boolean(args.hasStoredSecret);
}

export type RemoteModelsStatusKind = 'idle' | 'loading' | 'failed' | 'empty' | 'ready';

export type RemoteModelsStatusLabelKey =
  | 'connections.providerDialog.remoteModelsLoading'
  | 'connections.providerDialog.remoteModelsFailed'
  | 'connections.providerDialog.remoteModelsEmpty';

export type RemoteModelsStatusView = {
  kind: RemoteModelsStatusKind;
  showRetry: boolean;
  showPicker: boolean;
  labelKey: RemoteModelsStatusLabelKey | null;
};

/**
 * Pure view-model for the model-field fetch status.
 * `active: false` is official / gate-closed (idle, no chrome).
 */
export function remoteModelsStatusView(args: {
  loading: boolean;
  error: boolean;
  ids: readonly string[];
  active?: boolean;
}): RemoteModelsStatusView {
  if (args.active === false) {
    return { kind: 'idle', showRetry: false, showPicker: false, labelKey: null };
  }
  if (args.loading) {
    return {
      kind: 'loading',
      showRetry: false,
      showPicker: false,
      labelKey: 'connections.providerDialog.remoteModelsLoading',
    };
  }
  if (args.error) {
    return {
      kind: 'failed',
      showRetry: true,
      showPicker: false,
      labelKey: 'connections.providerDialog.remoteModelsFailed',
    };
  }
  if (args.ids.length > 0) {
    return { kind: 'ready', showRetry: false, showPicker: true, labelKey: null };
  }
  return {
    kind: 'empty',
    showRetry: false,
    showPicker: false,
    labelKey: 'connections.providerDialog.remoteModelsEmpty',
  };
}
