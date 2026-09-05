/**
 * 「导入到 Agent」from a Sub2API key — same menu/eligibility as entry API keys,
 * but the draft points at the Sub2API gateway instead of the local loopback.
 */
import { sub2apiKeyToConnectDraft } from '@/lib/sub2api/sync';
import type { Sub2ApiGroup, Sub2ApiKey } from '@/lib/sub2api';
import type { ConnectApiKeyDraft } from '@/lib/connect-flow/connect-intent';
import type { TranslateFn } from '@/lib/i18n';
import { LOCAL_ENDPOINT_KINDS, type LocalEndpointKind } from '@/lib/route-endpoints';
import type { AgentKey } from '@/lib/types';
import {
  agentCanReceiveTokenImport,
  tokenImportAgentChoice,
  tokenImportGate,
  type TokenImportAgentRef,
  type TokenImportGate,
} from '@/pages/routes/tokens/token-import-model';
import { pickGroupPlatform } from './sub2api-page-model';

export type Sub2ApiImportKind = LocalEndpointKind | 'any';

export function sub2apiImportKind(platform?: string | null): Sub2ApiImportKind {
  const p = (platform ?? '').trim().toLowerCase();
  if (p === 'anthropic') return 'messages';
  if (p === 'grok') return 'responses_grok';
  if (
    p === 'openai'
    || p === 'kimi'
    || p === 'zhipu'
    || p === 'deepseek'
    || p === 'gemini'
    || p === 'antigravity'
  ) {
    return 'chat_completions';
  }
  return 'any';
}

export function sub2apiImportKindForKey(
  key: Sub2ApiKey,
  groups: readonly Sub2ApiGroup[] = [],
): Sub2ApiImportKind {
  return sub2apiImportKind(pickGroupPlatform(key, groups));
}

function firstModel(key: Sub2ApiKey): string {
  const record = key as unknown as Record<string, unknown>;
  const raw = record.models ?? record.model_list ?? record.allowed_models;
  if (Array.isArray(raw)) {
    const first = raw.find((item) => typeof item === 'string' && item.trim());
    return typeof first === 'string' ? first.trim() : '';
  }
  if (typeof raw === 'string' && raw.trim()) {
    return raw.split(',')[0]?.trim() ?? '';
  }
  return '';
}

export function sub2apiImportDraft(
  gatewayBaseUrl: string,
  key: Sub2ApiKey,
  agentId: AgentKey,
  kind: Sub2ApiImportKind,
): ConnectApiKeyDraft | null {
  const apiKey = key.key?.trim();
  if (!apiKey) return null;
  const draft = sub2apiKeyToConnectDraft(gatewayBaseUrl, { key: apiKey, name: key.name });
  const model = firstModel(key);
  return {
    ...draft,
    ...(model ? { model } : {}),
    ...(agentId === 'grok'
      ? { apiBackend: kind === 'chat_completions' ? 'chat_completions' : 'responses' }
      : {}),
  };
}

export function sub2apiImportGate(
  key: Pick<Sub2ApiKey, 'key'>,
  kind: Sub2ApiImportKind,
  agents: readonly TokenImportAgentRef[],
  t?: TranslateFn,
): TokenImportGate {
  const token = key.key?.trim() ?? '';
  if (kind !== 'any') {
    return tokenImportGate({ kind, token, unavailable: false }, agents, t);
  }
  const choices = agents.map((agent) => {
    const can = LOCAL_ENDPOINT_KINDS.some((spec) => agentCanReceiveTokenImport(agent.id, spec.kind));
    if (can) return { ...agent, enabled: true, reason: null };
    return tokenImportAgentChoice('chat_completions', agent, t);
  });
  if (!token) {
    return {
      enabled: false,
      reason: t ? t('routes.tokens.importNeedKey') : '先有入口 Key 才能导入',
      agents: choices,
    };
  }
  if (agents.length === 0) {
    return {
      enabled: false,
      reason: t ? t('routes.tokens.importNeedAgent') : '先安装 Agent',
      agents: choices,
    };
  }
  return { enabled: true, reason: null, agents: choices };
}
