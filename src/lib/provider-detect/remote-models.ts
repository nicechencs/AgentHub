/**
 * OpenAI-compatible remote model list helpers (pure; no network).
 *
 * URL normalize + list parse are mirrored in
 * `crates/agenthub-core/src/utils/remote_openai_models.rs`.
 */
import type { AgentId } from '@/lib/types';
import { officialApiDefaults } from '@/config/official-api';
import { looksRedactedOrPlaceholder } from './fields';
import { REDACTED_MARKER, type ProviderFormVars } from './types';

/** Agents without an official template (pi / workbuddy / cursor, …). */
export const FALLBACK_CUSTOM_MODEL = 'custom-model';

/** Build GET URL for `{base}/v1/models`, collapsing a trailing `/v1`. */
export function openaiModelsUrl(baseUrl: string): string {
  const trimmed = baseUrl.trim();
  if (!trimmed) return '';
  const stripped = trimmed.replace(/\/+$/, '');
  if (/\/v1$/i.test(stripped)) return `${stripped}/models`;
  return `${stripped}/v1/models`;
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
  const trimmed = model.trim();
  return trimmed || defaultModelForAgent(agentId);
}

/** Fill empty `model` (and lock official model) before validate / applyFormVars. */
export function withDefaultModel(
  agentId: AgentId,
  vars: ProviderFormVars,
  useOfficial: boolean,
): ProviderFormVars {
  const model = resolveModelForSave(agentId, vars.model, useOfficial);
  if (model === vars.model) return vars;
  return { ...vars, model };
}

export function shouldFetchRemoteModels(args: {
  useOfficial: boolean;
  baseUrl: string;
  apiKey: string;
}): boolean {
  if (args.useOfficial) return false;
  const baseUrl = args.baseUrl.trim();
  const apiKey = args.apiKey.trim();
  if (!baseUrl || !/^https?:\/\//i.test(baseUrl)) return false;
  if (!apiKey || apiKey === REDACTED_MARKER || looksRedactedOrPlaceholder(apiKey)) {
    return false;
  }
  return true;
}
