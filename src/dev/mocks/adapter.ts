import {
  adapterCommandError,
  type AdapterAction,
  type AdapterApplyRequest,
  type AdapterApplyResult,
  type AdapterApplyPlan,
  type AdapterBridgeRuntimeStatus,
  type AdapterEvidence,
  type AdapterPlanChange,
  type AdapterPort,
  type AdapterProfile,
  type AdapterProfileFilter,
  type AdapterRouteAnalysis,
  type AdapterRouteRequest,
} from '@/lib/backend/contracts/adapter';
import type { Account, Provider } from '@/lib/types';
import { delay } from './delay';

const verifiedAt = '2026-08-12';
interface MockAdapterState {
  profiles: AdapterProfile[];
  bridgeStatuses: Map<string, AdapterBridgeRuntimeStatus>;
  generatedProviders: Map<string, Provider>;
  removeGeneratedProvider?: (provider: Provider) => void;
}

const adapterStates = new Set<MockAdapterState>();

export function resetMockAdapters(): void {
  adapterStates.forEach((state) => {
    state.generatedProviders.forEach((provider) => state.removeGeneratedProvider?.(provider));
    state.profiles.length = 0;
    state.bridgeStatuses.clear();
    state.generatedProviders.clear();
  });
}

/** Snapshot of profiles across all mock adapter ports (ticket wallet). */
export function listMockAdapterProfiles(): AdapterProfile[] {
  const out: AdapterProfile[] = [];
  for (const state of adapterStates) {
    for (const profile of state.profiles) {
      out.push({ ...profile });
    }
  }
  return out;
}

/**
 * Unbind helper: stop bridge + drop generated projection even when it is current.
 * Ticket source rows stay in the wallet.
 */
export function removeMockAdapterBinding(profileId: string): void {
  for (const state of adapterStates) {
    const index = state.profiles.findIndex((profile) => profile.id === profileId);
    if (index < 0) continue;
    const profile = state.profiles[index];
    const providerId = profile.generatedProviderId;
    const generated = providerId
      ? state.generatedProviders.get(providerId)
      : undefined;
    if (generated) {
      state.removeGeneratedProvider?.(generated);
      state.generatedProviders.delete(generated.id);
    }
    state.bridgeStatuses.delete(profileId);
    state.profiles.splice(index, 1);
    return;
  }
  throw adapterCommandError({
    code: 'not_found',
    message: `adapter profile not found: ${profileId}`,
    retryable: false,
  });
}

/** Sync bridge status lookup for ticket wallet bindings. */
export function getMockBridgeStatusSync(profileId: string): AdapterBridgeRuntimeStatus | undefined {
  for (const state of adapterStates) {
    const status = state.bridgeStatuses.get(profileId);
    if (status) return { ...status };
  }
  return undefined;
}

/**
 * Test / fixture helper: insert profiles + optional bridge statuses into every
 * live mock adapter port (normally one per createBackend()).
 */
export function seedMockAdapterProfiles(
  profiles: readonly AdapterProfile[],
  bridges?: ReadonlyMap<string, AdapterBridgeRuntimeStatus> | Record<string, AdapterBridgeRuntimeStatus>,
): void {
  const bridgeEntries = bridges instanceof Map
    ? [...bridges.entries()]
    : Object.entries(bridges ?? {});
  for (const state of adapterStates) {
    for (const profile of profiles) {
      const existing = state.profiles.findIndex((item) => item.id === profile.id);
      if (existing >= 0) state.profiles[existing] = { ...profile };
      else state.profiles.push({ ...profile });
    }
    for (const [id, status] of bridgeEntries) {
      state.bridgeStatuses.set(id, { ...status });
    }
  }
}

/** Resolver is injected so the mock classifies the actual saved rows, never fixture ids. */
export interface MockAdapterSourceResolver {
  getAccountById(id: string): Account | undefined;
  getProviderById(id: string): Provider | undefined;
  /** Optional to keep focused route tests independent of mock Connection storage. */
  upsertGeneratedProvider?(provider: Provider): Provider;
  /** Removes only the Adapter-created Connection during reset or a successful remove. */
  removeGeneratedProvider?(provider: Provider): void;
}

function evidence(label: string, url: string): AdapterEvidence {
  return { label, url, verifiedAt };
}

function action(
  kind: AdapterAction['kind'],
  target: string,
  description: string,
  value?: string,
): AdapterAction {
  return { kind, target, description, value, secret: false };
}

function secretAction(target: string, description: string): AdapterAction {
  return { kind: 'reference_connection_secret', target, description, secret: true };
}

function change(target: string, field: string, value?: string): AdapterPlanChange {
  return { target, field, value, secret: false };
}

function secretChange(target: string, field: string): AdapterPlanChange {
  return { target, field, secret: true };
}

/** Keep in lockstep with `CODEX_SUBSCRIPTION_TO_CLAUDE_REASON` in agenthub-core. */
export const CODEX_SUBSCRIPTION_TO_CLAUDE_REASON = [
  'Codex / ChatGPT 订阅可通过本机桥接到 Claude Code（Messages → Responses）。',
].join('');

export const CLAUDE_SUBSCRIPTION_TO_CODEX_REASON =
  'Claude 订阅 → Codex：产品不做。Codex 不吃 Anthropic PKCE，本产品不走这条边。';

const CODEX_SUBSCRIPTION_TO_CLAUDE_CANDIDATE_REASON = [
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

type TicketProtocol =
  | 'anthropic-messages'
  | 'anthropic-pkce'
  | 'openai-chat'
  | 'openai-responses'
  | 'openai-codex-pkce'
  | 'xai-device-code';

/** Keep in lockstep with `agent_bind_capability` in agenthub-core. */
function agentBindCapability(id: string): { accepts: TicketProtocol[]; writer: boolean } {
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

function sourceSpeaks(source: RouteSourceLabel): TicketProtocol[] {
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

function unsupportedReasonFromGraph(source: RouteSourceLabel, targetAgentId: string): string {
  const cap = agentBindCapability(targetAgentId);
  if (!cap.writer) return AGENT_NO_WRITER_REASON;
  const speaks = sourceSpeaks(source);
  const overlap = speaks.some((protocol) => cap.accepts.includes(protocol));
  return overlap ? SAME_PROTOCOL_NO_EDGE_REASON : PROTOCOL_MISMATCH_REASON;
}

function unsupported(
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
type ClassifiableAccount = Account & {
  extra?: Record<string, unknown>;
  credentials?: Record<string, unknown>;
};

type RouteSourceLabel =
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
const SAME_EDGE_UNWRITABLE_REASON =
  '同边但暂不可写：写入仍只接受 Provider 行，下一步 bind 打通。';

/** Keep lockstep with core `KIMI_NON_MEMBERSHIP_REASON`. */
const KIMI_NON_MEMBERSHIP_REASON =
  '当前 Kimi 连接不是「Kimi Code 会员」来源。跨 Agent 适配仅支持会员：Connections 中选择 preset「Kimi Code 会员」，或配置端点包含 api.kimi.com/coding。开放平台（moonshot）与任意兼容 API 不会自动升级。当前不支持不等于连接失效。';

/** Keep lockstep with core `KIMI_MEMBERSHIP_PRESET` / `KIMI_CODING_ENDPOINT_NEEDLE`. */
const KIMI_MEMBERSHIP_PRESET = 'kimi-code-membership';
const KIMI_CODING_ENDPOINT_NEEDLE = 'api.kimi.com/coding';
const OPENAI_API_ENDPOINT_NEEDLE = 'api.openai.com';
const XAI_API_ENDPOINT_NEEDLE = 'api.x.ai';
const GLM_CODING_ANTHROPIC_NEEDLE = 'open.bigmodel.cn/api/anthropic';
const GLM_CODING_CHAT_NEEDLE = 'open.bigmodel.cn/api/coding';
const GLM_CODING_RESPONSES_NEEDLE = 'open.bigmodel.cn/api/v1';
const DEEPSEEK_API_ENDPOINT_NEEDLE = 'api.deepseek.com';
const KIMI_CLAUDE_BASE_URL = 'https://api.kimi.com/coding/';
const GLM_CLAUDE_BASE_URL = 'https://open.bigmodel.cn/api/anthropic';
const DEEPSEEK_CLAUDE_BASE_URL = 'https://api.deepseek.com/anthropic';
const GLM_PI_BASE_URL = 'https://open.bigmodel.cn/api/coding/paas/v4';
const DEEPSEEK_PI_BASE_URL = 'https://api.deepseek.com';
const GLM_CODEX_BASE_URL = 'https://open.bigmodel.cn/api/v1';
const DEEPSEEK_CODEX_BASE_URL = 'https://api.deepseek.com';
const GLM_CODEX_RULE_ID = 'glm-coding-plan-to-codex-v1';
const DEEPSEEK_CODEX_RULE_ID = 'deepseek-api-to-codex-v1';
const GLM_CODEX_PROVIDER_SLUG = 'agenthub_glm';
const DEEPSEEK_CODEX_PROVIDER_SLUG = 'agenthub_deepseek';
const GLM_CLAUDE_RULE_ID = 'glm-coding-plan-to-claude-v1';
const DEEPSEEK_CLAUDE_RULE_ID = 'deepseek-api-to-claude-v1';
const CLAUDE_NATIVE_EXPERIMENTAL_RULES = new Set([
  GLM_CLAUDE_RULE_ID,
  DEEPSEEK_CLAUDE_RULE_ID,
]);
const EXPLICIT_API_TO_PI_RULES = new Set([
  'anthropic-api-to-pi-v1',
  'openai-api-to-pi-v1',
  'xai-api-to-pi-v1',
  'glm-coding-plan-to-pi-v1',
  'deepseek-api-to-pi-v1',
]);
const EXPLICIT_API_TO_CODEX_RULES = new Set([
  'anthropic-api-to-codex-v1',
  GLM_CODEX_RULE_ID,
  DEEPSEEK_CODEX_RULE_ID,
]);
const KIMI_MEMBERSHIP_RULE_IDS = new Set([
  'kimi-membership-to-claude-v1',
  'kimi-membership-to-codex-v1',
  'kimi-membership-to-pi-v1',
  'kimi-membership-to-grok-v1',
]);
const NATIVE_SUBSCRIPTION_PI_RULE_IDS = new Set([
  'claude-subscription-to-pi-v1',
  'codex-subscription-to-pi-v1',
  'grok-subscription-to-pi-v1',
]);
const CODEX_CLAUDE_RULE_ID = 'codex-subscription-to-claude-responses-v1';
const KIMI_GROK_RULE_ID = 'kimi-membership-to-grok-v1';
const OPENAI_GROK_RULE_ID = 'openai-api-to-grok-v1';
const GROK_CLAUDE_RULE_ID = 'grok-subscription-to-claude-v1';
const KIMI_GROK_BASE_URL = 'https://api.kimi.com/coding/v1';
const OPENAI_GROK_BASE_URL = 'https://api.openai.com/v1';
const GROK_NATIVE_RULE_IDS = new Set([KIMI_GROK_RULE_ID, OPENAI_GROK_RULE_ID]);

function jsonString(value: unknown, key: string): string | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const raw = (value as Record<string, unknown>)[key];
  if (typeof raw !== 'string') return undefined;
  const trimmed = raw.trim();
  return trimmed || undefined;
}

/** Mirror `is_codex_auth_json`: format=auth_json OR nested tokens.access_token/refresh_token. */
function isCodexAuthJson(format: string | undefined, credentials: unknown): boolean {
  if (format?.toLowerCase() === 'auth_json') return true;
  const tokens = credentials && typeof credentials === 'object'
    ? (credentials as Record<string, unknown>).tokens
    : undefined;
  if (!tokens || typeof tokens !== 'object' || Array.isArray(tokens)) return false;
  return Object.prototype.hasOwnProperty.call(tokens, 'access_token')
    || Object.prototype.hasOwnProperty.call(tokens, 'refresh_token');
}

function textLooksLikeKimiCoding(text: string | undefined): boolean {
  return typeof text === 'string' && text.toLowerCase().includes(KIMI_CODING_ENDPOINT_NEEDLE);
}

/** Same rule as core classify/apply: Kimi + (preset or official coding endpoint). */
function isKimiMembershipProvider(provider: Pick<Provider, 'agentId' | 'preset' | 'configText'>): boolean {
  return provider.agentId === 'kimi'
    && (provider.preset === KIMI_MEMBERSHIP_PRESET
      || textLooksLikeKimiCoding(provider.configText));
}

function isKimiMembershipAccount(account: ClassifiableAccount | undefined): boolean {
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

function hasAccountApiKey(account: ClassifiableAccount | undefined): boolean {
  if (!account || account.kind !== 'apikey') return false;
  const credentials = account.credentials;
  return !!credentials
    && jsonString(credentials, 'format')?.toLowerCase() === 'api_key'
    && !!jsonString(credentials, 'api_key');
}

function classify(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): RouteSourceLabel {
  if (request.sourceKind === 'provider') {
    const provider = resolver.getProviderById(request.sourceId);
    if (!provider) return 'not_found';
    if (isKimiMembershipProvider(provider)) {
      return 'kimi_membership';
    }
    if (
      provider.agentId === 'claude'
      && (provider.preset === 'anthropic'
        || (typeof provider.configText === 'string'
          && provider.configText.toLowerCase().includes('api.anthropic.com')))
    ) {
      return 'anthropic_api_key';
    }
    const tag = provider.preset
      ?? jsonString((provider as { meta?: Record<string, unknown> }).meta, 'provider');
    const config = provider.configText ?? '';
    if (
      tag?.toLowerCase() === 'openai'
      || tag?.toLowerCase() === 'openai-api'
      || config.toLowerCase().includes(OPENAI_API_ENDPOINT_NEEDLE)
    ) {
      return 'openai_api_key';
    }
    if (
      tag?.toLowerCase() === 'xai'
      || tag?.toLowerCase() === 'xai-api'
      || config.toLowerCase().includes(XAI_API_ENDPOINT_NEEDLE)
    ) {
      return 'xai_api_key';
    }
    if (
      tag?.toLowerCase() === 'glm-coding-plan'
      || config.toLowerCase().includes(GLM_CODING_ANTHROPIC_NEEDLE)
      || config.toLowerCase().includes(GLM_CODING_CHAT_NEEDLE)
      || config.toLowerCase().includes(GLM_CODING_RESPONSES_NEEDLE)
    ) {
      return 'glm_coding_plan';
    }
    if (
      tag?.toLowerCase() === 'deepseek-api'
      || tag?.toLowerCase() === 'deepseek'
      || config.toLowerCase().includes(DEEPSEEK_API_ENDPOINT_NEEDLE)
    ) {
      return 'deepseek_api';
    }
    if (provider.agentId === 'kimi') return 'kimi_non_membership';
    return 'other';
  }

  const account = resolver.getAccountById(request.sourceId) as ClassifiableAccount | undefined;
  if (!account) return 'not_found';
  const explicitProvider =
    jsonString(account.extra, 'provider')
    ?? jsonString(account.credentials, 'provider')
    ?? account.provider?.trim();
  const credentialFormat =
    jsonString(account.credentials, 'format')
    ?? jsonString(account.extra, 'format')
    ?? account.credentialFormat?.trim();

  if (isKimiMembershipAccount(account)) {
    return 'kimi_membership';
  }
  if (account.kind === 'apikey' && account.agentId === 'kimi') {
    return 'kimi_non_membership';
  }
  if (account.agentId === 'claude' && account.kind === 'oauth') {
    return 'claude_subscription';
  }
  if (account.agentId === 'grok' && account.kind === 'oauth') {
    return 'grok_xai_subscription';
  }
  if (account.kind === 'apikey' && explicitProvider?.toLowerCase() === 'anthropic') {
    return 'anthropic_api_key';
  }
  const credentialsText = JSON.stringify(account.credentials ?? {});
  const extraText = JSON.stringify(account.extra ?? {});
  if (
    account.kind === 'apikey'
    && (explicitProvider?.toLowerCase() === 'openai'
      || explicitProvider?.toLowerCase() === 'openai-api'
      || credentialsText.toLowerCase().includes(OPENAI_API_ENDPOINT_NEEDLE)
      || extraText.toLowerCase().includes(OPENAI_API_ENDPOINT_NEEDLE))
  ) {
    return 'openai_api_key';
  }
  if (
    account.kind === 'apikey'
    && (explicitProvider?.toLowerCase() === 'xai'
      || explicitProvider?.toLowerCase() === 'xai-api'
      || credentialsText.toLowerCase().includes(XAI_API_ENDPOINT_NEEDLE)
      || extraText.toLowerCase().includes(XAI_API_ENDPOINT_NEEDLE))
  ) {
    return 'xai_api_key';
  }
  if (
    account.kind === 'apikey'
    && (explicitProvider?.toLowerCase() === 'glm-coding-plan'
      || credentialsText.toLowerCase().includes(GLM_CODING_ANTHROPIC_NEEDLE)
      || credentialsText.toLowerCase().includes(GLM_CODING_CHAT_NEEDLE)
      || credentialsText.toLowerCase().includes(GLM_CODING_RESPONSES_NEEDLE)
      || extraText.toLowerCase().includes(GLM_CODING_ANTHROPIC_NEEDLE)
      || extraText.toLowerCase().includes(GLM_CODING_CHAT_NEEDLE)
      || extraText.toLowerCase().includes(GLM_CODING_RESPONSES_NEEDLE))
  ) {
    return 'glm_coding_plan';
  }
  if (
    account.kind === 'apikey'
    && (explicitProvider?.toLowerCase() === 'deepseek-api'
      || explicitProvider?.toLowerCase() === 'deepseek'
      || credentialsText.toLowerCase().includes(DEEPSEEK_API_ENDPOINT_NEEDLE)
      || extraText.toLowerCase().includes(DEEPSEEK_API_ENDPOINT_NEEDLE))
  ) {
    return 'deepseek_api';
  }
  if (account.agentId === 'codex' && account.kind === 'oauth') {
    return isCodexAuthJson(credentialFormat, account.credentials ?? {})
      ? 'codex_subscription'
      : 'codex_subscription_oauth_other';
  }
  if (account.agentId === 'claude' && account.kind === 'oauth') {
    return 'claude_subscription';
  }
  if (account.agentId === 'grok' && account.kind === 'oauth') {
    return 'grok_xai_subscription';
  }
  return 'other';
}

function analyze(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): AdapterRouteAnalysis {
  const source = classify(resolver, request);
  if (source === 'not_found') {
    throw adapterCommandError({
      code: 'not_found',
      message: `${request.sourceKind} not found: ${request.sourceId}`,
      retryable: false,
    });
  }
  const compatibilityEvidence = [evidence(
    'AgentHub：厂商、API 与 OAuth 适配规则',
    'https://github.com/nicechencs/AgentHub/blob/release/docs/provider-api-oauth-adaptation.md',
  )];
  // Bind-entry table first: no writer → infeasible. Cursor must take this path.
  if (!agentBindCapability(request.targetAgentId).writer) {
    return unsupported(AGENT_NO_WRITER_REASON, compatibilityEvidence);
  }
  if (source === 'kimi_membership' && request.targetAgentId === 'claude') {
    return {
      route: 'native_endpoint',
      support: 'stable',
      reason: 'Kimi Code 会员可预览为 Claude 的原生 Anthropic Messages 端点。',
      actions: [
        action('set_config', 'Claude Code', '设置 Kimi Code 官方 Anthropic-compatible Base URL。', 'https://api.kimi.com/coding/'),
        action('set_env', 'Claude Code', '使用 Claude Code 的认证环境变量名。', 'ANTHROPIC_AUTH_TOKEN'),
        secretAction('Claude Code', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 Claude 的 base URL 与凭据引用标记；不会在预览中传输明文 Key。',
        '应用后会切换当前 Claude Connection；请确认无其他进行中的配置写入。',
      ],
      evidence: [evidence('Kimi Code: Claude Code integration', 'https://www.kimi.com/code/docs/en/third-party-tools/claude-code.html')],
      ruleId: 'kimi-membership-to-claude-v1',
      gateKind: 'none',
    };
  }
  if (source === 'kimi_membership' && request.targetAgentId === 'grok') {
    return {
      route: 'native_endpoint',
      support: 'experimental',
      reason: 'Kimi Code 会员可实验写入 Grok 的 OpenAI Chat Completions 配置。',
      actions: [
        action('set_config', 'Grok', '写入 Grok 官方 OpenAI Chat Completions TOML。', KIMI_GROK_BASE_URL),
        action('set_config', 'Grok', '使用 Grok Chat Completions 与 kimi-k2.5。', 'api_backend=chat_completions; model=kimi-k2.5'),
        secretAction('Grok', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '只修改 Grok ~/.grok/config.toml 的官方 TOML provider；不会启动本机桥接。',
        '生成 Provider 只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。',
      ],
      evidence: compatibilityEvidence,
      ruleId: KIMI_GROK_RULE_ID,
      gateKind: 'none',
    };
  }
  if (source === 'openai_api_key' && request.targetAgentId === 'grok') {
    return {
      route: 'native_endpoint',
      support: 'experimental',
      reason: 'OpenAI API 可实验写入 Grok 的官方 OpenAI Chat Completions 配置。',
      actions: [
        action('set_config', 'Grok', '写入 Grok 官方 OpenAI Chat Completions TOML。', OPENAI_GROK_BASE_URL),
        action('set_config', 'Grok', '使用 Grok Chat Completions 与 gpt-4o。', 'api_backend=chat_completions; model=gpt-4o'),
        secretAction('Grok', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '只修改 Grok ~/.grok/config.toml 的官方 TOML provider；不会启动本机桥接。',
        '生成 Provider 只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。',
      ],
      evidence: compatibilityEvidence,
      ruleId: OPENAI_GROK_RULE_ID,
      gateKind: 'none',
    };
  }
  if (source === 'glm_coding_plan' && request.targetAgentId === 'claude') {
    return {
      route: 'native_endpoint',
      support: 'experimental',
      reason: 'GLM Coding Plan 可实验预览为 Claude 的原生 Anthropic Messages 端点。',
      actions: [
        action('set_config', 'Claude Code', '设置 GLM Coding Plan 官方 Anthropic-compatible Base URL。', GLM_CLAUDE_BASE_URL),
        action('set_env', 'Claude Code', '使用 Claude Code 的认证环境变量名。', 'ANTHROPIC_AUTH_TOKEN'),
        secretAction('Claude Code', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 Claude 的 GLM Coding Plan Anthropic 兼容 Base URL 与凭据引用标记；不会在预览中传输明文 Key。',
        '应用后会切换当前 Claude Connection；请确认无其他进行中的配置写入。',
        '实验性：官方 Anthropic 兼容入口；部分扩展字段可能被忽略或不支持。',
      ],
      evidence: [evidence('GLM Coding Plan 接入工具与双协议端点', 'https://docs.bigmodel.cn/cn/coding-plan/tool/others')],
      ruleId: GLM_CLAUDE_RULE_ID,
      gateKind: 'none',
    };
  }
  if (source === 'deepseek_api' && request.targetAgentId === 'claude') {
    return {
      route: 'native_endpoint',
      support: 'experimental',
      reason: 'DeepSeek API 可实验预览为 Claude 的原生 Anthropic Messages 端点。',
      actions: [
        action('set_config', 'Claude Code', '设置 DeepSeek 官方 Anthropic-compatible Base URL。', DEEPSEEK_CLAUDE_BASE_URL),
        action('set_env', 'Claude Code', '使用 Claude Code 的认证环境变量名。', 'ANTHROPIC_AUTH_TOKEN'),
        secretAction('Claude Code', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 Claude 的 DeepSeek Anthropic 兼容 Base URL 与凭据引用标记；不会在预览中传输明文 Key。',
        '应用后会切换当前 Claude Connection；请确认无其他进行中的配置写入。',
        '实验性：官方 Anthropic 兼容入口；部分扩展字段可能被忽略或不支持。',
      ],
      evidence: [evidence('DeepSeek 接入 Claude Code', 'https://api-docs.deepseek.com/quick_start/agent_integrations/claude_code/')],
      ruleId: DEEPSEEK_CLAUDE_RULE_ID,
      gateKind: 'none',
    };
  }
  if (
    (source === 'kimi_membership' || source === 'openai_api_key')
    && request.targetAgentId === 'grok'
  ) {
    const kimi = source === 'kimi_membership';
    return {
      route: 'native_endpoint',
      support: 'experimental',
      reason: kimi
        ? 'Kimi Code 会员可实验写入 Grok 的 OpenAI Chat Completions 配置。'
        : 'OpenAI API 可实验写入 Grok 的官方 OpenAI Chat Completions 配置。',
      actions: [
        action(
          'set_config',
          'Grok',
          `写入 Grok 的${kimi ? ' Kimi Code' : ' OpenAI'} 官方 OpenAI Chat Completions 配置。`,
          kimi ? KIMI_GROK_BASE_URL : OPENAI_GROK_BASE_URL,
        ),
        action(
          'set_config',
          'Grok',
          '设置 Grok 模型与 Chat Completions backend。',
          `model=${kimi ? 'kimi-k2.5' : 'gpt-4o'}; api_backend=chat_completions`,
        ),
        secretAction('Grok', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '写入 Grok config.toml 的官方 OpenAI Chat Completions model 槽；不会启动本机桥接。',
        '生成 Provider 只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。',
      ],
      evidence: compatibilityEvidence,
      ruleId: kimi ? KIMI_GROK_RULE_ID : OPENAI_GROK_RULE_ID,
      gateKind: 'none',
    };
  }
  if (source === 'kimi_membership' && request.targetAgentId === 'codex') {
    return {
      route: 'local_bridge',
      support: 'experimental',
      reason: 'Kimi Code 会员到 Codex 需要本地协议桥接。',
      actions: [action('requires_local_bridge', 'Codex', 'Codex Responses 与 Kimi Chat Completions 需要本地双向协议转换。')],
      limitations: [
        '将在本机 loopback 启动协议桥接，并切换 Codex 到该本地端点。',
        'AgentHub 需保持在托盘运行；退出前会尝试排空监听。',
        '桥接为实验性协议覆盖；长流与工具调用可能受实现限制。',
        '固定端口被占用时会尝试重新分配端口并写回配置。',
      ],
      evidence: [evidence('Kimi Code: Codex local routing', 'https://www.kimi.com/code/docs/third-party-tools/codex.html')],
      ruleId: 'kimi-membership-to-codex-v1',
      gateKind: 'none',
    };
  }
  if (source === 'kimi_membership' && request.targetAgentId === 'pi') {
    return {
      route: 'config_sync',
      support: 'stable',
      reason: 'Kimi Code 会员可预览为 Pi 的配置同步。',
      actions: [
        action('set_config', 'Pi', '选择 Pi 的 Kimi For Coding provider。', 'kimi-for-coding'),
        secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 Pi models.json 对应 provider 槽；凭据只引用已保存的 Connection，不会读取或显示明文 Key。',
        '应用后会切换当前 Pi Connection。',
      ],
      evidence: [evidence('Kimi Code CLI provider configuration', 'https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/providers.html')],
      ruleId: 'kimi-membership-to-pi-v1',
      gateKind: 'none',
    };
  }
  if (source === 'anthropic_api_key' && request.targetAgentId === 'codex') {
    return {
      route: 'local_bridge',
      support: 'experimental',
      reason: '显式 Anthropic API Key 到 Codex 需要本地协议桥接。',
      actions: [action('requires_local_bridge', 'Codex', 'Codex Responses 与 Anthropic Messages 需要本地双向协议转换。')],
      limitations: [
        '将在本机 loopback 启动协议桥接，并切换 Codex 到该本地端点。',
        'AgentHub 需保持在托盘运行；退出前会尝试排空监听。',
        '桥接为实验性协议覆盖：下游 Responses，上游 Anthropic Messages。',
        '固定端口被占用时会尝试重新分配端口并写回配置。',
      ],
      evidence: [evidence('Anthropic Messages API', 'https://docs.anthropic.com/en/api/messages')],
      ruleId: 'anthropic-api-to-codex-v1',
      gateKind: 'none',
    };
  }
  if (
    (source === 'glm_coding_plan' || source === 'deepseek_api')
    && request.targetAgentId === 'codex'
  ) {
    const glm = source === 'glm_coding_plan';
    const baseUrl = glm ? GLM_CODEX_BASE_URL : DEEPSEEK_CODEX_BASE_URL;
    const ruleId = glm ? GLM_CODEX_RULE_ID : DEEPSEEK_CODEX_RULE_ID;
    const model = glm ? 'glm-5.3' : 'deepseek-v4-flash';
    return {
      route: 'native_endpoint',
      support: 'experimental',
      reason: `${glm ? 'GLM Coding Plan' : 'DeepSeek API'} 官方 Responses 端点可实验直连 Codex。`,
      actions: [
        action('set_config', 'Codex', `${glm ? 'GLM Coding Plan' : 'DeepSeek API'} 官方 Responses Base URL；不会启动本机桥接。`, baseUrl),
        action('set_config', 'Codex', `使用 Codex Responses wire_api 与默认模型 ${model}。`, `wire_api=responses; model=${model}`),
        secretAction('Codex', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将把 Codex 配置为官方 Responses 端点；不会启动本机 loopback Bridge。',
        '生成 Provider 只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。',
        '当前未写入官方 ~/.codex/models.json；使用默认 model 与显式 Provider 配置。',
      ],
      evidence: [evidence(
        glm ? 'GLM Coding Plan Codex Responses integration' : 'DeepSeek API Codex Responses integration',
        glm ? 'https://docs.bigmodel.cn/cn/coding-plan/tool/codex' : 'https://api-docs.deepseek.com/quick_start/agent_integrations/codex/',
      )],
      ruleId,
      gateKind: 'none',
    };
  }
  if (source === 'deepseek_api' && request.targetAgentId === 'dsh') {
    return {
      route: 'config_sync',
      support: 'stable',
      reason: 'DeepSeek API Key 可预览为 DeepSeek Harness 的配置同步。',
      actions: [
        action('set_config', 'DeepSeek Harness', '选择 DSH 的官方 DeepSeek provider。', 'deepseek-official'),
        secretAction('DeepSeek Harness', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 DeepSeek Harness 的 home 级 provider 引用与凭据文件；不会把 API Key 写入 cordis.patch.yml。',
        '应用后会把该生成 Provider 设为 DSH 当前连接；请确认无其他进行中的配置写入。',
      ],
      evidence: [evidence('DeepSeek Harness LLM / credentials', 'https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/credentials')],
      ruleId: 'deepseek-api-to-dsh-v1',
      gateKind: 'none',
    };
  }
  if (source === 'anthropic_api_key' && request.targetAgentId === 'pi') {
    return {
      route: 'config_sync',
      support: 'stable',
      reason: '显式 Anthropic API Key 可预览为 Pi 的配置同步。',
      actions: [
        action('set_config', 'Pi', '选择 Pi 的 Anthropic provider。', 'anthropic'),
        secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 Pi models.json 对应 provider 槽；凭据只引用已保存的 Connection，不会读取或显示明文 Key。',
        '应用后会切换当前 Pi Connection。',
      ],
      evidence: [evidence('Pi custom provider and model configuration', 'https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md')],
      ruleId: 'anthropic-api-to-pi-v1',
      gateKind: 'none',
    };
  }
  if (source === 'openai_api_key' && request.targetAgentId === 'pi') {
    return {
      route: 'config_sync',
      support: 'stable',
      reason: '显式 OpenAI API Key 可预览为 Pi 的配置同步。',
      actions: [
        action('set_config', 'Pi', '选择 Pi 的 OpenAI provider。', 'openai'),
        secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 Pi models.json 的 openai 槽与凭据引用标记；不会在预览中传输明文 Key。',
        '应用后会把该生成 Provider 设为 Pi 当前连接；请确认无其他进行中的配置写入。',
      ],
      evidence: [evidence('Pi custom provider and model configuration', 'https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md')],
      ruleId: 'openai-api-to-pi-v1',
      gateKind: 'none',
    };
  }
  if (source === 'xai_api_key' && request.targetAgentId === 'pi') {
    return {
      route: 'config_sync',
      support: 'stable',
      reason: '显式 xAI API Key 可预览为 Pi 的配置同步。',
      actions: [
        action('set_config', 'Pi', '选择 Pi 的 xAI provider。', 'xai'),
        secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 Pi models.json 的 xai 槽与凭据引用标记；不会在预览中传输明文 Key。',
        '应用后会把该生成 Provider 设为 Pi 当前连接；请确认无其他进行中的配置写入。',
      ],
      evidence: [evidence('Pi custom provider and model configuration', 'https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md')],
      ruleId: 'xai-api-to-pi-v1',
      gateKind: 'none',
    };
  }
  if (
    (source === 'glm_coding_plan' || source === 'deepseek_api')
    && request.targetAgentId === 'pi'
  ) {
    const glm = source === 'glm_coding_plan';
    const slot = glm ? 'glm-coding-plan' : 'deepseek';
    const ruleId = glm ? 'glm-coding-plan-to-pi-v1' : 'deepseek-api-to-pi-v1';
    return {
      route: 'config_sync',
      support: 'experimental',
      reason: `${glm ? 'GLM Coding Plan' : 'DeepSeek API'} 可实验预览为 Pi 的配置同步。`,
      actions: [
        action(
          'set_config',
          'Pi',
          `写入 Pi 的 ${glm ? 'GLM Coding Plan' : 'DeepSeek'} 自定义 provider 槽。`,
          slot,
        ),
        secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        `将写入 Pi models.json 的 ${slot} 自定义槽（baseUrl、api、models）与凭据引用标记；不会在预览中传输明文 Key。`,
        '生成 Provider 只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。',
      ],
      evidence: [evidence('Pi custom provider and model configuration', 'https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md')],
      ruleId,
      gateKind: 'none',
    };
  }
  if (
    (source === 'claude_subscription'
      || source === 'codex_subscription'
      || source === 'codex_subscription_oauth_other'
      || source === 'grok_xai_subscription')
    && request.targetAgentId === 'pi'
  ) {
    const subscription = source === 'claude_subscription'
      ? {
          value: 'anthropic',
          ruleId: 'claude-subscription-to-pi-v1',
          reason: 'Claude 订阅可写入 Pi 的 anthropic 登录槽（原生订阅复用）。',
        }
      : source === 'grok_xai_subscription'
        ? {
            value: 'xai',
            ruleId: 'grok-subscription-to-pi-v1',
            reason: 'Grok / xAI 订阅可写入 Pi 的 xai 登录槽（原生订阅复用）。',
          }
        : {
            value: 'openai-codex',
            ruleId: 'codex-subscription-to-pi-v1',
            reason: 'Codex / ChatGPT 订阅可写入 Pi 的 openai-codex 登录槽（原生订阅复用）。',
          };
    return {
      route: 'config_sync',
      support: 'experimental',
      reason: subscription.reason,
      actions: [
        action('set_config', 'Pi', '选择 Pi 的订阅登录槽。', subscription.value),
        secretAction('Pi', '从已选 Connection 引用授权（OAuth）；不会读取或显示 token。'),
      ],
      limitations: [
        '会把 OAuth access/refresh 写入 Pi auth.json 对应槽；预览、IPC、日志不传输明文 token。',
        '写入后由 Pi 刷新该槽；Hub 不双刷同一 refresh token。原 Agent 与 Pi 同时刷新可能互相打翻。',
        '实验性：应用后会把生成 Provider 设为 Pi 当前连接。',
      ],
      evidence: compatibilityEvidence,
      ruleId: subscription.ruleId,
      gateKind: 'none',
    };
  }

  if (source === 'claude_subscription' && request.targetAgentId === 'codex') {
    return unsupported(CLAUDE_SUBSCRIPTION_TO_CODEX_REASON, compatibilityEvidence);
  }
  if (source === 'grok_xai_subscription' && request.targetAgentId === 'claude') {
    return {
      route: 'local_bridge',
      support: 'experimental',
      reason: 'Grok 订阅可通过本机桥接到 Claude Code（Messages → xAI Chat Completions）。',
      actions: [
        action(
          'requires_local_bridge',
          'Claude Code',
          'Claude Messages 与 xAI Chat Completions 需要本地双向协议转换。',
        ),
        action(
          'set_env',
          'Claude Code',
          '写入 Claude Code 的 loopback Base URL 与本机 bearer；不会写入上游 OAuth token。',
          'ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN',
        ),
      ],
      limitations: [
        'Claude 只写入本机 loopback URL 与本地 bearer；xAI OAuth token 不进入 Claude 配置、IPC 或日志。',
        '实验性协议桥接：Claude Messages → xAI Chat Completions；AgentHub 需保持在托盘运行。',
        'Grok access token 过期后需重新同步 Grok 登录；Hub 本轮不自动 refresh。',
      ],
      evidence: compatibilityEvidence,
      ruleId: GROK_CLAUDE_RULE_ID,
      gateKind: 'none',
    };
  }
  // Only Codex auth_json opens the experimental Responses → Claude bridge.
  // Bare Codex OAuth remains a closed subscription candidate.
  if (
    (source === 'codex_subscription' || source === 'codex_subscription_oauth_other')
    && request.targetAgentId === 'claude'
  ) {
    if (source === 'codex_subscription') {
      return {
        route: 'local_bridge',
        support: 'experimental',
        reason: CODEX_SUBSCRIPTION_TO_CLAUDE_REASON,
        actions: [
          action(
            'requires_local_bridge',
            'Claude Code',
            'Claude Messages 与 Codex Responses 需要本地双向协议转换。',
          ),
          action(
            'set_env',
            'Claude Code',
            '写入 Claude Code 的 loopback Base URL 与本机 bearer；不会写入上游 OAuth token。',
            'ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN',
          ),
        ],
        limitations: [
          '会把 Claude 的 ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN 指向本机 loopback；上游 token 不进 Claude。',
          '实验性协议桥接：Claude Messages → Codex Responses；AgentHub 需保持在托盘运行。',
          'Codex access token 过期后需重新同步 Codex 登录；Hub 本轮不自动 refresh。',
          '固定端口被占用时会尝试重新分配端口并写回配置。',
        ],
        evidence: compatibilityEvidence,
        ruleId: CODEX_CLAUDE_RULE_ID,
        gateKind: 'none',
      };
    }
    return unsupported(CODEX_SUBSCRIPTION_TO_CLAUDE_CANDIDATE_REASON, compatibilityEvidence, {
      gateKind: 'subscription_candidate',
      ruleId: null,
    });
  }
  if (source === 'kimi_non_membership') {
    return unsupported(KIMI_NON_MEMBERSHIP_REASON, compatibilityEvidence);
  }
  if (source === 'other') {
    return unsupported(
      'AgentHub 暂未提供此来源到所选目标的适配规则。当前不支持不等于连接失效。',
      compatibilityEvidence,
    );
  }
  return unsupported(unsupportedReasonFromGraph(source, request.targetAgentId), compatibilityEvidence);
}

function buildPlan(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
  analysis: AdapterRouteAnalysis,
): AdapterApplyPlan {
  const configuredProvider = analysis.actions.find(
    (item) => item.kind === 'set_config' && item.target === 'Pi',
  )?.value;
  const claudeBaseUrl = analysis.ruleId === GLM_CLAUDE_RULE_ID
    ? GLM_CLAUDE_BASE_URL
    : analysis.ruleId === DEEPSEEK_CLAUDE_RULE_ID
      ? DEEPSEEK_CLAUDE_BASE_URL
      : KIMI_CLAUDE_BASE_URL;
  const changes = analysis.route === 'native_endpoint' && request.targetAgentId === 'grok'
    ? [
        change(
          'grok',
          'baseUrl',
          analysis.ruleId === KIMI_GROK_RULE_ID ? KIMI_GROK_BASE_URL : OPENAI_GROK_BASE_URL,
        ),
        change('grok', 'model', analysis.ruleId === KIMI_GROK_RULE_ID ? 'kimi-k2.5' : 'gpt-4o'),
        change('grok', 'apiBackend', 'chat_completions'),
        secretChange('grok', 'apiKey'),
      ]
    : analysis.route === 'native_endpoint' && request.targetAgentId === 'claude'
    ? [
        change('claude', 'baseUrl', claudeBaseUrl),
        change('claude', 'claudeAuthEnv', 'ANTHROPIC_AUTH_TOKEN'),
        secretChange('claude', 'apiKey'),
      ]
      : analysis.route === 'local_bridge' && request.targetAgentId === 'codex'
        ? [
            change(
              'codex',
              'provider',
              analysis.ruleId === 'anthropic-api-to-codex-v1'
                ? 'AgentHub Anthropic 本地桥接'
                : 'AgentHub Kimi 本地桥接',
            ),
            change('codex', 'baseUrl', 'http://127.0.0.1:<本机端口>/v1'),
          ]
        : analysis.route === 'local_bridge' && request.targetAgentId === 'claude'
          ? [
              change('claude', 'ANTHROPIC_BASE_URL', 'http://127.0.0.1:<本机端口>'),
              secretChange('claude', 'ANTHROPIC_AUTH_TOKEN'),
            ]
        : analysis.route === 'native_endpoint' && request.targetAgentId === 'codex'
          ? [
              change('codex', 'provider', analysis.ruleId === GLM_CODEX_RULE_ID ? 'GLM Coding Plan' : 'DeepSeek API'),
              change('codex', 'baseUrl', analysis.ruleId === GLM_CODEX_RULE_ID ? GLM_CODEX_BASE_URL : DEEPSEEK_CODEX_BASE_URL),
              change('codex', 'wireApi', 'responses'),
            ]
        : analysis.route === 'config_sync' && request.targetAgentId === 'pi'
      ? [
          change('pi', 'provider', configuredProvider ?? 'anthropic'),
          secretChange(
            'pi',
            analysis.ruleId && NATIVE_SUBSCRIPTION_PI_RULE_IDS.has(analysis.ruleId)
              ? 'auth'
              : 'apiKey',
          ),
        ]
        : analysis.route === 'config_sync' && request.targetAgentId === 'dsh'
      ? [
          change('dsh', 'provider', 'deepseek-official'),
          change('dsh', 'apiKeyEnv', 'DEEPSEEK_API_KEY'),
          secretChange('dsh', 'apiKey'),
        ]
      : [];
  const implementedPath =
    (analysis.route === 'native_endpoint' && analysis.support === 'stable' && request.targetAgentId === 'claude')
    || (analysis.route === 'native_endpoint' && analysis.support === 'experimental'
      && (request.targetAgentId === 'claude' || request.targetAgentId === 'codex')
      && !!analysis.ruleId && CLAUDE_NATIVE_EXPERIMENTAL_RULES.has(analysis.ruleId))
    || (analysis.route === 'native_endpoint' && analysis.support === 'experimental'
      && request.targetAgentId === 'codex'
      && !!analysis.ruleId && EXPLICIT_API_TO_CODEX_RULES.has(analysis.ruleId))
    || (analysis.route === 'native_endpoint' && analysis.support === 'experimental'
      && request.targetAgentId === 'grok'
      && !!analysis.ruleId && GROK_NATIVE_RULE_IDS.has(analysis.ruleId))
    || (analysis.route === 'local_bridge' && analysis.support === 'experimental' && request.targetAgentId === 'codex')
    || (analysis.route === 'local_bridge' && analysis.support === 'experimental'
      && request.sourceKind === 'account'
      && request.targetAgentId === 'claude'
      && analysis.ruleId === CODEX_CLAUDE_RULE_ID
      && hasCodexAccessToken(resolver, request.sourceId))
    || (analysis.route === 'local_bridge' && analysis.support === 'experimental'
      && request.sourceKind === 'account'
      && request.targetAgentId === 'claude'
      && analysis.ruleId === GROK_CLAUDE_RULE_ID
      && hasGrokAccessToken(resolver, request.sourceId))
    || (analysis.route === 'config_sync' && analysis.support === 'stable' && request.targetAgentId === 'pi')
    || (analysis.route === 'config_sync' && analysis.support === 'experimental'
      && request.targetAgentId === 'pi'
      && !!analysis.ruleId
      && EXPLICIT_API_TO_PI_RULES.has(analysis.ruleId))
    || (analysis.route === 'config_sync' && analysis.support === 'experimental'
      && request.targetAgentId === 'pi'
      && !!analysis.ruleId
      && NATIVE_SUBSCRIPTION_PI_RULE_IDS.has(analysis.ruleId))
    || (analysis.route === 'config_sync' && analysis.support === 'stable' && request.targetAgentId === 'dsh');
  const accountSource = request.sourceKind === 'account'
    ? resolver.getAccountById(request.sourceId) as ClassifiableAccount | undefined
    : undefined;
  const accountExplicitApiToPi = request.sourceKind === 'account'
    && implementedPath
    && request.targetAgentId === 'pi'
    && !!analysis.ruleId
    && EXPLICIT_API_TO_PI_RULES.has(analysis.ruleId);
  const accountNativeSubscriptionPi = request.sourceKind === 'account'
    && implementedPath
    && request.targetAgentId === 'pi'
    && !!analysis.ruleId
    && NATIVE_SUBSCRIPTION_PI_RULE_IDS.has(analysis.ruleId);
  const accountExplicitApiToCodex = request.sourceKind === 'account'
    && implementedPath
    && request.targetAgentId === 'codex'
    && !!analysis.ruleId
    && EXPLICIT_API_TO_CODEX_RULES.has(analysis.ruleId);
  const accountClaudeNative = request.sourceKind === 'account'
    && implementedPath
    && request.targetAgentId === 'claude'
    && !!analysis.ruleId
    && CLAUDE_NATIVE_EXPERIMENTAL_RULES.has(analysis.ruleId);
  const accountKimiMembership = request.sourceKind === 'account'
    && implementedPath
    && !!analysis.ruleId
    && KIMI_MEMBERSHIP_RULE_IDS.has(analysis.ruleId)
    && isKimiMembershipAccount(accountSource)
    && hasAccountApiKey(accountSource);
  const accountCodexClaudeBridge = request.sourceKind === 'account'
    && implementedPath
    && request.targetAgentId === 'claude'
    && analysis.ruleId === CODEX_CLAUDE_RULE_ID
    && hasCodexAccessToken(resolver, request.sourceId);
  const accountGrokNative = request.sourceKind === 'account'
    && implementedPath
    && request.targetAgentId === 'grok'
    && !!analysis.ruleId
    && GROK_NATIVE_RULE_IDS.has(analysis.ruleId)
    && hasAccountApiKey(accountSource);
  const accountGrokClaudeBridge = request.sourceKind === 'account'
    && implementedPath
    && request.targetAgentId === 'claude'
    && analysis.ruleId === GROK_CLAUDE_RULE_ID
    && hasGrokAccessToken(resolver, request.sourceId);
  const writeGate = (request.sourceKind === 'provider' && implementedPath)
    || accountKimiMembership
    || accountGrokNative
    || accountExplicitApiToPi
    || accountExplicitApiToCodex
    || accountClaudeNative
    || accountNativeSubscriptionPi
    || accountCodexClaudeBridge
    || accountGrokClaudeBridge;
  const canApply = writeGate;
  const maturity = mockPlanMaturity(analysis);
  const reusePath = NATIVE_SUBSCRIPTION_PI_RULE_IDS.has(analysis.ruleId ?? '')
    ? 'native_subscription' as const
    : analysis.route === 'unsupported'
      ? 'none' as const
      : analysis.route === 'local_bridge'
        ? 'local_bridge' as const
        : 'api_endpoint' as const;
  // Same-edge Account stays closed except explicit API → Pi / Codex.
  const reason = implementedPath && request.sourceKind !== 'provider'
    && !accountExplicitApiToPi
    && !accountExplicitApiToCodex
    && !accountClaudeNative
    && !accountKimiMembership
    && !accountGrokNative
    && !accountNativeSubscriptionPi
    && !accountCodexClaudeBridge
    && !accountGrokClaudeBridge
    ? `${analysis.reason} ${SAME_EDGE_UNWRITABLE_REASON}`
    : analysis.reason;
  return {
    analysis,
    targetAgentId: request.targetAgentId,
    canApply,
    maturity,
    reusePath,
    reason,
    serviceImpact: analysis.route === 'local_bridge' ? 'requires_local_bridge' : 'none',
    changes,
  };
}

function hasCodexAccessToken(
  resolver: MockAdapterSourceResolver,
  sourceId: string,
): boolean {
  const account = resolver.getAccountById(sourceId) as ClassifiableAccount | undefined;
  if (!account || account.agentId !== 'codex' || account.kind !== 'oauth') return false;
  const credentials = account.credentials;
  if (!credentials || typeof credentials !== 'object') return false;
  const record = credentials as Record<string, unknown>;
  const tokens = record.tokens;
  if (tokens && typeof tokens === 'object' && !Array.isArray(tokens)) {
    const accessToken = (tokens as Record<string, unknown>).access_token;
    if (typeof accessToken === 'string' && accessToken.trim()) return true;
  }
  const body = record.body;
  if (body && typeof body === 'object' && !Array.isArray(body)) {
    const bodyTokens = (body as Record<string, unknown>).tokens;
    if (bodyTokens && typeof bodyTokens === 'object' && !Array.isArray(bodyTokens)) {
      const accessToken = (bodyTokens as Record<string, unknown>).access_token;
      return typeof accessToken === 'string' && Boolean(accessToken.trim());
    }
  }
  return false;
}

function hasGrokAccessToken(
  resolver: MockAdapterSourceResolver,
  sourceId: string,
): boolean {
  const account = resolver.getAccountById(sourceId) as ClassifiableAccount | undefined;
  if (!account || account.agentId !== 'grok' || account.kind !== 'oauth') return false;
  const credentials = account.credentials;
  if (!credentials || typeof credentials !== 'object') return false;
  const record = credentials as Record<string, unknown>;
  const candidates = [
    record.access_token,
    (record.tokens as Record<string, unknown> | undefined)?.access_token,
    ((record.body as Record<string, unknown> | undefined)?.tokens as Record<string, unknown> | undefined)?.access_token,
  ];
  return candidates.some((token) => typeof token === 'string' && Boolean(token.trim()));
}

/** Mirror of core `adapter_maturity_from_decision` on the public analysis surface. */
function mockPlanMaturity(analysis: AdapterRouteAnalysis): AdapterApplyPlan['maturity'] {
  const matrixOpen = analysis.route !== 'unsupported' && analysis.support !== 'unsupported';
  if (analysis.gateKind === 'preview_only') return 'preview';
  if (matrixOpen && analysis.support === 'stable') return 'stable';
  if (matrixOpen && analysis.support === 'experimental') return 'experimental';
  if (analysis.gateKind === 'subscription_candidate') {
    return 'preview';
  }
  return 'none';
}

/** Browser-only mirror of the core's explicit routing rules. */
const CONNECTION_SECRET_MARKER = '$AGENTHUB_CONNECTION_SECRET$';

function safeSourceId(sourceId: string): string {
  return sourceId.replace(/[^a-zA-Z0-9_-]/g, '-').slice(0, 40) || 'source';
}

function piSlotFromPlan(plan: AdapterApplyPlan, fallback: string): string {
  return plan.analysis.actions.find(
    (item) => item.kind === 'set_config' && item.target === 'Pi',
  )?.value ?? fallback;
}

function materializeApply(
  request: AdapterApplyRequest,
  plan: AdapterApplyPlan,
  existing: AdapterProfile | undefined,
  now: string,
): { profile: AdapterProfile; provider: Provider } {
  const safeId = safeSourceId(request.sourceId);
  if (plan.analysis.route === 'native_endpoint' && request.targetAgentId === 'grok') {
    const kimi = plan.analysis.ruleId === KIMI_GROK_RULE_ID;
    const alias = kimi ? 'agenthub_kimi' : 'agenthub_openai';
    const model = kimi ? 'kimi-k2.5' : 'gpt-4o';
    const baseUrl = kimi ? KIMI_GROK_BASE_URL : OPENAI_GROK_BASE_URL;
    const prefix = kimi ? 'kimi-grok' : 'openai-grok';
    const profile: AdapterProfile = existing ?? {
      id: `adapter-${prefix}-${safeId}`,
      name: `${kimi ? 'Kimi' : 'OpenAI'} → Grok (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: 'grok',
      route: 'native_endpoint',
      mode: 'api',
      status: 'active',
      ruleId: plan.analysis.ruleId!,
      ruleVersion: '1',
      generatedProviderId: `grok-${prefix}-adapter-${safeId}`,
      localPort: null,
      autoStart: false,
      createdAt: now,
      updatedAt: now,
    };
    return {
      profile,
      provider: {
        id: profile.generatedProviderId!,
        agentId: 'grok',
        name: profile.name,
        preset: 'openai-chat',
        configText: [
          '[models]',
          `default = "${alias}"`,
          '',
          `[model."${alias}"]`,
          `model = "${model}"`,
          `base_url = "${baseUrl}"`,
          `api_key = "${CONNECTION_SECRET_MARKER}"`,
          'api_backend = "chat_completions"',
        ].join('\n'),
        configFormat: 'toml',
        isCurrent: true,
      },
    };
  }
  if (plan.analysis.route === 'native_endpoint' && request.targetAgentId === 'codex') {
    const glm = plan.analysis.ruleId === GLM_CODEX_RULE_ID;
    const slug = glm ? GLM_CODEX_PROVIDER_SLUG : DEEPSEEK_CODEX_PROVIDER_SLUG;
    const model = glm ? 'glm-5.3' : 'deepseek-v4-flash';
    const baseUrl = glm ? GLM_CODEX_BASE_URL : DEEPSEEK_CODEX_BASE_URL;
    const profile: AdapterProfile = existing ?? {
      id: `adapter-${glm ? 'glm' : 'deepseek'}-codex-${safeId}`,
      name: `${glm ? 'GLM Coding Plan' : 'DeepSeek'} → Codex (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'native_endpoint',
      mode: 'api',
      status: 'active',
      ruleId: plan.analysis.ruleId!,
      ruleVersion: '1',
      generatedProviderId: `codex-${glm ? 'glm' : 'deepseek'}-adapter-${safeId}`,
      localPort: null,
      autoStart: false,
      createdAt: now,
      updatedAt: now,
    };
    return {
      profile,
      provider: {
        id: profile.generatedProviderId!,
        agentId: 'codex',
        name: profile.name,
        preset: 'openai-compatible',
        configText: [
          `model_provider = "${slug}"`,
          `model = "${model}"`,
          'model_reasoning_effort = "high"',
          'preferred_auth_method = "apikey"',
          '',
          `[model_providers.${slug}]`,
          `name = "${glm ? 'GLM Coding Plan' : 'DeepSeek'}"`,
          `base_url = "${baseUrl}"`,
          'wire_api = "responses"',
          `experimental_bearer_token = "${CONNECTION_SECRET_MARKER}"`,
        ].join('\n'),
        configFormat: 'toml',
        isCurrent: true,
      },
    };
  }
  if (plan.analysis.route === 'local_bridge') {
    const codexClaudeBridge = plan.analysis.ruleId === CODEX_CLAUDE_RULE_ID;
    const grokClaudeBridge = plan.analysis.ruleId === GROK_CLAUDE_RULE_ID;
    const anthropicBridge = plan.analysis.ruleId === 'anthropic-api-to-codex-v1';
    const profile: AdapterProfile = existing ?? {
      id: codexClaudeBridge
        ? `adapter-codex-claude-bridge-${safeId}`
        : grokClaudeBridge
        ? `adapter-grok-claude-bridge-${safeId}`
        : anthropicBridge
        ? `adapter-anthropic-codex-bridge-${safeId}`
        : `adapter-kimi-codex-bridge-${safeId}`,
      name: codexClaudeBridge
        ? `Codex → Claude Code 本地桥接 (${safeId})`
        : grokClaudeBridge
        ? `Grok → Claude Code 本地桥接 (${safeId})`
        : anthropicBridge
        ? `Anthropic → Codex 本地桥接 (${safeId})`
        : `Kimi → Codex 本地桥接 (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'local_bridge',
      mode: codexClaudeBridge || grokClaudeBridge ? 'oauth' : 'api',
      status: 'active',
      ruleId: codexClaudeBridge
        ? CODEX_CLAUDE_RULE_ID
        : grokClaudeBridge
        ? GROK_CLAUDE_RULE_ID
        : anthropicBridge ? 'anthropic-api-to-codex-v1' : 'kimi-membership-to-codex-v1',
      ruleVersion: '1',
      generatedProviderId: codexClaudeBridge
        ? `claude-codex-bridge-${safeId}`
        : grokClaudeBridge
        ? `claude-grok-bridge-${safeId}`
        : anthropicBridge
        ? `codex-anthropic-bridge-${safeId}`
        : `codex-kimi-bridge-${safeId}`,
      localPort: 32123,
      autoStart: false,
      createdAt: now,
      updatedAt: now,
    };
    return {
      profile,
      provider: {
        id: profile.generatedProviderId!,
        agentId: codexClaudeBridge || grokClaudeBridge ? 'claude' : 'codex',
        name: profile.name,
        preset: codexClaudeBridge || grokClaudeBridge ? 'anthropic' : 'openai-compatible',
        configText: JSON.stringify({
          ...(codexClaudeBridge || grokClaudeBridge
            ? {
                env: {
                  ANTHROPIC_BASE_URL: `http://127.0.0.1:${profile.localPort ?? 32123}`,
                  ANTHROPIC_AUTH_TOKEN: CONNECTION_SECRET_MARKER,
                },
              }
            : {
                baseUrl: `http://127.0.0.1:${profile.localPort ?? 32123}/v1`,
                model: anthropicBridge ? 'claude-sonnet-4-20250514' : 'kimi-k2.5',
              }),
        }),
        configFormat: 'json',
        isCurrent: true,
      },
    };
  }

  if (plan.analysis.route === 'config_sync' && request.targetAgentId === 'dsh') {
    const profile: AdapterProfile = existing ?? {
      id: `adapter-deepseek-dsh-${safeId}`,
      name: `DeepSeek → DSH (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'config_sync',
      mode: 'api',
      status: 'active',
      ruleId: 'deepseek-api-to-dsh-v1',
      ruleVersion: '1',
      generatedProviderId: `dsh-deepseek-adapter-${safeId}`,
      localPort: null,
      autoStart: false,
      createdAt: now,
      updatedAt: now,
    };
    return {
      profile,
      provider: {
        id: profile.generatedProviderId!,
        agentId: 'dsh',
        name: profile.name,
        preset: 'deepseek',
        configText: JSON.stringify({
          provider: 'deepseek-official',
          apiKeyEnv: 'DEEPSEEK_API_KEY',
          apiKey: CONNECTION_SECRET_MARKER,
        }),
        configFormat: 'json',
        isCurrent: true,
      },
    };
  }

  if (plan.analysis.route === 'config_sync' && request.targetAgentId === 'pi') {
    const ruleId = plan.analysis.ruleId
      ?? (plan.analysis.actions.find((item) => item.kind === 'set_config')?.value === 'anthropic'
        ? 'anthropic-api-to-pi-v1'
        : 'kimi-membership-to-pi-v1');
    const slotFallback = ruleId === 'openai-api-to-pi-v1'
      ? 'openai'
      : ruleId === 'xai-api-to-pi-v1'
        ? 'xai'
        : ruleId === 'glm-coding-plan-to-pi-v1'
          ? 'glm-coding-plan'
          : ruleId === 'deepseek-api-to-pi-v1'
            ? 'deepseek'
        : ruleId === 'anthropic-api-to-pi-v1'
          ? 'anthropic'
          : 'kimi-for-coding';
    const display = ruleId === 'openai-api-to-pi-v1'
      ? 'OpenAI'
      : ruleId === 'xai-api-to-pi-v1'
        ? 'xAI'
        : ruleId === 'glm-coding-plan-to-pi-v1'
          ? 'GLM Coding Plan'
          : ruleId === 'deepseek-api-to-pi-v1'
            ? 'DeepSeek'
        : ruleId === 'anthropic-api-to-pi-v1'
          ? 'Anthropic'
          : 'Kimi';
    const subscription = NATIVE_SUBSCRIPTION_PI_RULE_IDS.has(ruleId);
    const subscriptionDisplay = ruleId === 'claude-subscription-to-pi-v1'
      ? 'Claude'
      : ruleId === 'codex-subscription-to-pi-v1'
        ? 'Codex / ChatGPT'
        : 'Grok / xAI';
    const prefix = subscription
      ? ruleId === 'claude-subscription-to-pi-v1'
        ? 'claude-oauth'
        : ruleId === 'codex-subscription-to-pi-v1'
          ? 'codex-oauth'
          : 'grok-oauth'
      : ruleId === 'openai-api-to-pi-v1'
      ? 'openai'
      : ruleId === 'xai-api-to-pi-v1'
        ? 'xai'
      : ruleId === 'glm-coding-plan-to-pi-v1'
        ? 'glm'
        : ruleId === 'deepseek-api-to-pi-v1'
          ? 'deepseek'
        : ruleId === 'anthropic-api-to-pi-v1'
          ? 'anthropic'
          : 'kimi';
    const slot = piSlotFromPlan(plan, slotFallback);
    const piCustom = ruleId === 'glm-coding-plan-to-pi-v1' || ruleId === 'deepseek-api-to-pi-v1';
    const piBaseUrl = ruleId === 'glm-coding-plan-to-pi-v1' ? GLM_PI_BASE_URL : DEEPSEEK_PI_BASE_URL;
    const piModel = ruleId === 'glm-coding-plan-to-pi-v1' ? 'glm-4.6' : 'deepseek-chat';
    const profile: AdapterProfile = existing ?? {
      id: `adapter-${prefix}-pi-${safeId}`,
      name: `${subscription ? subscriptionDisplay : display} → Pi (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'config_sync',
      mode: subscription ? 'oauth' : 'api',
      status: 'active',
      ruleId,
      ruleVersion: '1',
      generatedProviderId: `pi-${prefix}-adapter-${safeId}`,
      localPort: null,
      autoStart: false,
      createdAt: now,
      updatedAt: now,
    };
    return {
      profile,
      provider: {
        id: profile.generatedProviderId!,
        agentId: 'pi',
        name: profile.name,
        preset: slot,
        configText: JSON.stringify(subscription
          ? {
              auth: {
                [slot]: {
                  type: 'oauth',
                  access: CONNECTION_SECRET_MARKER,
                  refresh: CONNECTION_SECRET_MARKER,
                },
              },
            }
          : piCustom
            ? {
                models: {
                  providers: {
                    [slot]: {
                      baseUrl: piBaseUrl,
                      api: 'openai-completions',
                      models: [{ id: piModel }],
                      apiKey: CONNECTION_SECRET_MARKER,
                    },
                  },
                },
              }
            : {
                slot,
                apiKey: CONNECTION_SECRET_MARKER,
              }),
        configFormat: 'json',
        isCurrent: true,
      },
    };
  }

  const claudeLayout = plan.analysis.ruleId === GLM_CLAUDE_RULE_ID
    ? { prefix: 'glm', display: 'GLM', baseUrl: GLM_CLAUDE_BASE_URL, ruleId: GLM_CLAUDE_RULE_ID }
    : plan.analysis.ruleId === DEEPSEEK_CLAUDE_RULE_ID
      ? { prefix: 'deepseek', display: 'DeepSeek', baseUrl: DEEPSEEK_CLAUDE_BASE_URL, ruleId: DEEPSEEK_CLAUDE_RULE_ID }
      : { prefix: 'kimi', display: 'Kimi', baseUrl: KIMI_CLAUDE_BASE_URL, ruleId: 'kimi-membership-to-claude-v1' };
  const profile: AdapterProfile = existing ?? {
    id: `adapter-${claudeLayout.prefix}-claude-${safeId}`,
    name: `${claudeLayout.display} → Claude (${safeId})`,
    sourceKind: request.sourceKind,
    sourceId: request.sourceId,
    targetAgentId: request.targetAgentId,
    route: 'native_endpoint',
    mode: 'api',
    status: 'active',
    ruleId: claudeLayout.ruleId,
    ruleVersion: '1',
    generatedProviderId: `claude-${claudeLayout.prefix}-adapter-${safeId}`,
    localPort: null,
    autoStart: false,
    createdAt: now,
    updatedAt: now,
  };
  return {
    profile,
    provider: {
      id: profile.generatedProviderId!,
      agentId: 'claude',
      name: profile.name,
      preset: 'anthropic-compatible',
      configText: JSON.stringify({
        env: {
          ANTHROPIC_BASE_URL: claudeLayout.baseUrl,
          ANTHROPIC_AUTH_TOKEN: CONNECTION_SECRET_MARKER,
        },
      }),
      configFormat: 'json',
      isCurrent: true,
    },
  };
}

export function createMockAdapterPort(resolver: MockAdapterSourceResolver): AdapterPort {
  const state: MockAdapterState = {
    profiles: [],
    bridgeStatuses: new Map(),
    generatedProviders: new Map(),
    removeGeneratedProvider: resolver.removeGeneratedProvider,
  };
  adapterStates.add(state);

  return {
    async analyze(request) {
      await delay(20);
      return analyze(resolver, request);
    },
    async plan(request) {
      await delay(20);
      return buildPlan(resolver, request, analyze(resolver, request));
    },
    async listProfiles(filter: AdapterProfileFilter = {}) {
      await delay(20);
      return state.profiles
        .filter((profile) => !filter.sourceKind || profile.sourceKind === filter.sourceKind)
        .filter((profile) => !filter.sourceId || profile.sourceId === filter.sourceId)
        .filter((profile) => !filter.targetAgentId || profile.targetAgentId === filter.targetAgentId)
        .filter((profile) => !filter.mode || profile.mode === filter.mode)
        .filter((profile) => !filter.route || profile.route === filter.route)
        .filter((profile) => !filter.status || profile.status === filter.status)
        .filter((profile) => filter.autoStart == null || profile.autoStart === filter.autoStart)
        .map((profile) => ({ ...profile }));
    },
    async apply(request: AdapterApplyRequest): Promise<AdapterApplyResult> {
      await delay(20);
      const plan = buildPlan(resolver, request, analyze(resolver, request));
      if (!plan.canApply) {
        throw adapterCommandError({
          code: 'unsupported',
          message: '当前适配路径尚不可应用',
          retryable: false,
        });
      }
      // Re-validate source secrets independently of plan.canApply (same rule as core).
      if (plan.analysis.ruleId && KIMI_MEMBERSHIP_RULE_IDS.has(plan.analysis.ruleId)) {
        const providerSource = request.sourceKind === 'provider'
          ? resolver.getProviderById(request.sourceId)
          : undefined;
        const accountSource = request.sourceKind === 'account'
          ? resolver.getAccountById(request.sourceId) as ClassifiableAccount | undefined
          : undefined;
        const valid = request.sourceKind === 'provider'
          ? !!providerSource && isKimiMembershipProvider(providerSource)
          : isKimiMembershipAccount(accountSource) && hasAccountApiKey(accountSource);
        if (!valid) {
          throw adapterCommandError({
            code: 'invalid_arg',
            message: 'invalid adapter secret reference',
            retryable: false,
          });
        }
      }
      if (plan.analysis.ruleId === CODEX_CLAUDE_RULE_ID
        && !hasCodexAccessToken(resolver, request.sourceId)) {
        throw adapterCommandError({
          code: 'invalid_arg',
          message: 'invalid adapter secret reference',
          retryable: false,
        });
      }
      const existing = state.profiles.find(
        (profile) =>
          profile.sourceKind === request.sourceKind &&
          profile.sourceId === request.sourceId &&
          profile.targetAgentId === request.targetAgentId,
      );
      const now = new Date().toISOString();
      const { profile, provider } = materializeApply(request, plan, existing, now);
      if (!existing) state.profiles.push(profile);
      if (plan.analysis.route === 'local_bridge') {
        state.bridgeStatuses.set(profile.id, runningBridgeStatus(profile));
      }
      const generated = resolver.upsertGeneratedProvider?.(provider) ?? provider;
      state.generatedProviders.set(generated.id, { ...generated });
      return {
        profile: { ...profile },
        provider: { ...generated },
      };
    },
    async remove(profileId: string) {
      await delay(20);
      const index = state.profiles.findIndex((profile) => profile.id === profileId);
      if (index < 0) {
        throw adapterCommandError({
          code: 'not_found',
          message: `adapter profile not found: ${profileId}`,
          retryable: false,
        });
      }
      const profile = state.profiles[index];
      const providerId = profile.generatedProviderId;
      const generated = providerId
        ? resolver.getProviderById(providerId) ?? state.generatedProviders.get(providerId)
        : undefined;
      if (!generated) {
        throw adapterCommandError({
          code: 'not_found',
          message: '适配生成的 Connection 不存在，无法安全删除',
          retryable: false,
        });
      }
      resolver.removeGeneratedProvider?.(generated);
      state.generatedProviders.delete(generated.id);
      state.bridgeStatuses.delete(profileId);
      state.profiles.splice(index, 1);
    },
    async startBridge(profileId) {
      await delay(20);
      const profile = localBridgeProfile(state, profileId);
      const status = runningBridgeStatus(profile);
      state.bridgeStatuses.set(profileId, status);
      return { ...status };
    },
    async stopBridge(profileId) {
      await delay(20);
      const profile = localBridgeProfile(state, profileId);
      const current = state.bridgeStatuses.get(profileId);
      const status: AdapterBridgeRuntimeStatus = {
        profileId,
        state: 'stopped',
        port: profile.localPort ?? current?.port ?? null,
        endpoint: profile.localPort ? `http://127.0.0.1:${profile.localPort}/v1` : null,
        startedAt: current?.startedAt ?? null,
        upstreamStatus: 'stopped',
      };
      state.bridgeStatuses.set(profileId, status);
      return { ...status };
    },
    async getBridgeStatus(profileId) {
      await delay(20);
      const profile = localBridgeProfile(state, profileId);
      const status = state.bridgeStatuses.get(profileId) ?? {
        profileId,
        state: 'stopped' as const,
        port: profile.localPort ?? null,
        endpoint: profile.localPort ? `http://127.0.0.1:${profile.localPort}/v1` : null,
        startedAt: null,
        upstreamStatus: 'stopped',
      };
      return { ...status };
    },
    async setBridgeAutoStart(profileId, autoStart) {
      await delay(20);
      const profile = localBridgeProfile(state, profileId);
      profile.autoStart = autoStart;
      profile.updatedAt = new Date().toISOString();
      return { ...profile };
    },
  };
}

function localBridgeProfile(state: MockAdapterState, profileId: string): AdapterProfile {
  const profile = state.profiles.find((item) => item.id === profileId);
  if (!profile) {
    throw adapterCommandError({
      code: 'not_found',
      message: `adapter profile not found: ${profileId}`,
      retryable: false,
    });
  }
  if (profile.route !== 'local_bridge') {
    throw adapterCommandError({
      code: 'unsupported',
      message: '此适配不需要本地桥接',
      retryable: false,
    });
  }
  return profile;
}

function runningBridgeStatus(profile: AdapterProfile): AdapterBridgeRuntimeStatus {
  const port = profile.localPort ?? 32123;
  return {
    profileId: profile.id,
    state: 'running',
    port,
    endpoint: `http://127.0.0.1:${port}/v1`,
    startedAt: new Date().toISOString(),
    upstreamStatus: 'unknown',
  };
}
