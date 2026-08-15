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
  'Codex / ChatGPT 订阅 → Claude Code：当前不支持。',
  '尚未通过上游授权、条款与协议兼容性门禁，plan.canApply=false。',
  '不会创建适配、启动 Bridge，也不会把订阅凭据写入 Claude。',
  '这只表示没有可执行规则，不代表连接失效。',
  '替代路径：在 Claude 使用自身官方登录，或改用已支持的 API Key 来源。',
].join('');

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
  | 'codex_subscription'
  | 'codex_subscription_oauth_other'
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
const KIMI_MEMBERSHIP_RULE_IDS = new Set([
  'kimi-membership-to-claude-v1',
  'kimi-membership-to-codex-v1',
  'kimi-membership-to-pi-v1',
]);

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

  if (account.kind === 'apikey' && explicitProvider?.toLowerCase() === 'anthropic') {
    return 'anthropic_api_key';
  }
  if (account.agentId === 'codex' && account.kind === 'oauth') {
    return isCodexAuthJson(credentialFormat, account.credentials ?? {})
      ? 'codex_subscription'
      : 'codex_subscription_oauth_other';
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

  const compatibilityEvidence = [evidence(
    'AgentHub：厂商、API 与 OAuth 适配规则',
    'https://github.com/nicechencs/AgentHub/blob/release/docs/provider-api-oauth-adaptation.md',
  )];
  // Codex / ChatGPT subscription → Claude is a gated experimental candidate only.
  // auth_json matches the closed matrix cell (ruleId present). Bare Codex OAuth
  // still uses the closed subscription surface but does not pretend the cell matched.
  if (
    (source === 'codex_subscription' || source === 'codex_subscription_oauth_other')
    && request.targetAgentId === 'claude'
  ) {
    return unsupported(CODEX_SUBSCRIPTION_TO_CLAUDE_REASON, compatibilityEvidence, {
      gateKind: 'subscription_candidate',
      ruleId: source === 'codex_subscription'
        ? 'codex-subscription-to-claude-app-server-v0'
        : null,
    });
  }
  const reason = source === 'kimi_membership'
    ? 'Kimi Code 会员当前仅支持预览到 Claude、Codex 或 Pi。'
    : source === 'kimi_non_membership'
      ? KIMI_NON_MEMBERSHIP_REASON
    : source === 'anthropic_api_key'
      ? 'Anthropic API Key 当前仅支持预览到 Pi。'
      : source === 'codex_subscription' || source === 'codex_subscription_oauth_other'
        ? 'AgentHub 暂未提供从 Codex 账户到所选目标的适配规则。当前不支持不等于连接失效。'
        : 'AgentHub 暂未提供此来源到所选目标的适配规则。当前不支持不等于连接失效。';
  return unsupported(reason, compatibilityEvidence);
}

function buildPlan(request: AdapterRouteRequest, analysis: AdapterRouteAnalysis): AdapterApplyPlan {
  const configuredProvider = analysis.actions.find(
    (item) => item.kind === 'set_config' && item.target === 'Pi',
  )?.value;
  const changes = analysis.route === 'native_endpoint' && request.targetAgentId === 'claude'
    ? [
        change('claude', 'baseUrl', 'https://api.kimi.com/coding/'),
        change('claude', 'claudeAuthEnv', 'ANTHROPIC_AUTH_TOKEN'),
        secretChange('claude', 'apiKey'),
      ]
      : analysis.route === 'local_bridge' && request.targetAgentId === 'codex'
        ? [
            change('codex', 'provider', 'AgentHub Kimi 本地桥接'),
            change('codex', 'baseUrl', 'http://127.0.0.1:<本机端口>/v1'),
          ]
        : analysis.route === 'config_sync' && request.targetAgentId === 'pi'
      ? [
          change('pi', 'provider', configuredProvider ?? 'anthropic'),
          secretChange('pi', 'apiKey'),
        ]
      : [];
  const implementedPath =
    (analysis.route === 'native_endpoint' && analysis.support === 'stable' && request.targetAgentId === 'claude')
    || (analysis.route === 'local_bridge' && analysis.support === 'experimental' && request.targetAgentId === 'codex')
    || (analysis.route === 'config_sync' && analysis.support === 'stable' && request.targetAgentId === 'pi');
  const writeGate = request.sourceKind === 'provider' && implementedPath;
  const canApply = writeGate;
  const maturity = mockPlanMaturity(analysis);
  // Same condition as core `write_gate`: matrix-open implemented path ∩ non-Provider.
  const reason = implementedPath && request.sourceKind !== 'provider'
    ? `${analysis.reason} ${SAME_EDGE_UNWRITABLE_REASON}`
    : analysis.reason;
  return {
    analysis,
    targetAgentId: request.targetAgentId,
    canApply,
    maturity,
    reason,
    serviceImpact: analysis.route === 'local_bridge' ? 'requires_local_bridge' : 'none',
    changes,
  };
}

/** Mirror of core `adapter_maturity_from_decision` on the public analysis surface. */
function mockPlanMaturity(analysis: AdapterRouteAnalysis): AdapterApplyPlan['maturity'] {
  const matrixOpen = analysis.route !== 'unsupported' && analysis.support !== 'unsupported';
  if (matrixOpen && analysis.support === 'stable') return 'stable';
  if (matrixOpen && analysis.support === 'experimental') return 'experimental';
  if (analysis.gateKind === 'subscription_candidate' || analysis.gateKind === 'preview_only') {
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
  if (plan.analysis.route === 'local_bridge') {
    const profile: AdapterProfile = existing ?? {
      id: `adapter-kimi-codex-bridge-${safeId}`,
      name: `Kimi → Codex 本地桥接 (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'local_bridge',
      mode: 'api',
      status: 'active',
      ruleId: 'kimi-membership-to-codex-v1',
      ruleVersion: '1',
      generatedProviderId: `codex-kimi-bridge-${safeId}`,
      localPort: 32123,
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
        configText: JSON.stringify({
          baseUrl: `http://127.0.0.1:${profile.localPort ?? 32123}/v1`,
          model: 'kimi-k2.5',
        }),
        configFormat: 'json',
        isCurrent: true,
      },
    };
  }

  if (plan.analysis.route === 'config_sync' && request.targetAgentId === 'pi') {
    const isAnthropic = plan.analysis.ruleId === 'anthropic-api-to-pi-v1';
    const ruleId = isAnthropic ? 'anthropic-api-to-pi-v1' : 'kimi-membership-to-pi-v1';
    const slot = piSlotFromPlan(plan, isAnthropic ? 'anthropic' : 'kimi-for-coding');
    const profile: AdapterProfile = existing ?? {
      id: isAnthropic ? `adapter-anthropic-pi-${safeId}` : `adapter-kimi-pi-${safeId}`,
      name: `${isAnthropic ? 'Anthropic' : 'Kimi'} → Pi (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'config_sync',
      mode: 'api',
      status: 'active',
      ruleId,
      ruleVersion: '1',
      generatedProviderId: isAnthropic
        ? `pi-anthropic-adapter-${safeId}`
        : `pi-kimi-adapter-${safeId}`,
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
        configText: JSON.stringify({
          slot,
          apiKey: CONNECTION_SECRET_MARKER,
        }),
        configFormat: 'json',
        isCurrent: true,
      },
    };
  }

  const profile: AdapterProfile = existing ?? {
    id: `adapter-kimi-claude-${safeId}`,
    name: `Kimi → Claude (${safeId})`,
    sourceKind: request.sourceKind,
    sourceId: request.sourceId,
    targetAgentId: request.targetAgentId,
    route: 'native_endpoint',
    mode: 'api',
    status: 'active',
    ruleId: 'kimi-membership-to-claude-v1',
    ruleVersion: '1',
    generatedProviderId: `claude-kimi-adapter-${safeId}`,
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
          ANTHROPIC_BASE_URL: 'https://api.kimi.com/coding/',
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
      return buildPlan(request, analyze(resolver, request));
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
      const plan = buildPlan(request, analyze(resolver, request));
      if (!plan.canApply) {
        throw adapterCommandError({
          code: 'unsupported',
          message: '当前适配路径尚不可应用',
          retryable: false,
        });
      }
      // Re-validate membership independently of plan.canApply (same rule as core).
      if (plan.analysis.ruleId && KIMI_MEMBERSHIP_RULE_IDS.has(plan.analysis.ruleId)) {
        const source = request.sourceKind === 'provider'
          ? resolver.getProviderById(request.sourceId)
          : undefined;
        if (!source || !isKimiMembershipProvider(source)) {
          throw adapterCommandError({
            code: 'invalid_arg',
            message: 'invalid adapter secret reference',
            retryable: false,
          });
        }
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
      if (generated.isCurrent) {
        throw adapterCommandError({
          code: 'unsupported',
          message: '请先在 Connections 切换到其他连接，再删除此适配',
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
