/**
 * mock 不是规则真源。规则真源是 core 的 `ADAPTER_CAPABILITY_MATRIX` /
 * `AdapterRouteService` / `AdapterApplyService`。
 */
import {
  type AdapterAction,
  type AdapterBridgeRuntimeStatus,
  type AdapterEvidence,
  type AdapterPlanChange,
  type AdapterProfile,
  type AdapterRouteAnalysis,
} from '@/lib/backend/contracts/adapter';
import type { Account, Provider } from '@/lib/types';

const verifiedAt = '2026-08-12';
export interface MockAdapterState {
  profiles: AdapterProfile[];
  bridgeStatuses: Map<string, AdapterBridgeRuntimeStatus>;
  generatedProviders: Map<string, Provider>;
  removeGeneratedProvider?: (provider: Provider) => void;
}

export interface MockAdapterSourceResolver {
  getAccountById(id: string): Account | undefined;
  getProviderById(id: string): Provider | undefined;
  /** Optional to keep focused route tests independent of mock Connection storage. */
  upsertGeneratedProvider?(provider: Provider): Provider;
  /** Removes only the Adapter-created Connection during reset or a successful remove. */
  removeGeneratedProvider?(provider: Provider): void;
}

export function evidence(label: string, url: string): AdapterEvidence {
  return { label, url, verifiedAt };
}

export function action(
  kind: AdapterAction['kind'],
  target: string,
  description: string,
  value?: string,
): AdapterAction {
  return { kind, target, description, value, secret: false };
}

export function secretAction(target: string, description: string): AdapterAction {
  return { kind: 'reference_connection_secret', target, description, secret: true };
}

export function change(target: string, field: string, value?: string): AdapterPlanChange {
  return { target, field, value, secret: false };
}

export function secretChange(target: string, field: string): AdapterPlanChange {
  return { target, field, secret: true };
}

/** Keep in lockstep with `CODEX_SUBSCRIPTION_TO_CLAUDE_REASON` in agenthub-core. */
export const CODEX_SUBSCRIPTION_TO_CLAUDE_REASON = [
  'Codex / ChatGPT 订阅可通过本机路由到 Claude Code（Messages → Responses）。',
].join('');

export const CLAUDE_SUBSCRIPTION_TO_CODEX_REASON =
  'Claude 订阅 → Codex：产品不做。Codex 不吃 Anthropic PKCE，本产品不走这条边。';

export const CODEX_SUBSCRIPTION_TO_CLAUDE_CANDIDATE_REASON = [
  'Codex / ChatGPT 订阅 → Claude Code：当前不支持。',
  '尚未通过上游授权、条款与协议兼容性门禁，plan.canApply=false。',
  '不会创建适配、启动 Bridge，也不会把订阅凭据写入 Claude。',
  '这只表示没有可执行规则，不代表连接失效。',
  '替代路径：在 Claude 使用自身官方登录，或改用已支持的 API Key 来源。',
].join('');

/** Keep in lockstep with `AGENT_NO_WRITER_REASON` in agenthub-core. */
export const AGENT_NO_WRITER_REASON = '该 Agent 无配置写入能力，不能作为绑定落点';

/** Keep in lockstep with `PROTOCOL_MISMATCH_REASON` in agenthub-core. */
export const PROTOCOL_MISMATCH_REASON =
  '协议不通：票所说的上游协议与该 Agent 所听的入口没有交集。';

/** Keep in lockstep with `SAME_PROTOCOL_NO_EDGE_REASON` in agenthub-core. */
export const SAME_PROTOCOL_NO_EDGE_REASON =
  '同协议但无已验证的边：票与该 Agent 入口相通，但协议图上尚无已验证的适配边。';

export type TicketProtocol =
  | 'anthropic-messages'
  | 'anthropic-pkce'
  | 'openai-chat'
  | 'openai-responses'
  | 'openai-codex-pkce'
  | 'xai-device-code';

/** Keep in lockstep with `agent_bind_capability` in agenthub-core. */
export function agentBindCapability(id: string): { accepts: TicketProtocol[]; writer: boolean } {
  switch (id) {
    case 'claude':
      return { accepts: ['anthropic-messages'], writer: true };
    case 'codex':
      return { accepts: ['openai-responses'], writer: true };
    case 'pi':
      return {
        accepts: [
          'anthropic-messages',
          'openai-chat',
          'anthropic-pkce',
          'openai-codex-pkce',
          'xai-device-code',
        ],
        writer: true,
      };
    case 'grok':
      return { accepts: ['openai-chat'], writer: true };
    case 'kimi':
      return { accepts: ['openai-chat'], writer: true };
    case 'dsh':
      return { accepts: ['openai-chat'], writer: true };
    case 'workbuddy':
      return { accepts: [], writer: true };
    case 'cursor':
      return { accepts: [], writer: false };
    default:
      return { accepts: [], writer: false };
  }
}

export function sourceSpeaks(source: RouteSourceLabel): TicketProtocol[] {
  switch (source) {
    case 'kimi_membership':
    case 'glm_coding_plan':
    case 'deepseek_api':
      return ['anthropic-messages', 'openai-chat'];
    case 'anthropic_api_key':
      return ['anthropic-messages'];
    case 'openai_api_key':
    case 'xai_api_key':
      return ['openai-chat'];
    case 'codex_subscription':
    case 'codex_subscription_oauth_other':
      return ['openai-responses', 'openai-codex-pkce'];
    case 'claude_subscription':
      return ['anthropic-messages', 'anthropic-pkce'];
    case 'grok_xai_subscription':
      return ['openai-chat', 'xai-device-code'];
    default:
      return [];
  }
}

export function unsupportedReasonFromGraph(source: RouteSourceLabel, targetAgentId: string): string {
  const cap = agentBindCapability(targetAgentId);
  if (!cap.writer) return AGENT_NO_WRITER_REASON;
  const speaks = sourceSpeaks(source);
  const overlap = speaks.some((protocol) => cap.accepts.includes(protocol));
  return overlap ? SAME_PROTOCOL_NO_EDGE_REASON : PROTOCOL_MISMATCH_REASON;
}

export function unsupported(
  reason: string,
  evidenceItems: AdapterEvidence[],
  options?: { gateKind?: AdapterRouteAnalysis['gateKind']; ruleId?: string | null },
): AdapterRouteAnalysis {
  return {
    route: 'unsupported',
    support: 'unsupported',
    reason,
    actions: [],
    limitations: [
      '当前不支持此组合；不会改动来源连接、本机服务或配置。',
      'plan.canApply=false：无 Apply、启动 Bridge 或强制继续入口。',
    ],
    evidence: evidenceItems,
    ruleId: options?.ruleId ?? null,
    gateKind: options?.gateKind ?? 'unsupported',
  };
}

/** Optional classify-only fields that mirror core Account.extra / credentials. */
export type ClassifiableAccount = Account & {
  extra?: Record<string, unknown>;
  credentials?: Record<string, unknown>;
};

export type RouteSourceLabel =
  | 'kimi_membership'
  | 'kimi_non_membership'
  | 'anthropic_api_key'
  | 'openai_api_key'
  | 'xai_api_key'
  | 'glm_coding_plan'
  | 'deepseek_api'
  | 'codex_subscription'
  | 'codex_subscription_oauth_other'
  | 'claude_subscription'
  | 'grok_xai_subscription'
  | 'other'
  | 'not_found';

/** Keep lockstep with core `SAME_EDGE_UNWRITABLE_REASON`. */
export const SAME_EDGE_UNWRITABLE_REASON =
  '同边但暂不可写：写入仍只接受 Provider 行，下一步 bind 打通。';

/** Keep lockstep with core `KIMI_NON_MEMBERSHIP_REASON`. */
export const KIMI_NON_MEMBERSHIP_REASON =
  '当前 Kimi 连接不是「Kimi Code 会员」来源。跨 Agent 适配仅支持会员：Connections 中选择 preset「Kimi Code 会员」，或配置端点包含 api.kimi.com/coding。开放平台（moonshot）与任意兼容 API 不会自动升级。当前不支持不等于连接失效。';

/** Keep lockstep with core `KIMI_MEMBERSHIP_PRESET` / `KIMI_CODING_ENDPOINT_NEEDLE`. */
export const KIMI_MEMBERSHIP_PRESET = 'kimi-code-membership';
export const KIMI_CODING_ENDPOINT_NEEDLE = 'api.kimi.com/coding';
export const OPENAI_API_ENDPOINT_NEEDLE = 'api.openai.com';
export const XAI_API_ENDPOINT_NEEDLE = 'api.x.ai';
export const GLM_CODING_ANTHROPIC_NEEDLE = 'open.bigmodel.cn/api/anthropic';
export const GLM_CODING_CHAT_NEEDLE = 'open.bigmodel.cn/api/coding';
export const GLM_CODING_RESPONSES_NEEDLE = 'open.bigmodel.cn/api/v1';
export const DEEPSEEK_API_ENDPOINT_NEEDLE = 'api.deepseek.com';
export const KIMI_CLAUDE_BASE_URL = 'https://api.kimi.com/coding/';
export const GLM_CLAUDE_BASE_URL = 'https://open.bigmodel.cn/api/anthropic';
export const DEEPSEEK_CLAUDE_BASE_URL = 'https://api.deepseek.com/anthropic';
export const GLM_PI_BASE_URL = 'https://open.bigmodel.cn/api/coding/paas/v4';
export const DEEPSEEK_PI_BASE_URL = 'https://api.deepseek.com';
export const GLM_CODEX_BASE_URL = 'https://open.bigmodel.cn/api/v1';
export const DEEPSEEK_CODEX_BASE_URL = 'https://api.deepseek.com';
export const GLM_CODEX_RULE_ID = 'glm-coding-plan-to-codex-v1';
export const DEEPSEEK_CODEX_RULE_ID = 'deepseek-api-to-codex-v1';
export const GLM_CODEX_PROVIDER_SLUG = 'agenthub_glm';
export const DEEPSEEK_CODEX_PROVIDER_SLUG = 'agenthub_deepseek';
export const GLM_CLAUDE_RULE_ID = 'glm-coding-plan-to-claude-v1';
export const DEEPSEEK_CLAUDE_RULE_ID = 'deepseek-api-to-claude-v1';
export const CLAUDE_NATIVE_EXPERIMENTAL_RULES = new Set([
  GLM_CLAUDE_RULE_ID,
  DEEPSEEK_CLAUDE_RULE_ID,
]);
export const EXPLICIT_API_TO_PI_RULES = new Set([
  'anthropic-api-to-pi-v1',
  'openai-api-to-pi-v1',
  'xai-api-to-pi-v1',
  'glm-coding-plan-to-pi-v1',
  'deepseek-api-to-pi-v1',
]);
export const EXPLICIT_API_TO_CODEX_RULES = new Set([
  'anthropic-api-to-codex-v1',
  GLM_CODEX_RULE_ID,
  DEEPSEEK_CODEX_RULE_ID,
]);
export const KIMI_MEMBERSHIP_RULE_IDS = new Set([
  'kimi-membership-to-claude-v1',
  'kimi-membership-to-codex-v1',
  'kimi-membership-to-pi-v1',
  'kimi-membership-to-grok-v1',
]);
export const NATIVE_SUBSCRIPTION_PI_RULE_IDS = new Set([
  'claude-subscription-to-pi-v1',
  'codex-subscription-to-pi-v1',
  'grok-subscription-to-pi-v1',
]);
export const CODEX_CLAUDE_RULE_ID = 'codex-subscription-to-claude-responses-v1';
export const KIMI_GROK_RULE_ID = 'kimi-membership-to-grok-v1';
export const OPENAI_GROK_RULE_ID = 'openai-api-to-grok-v1';
export const GROK_CLAUDE_RULE_ID = 'grok-subscription-to-claude-v1';
export const KIMI_GROK_BASE_URL = 'https://api.kimi.com/coding/v1';
export const OPENAI_GROK_BASE_URL = 'https://api.openai.com/v1';
export const GROK_NATIVE_RULE_IDS = new Set([KIMI_GROK_RULE_ID, OPENAI_GROK_RULE_ID]);

export function jsonString(value: unknown, key: string): string | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const raw = (value as Record<string, unknown>)[key];
  if (typeof raw !== 'string') return undefined;
  const trimmed = raw.trim();
  return trimmed || undefined;
}

/** Mirror `is_codex_auth_json`: format=auth_json OR nested tokens.access_token/refresh_token. */
export function isCodexAuthJson(format: string | undefined, credentials: unknown): boolean {
  if (format?.toLowerCase() === 'auth_json') return true;
  const tokens = credentials && typeof credentials === 'object'
    ? (credentials as Record<string, unknown>).tokens
    : undefined;
  if (!tokens || typeof tokens !== 'object' || Array.isArray(tokens)) return false;
  return Object.prototype.hasOwnProperty.call(tokens, 'access_token')
    || Object.prototype.hasOwnProperty.call(tokens, 'refresh_token');
}

export function textLooksLikeKimiCoding(text: string | undefined): boolean {
  return typeof text === 'string' && text.toLowerCase().includes(KIMI_CODING_ENDPOINT_NEEDLE);
}

/** Same rule as core classify/apply: Kimi + (preset or official coding endpoint). */
export function isKimiMembershipProvider(provider: Pick<Provider, 'agentId' | 'preset' | 'configText'>): boolean {
  return provider.agentId === 'kimi'
    && (provider.preset === KIMI_MEMBERSHIP_PRESET
      || textLooksLikeKimiCoding(provider.configText));
}

export function isKimiMembershipAccount(account: ClassifiableAccount | undefined): boolean {
  if (!account) return false;
  if (account.agentId !== 'kimi' || account.kind !== 'apikey') return false;
  const extra = account.extra ?? {};
  const credentials = account.credentials ?? {};
  const tags = [
    jsonString(extra, 'provider'),
    jsonString(extra, 'preset'),
    jsonString(credentials, 'provider'),
  ];
  return tags.some((tag) => tag?.toLowerCase() === KIMI_MEMBERSHIP_PRESET)
    || textLooksLikeKimiCoding(JSON.stringify(extra))
    || textLooksLikeKimiCoding(JSON.stringify(credentials));
}

export function hasAccountApiKey(account: ClassifiableAccount | undefined): boolean {
  if (!account || account.kind !== 'apikey') return false;
  const credentials = account.credentials;
  return !!credentials
    && jsonString(credentials, 'format')?.toLowerCase() === 'api_key'
    && !!jsonString(credentials, 'api_key');
}
