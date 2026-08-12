import type {
  AdapterAction,
  AdapterApplyRequest,
  AdapterApplyResult,
  AdapterApplyPlan,
  AdapterBridgeRuntimeStatus,
  AdapterEvidence,
  AdapterPlanChange,
  AdapterPort,
  AdapterProfile,
  AdapterProfileFilter,
  AdapterRouteAnalysis,
  AdapterRouteRequest,
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

function unsupported(reason: string, evidenceItems: AdapterEvidence[]): AdapterRouteAnalysis {
  return {
    route: 'unsupported',
    support: 'unsupported',
    reason,
    actions: [],
    limitations: ['仅支持只读预览；不会 apply、sync 或启动 bridge。'],
    evidence: evidenceItems,
  };
}

function classify(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): 'kimi_membership' | 'anthropic_api_key' | 'other' | 'not_found' {
  if (request.sourceKind === 'provider') {
    const provider = resolver.getProviderById(request.sourceId);
    if (!provider) return 'not_found';
    if (provider.agentId === 'kimi' && provider.preset === 'kimi-code-membership') {
      return 'kimi_membership';
    }
    if (provider.agentId === 'claude' && provider.preset === 'anthropic') {
      return 'anthropic_api_key';
    }
    return 'other';
  }

  const account = resolver.getAccountById(request.sourceId);
  if (!account) return 'not_found';
  return account.kind === 'apikey' && account.provider?.toLowerCase() === 'anthropic'
    ? 'anthropic_api_key'
    : 'other';
}

function analyze(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): AdapterRouteAnalysis {
  const source = classify(resolver, request);
  if (source === 'not_found') {
    throw new Error(`${request.sourceKind} not found: ${request.sourceId}`);
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
      limitations: ['Phase 0 仅预览；不会同步配置或传输凭据。'],
      evidence: [evidence('Kimi Code CLI provider configuration', 'https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/providers.html')],
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
      limitations: ['Phase 0 仅预览；不会同步配置或传输凭据。'],
      evidence: [evidence('Pi custom provider and model configuration', 'https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md')],
    };
  }
  return unsupported(
    source === 'kimi_membership'
      ? 'Kimi Code 会员当前仅支持预览到 Claude、Codex 或 Pi。'
      : source === 'anthropic_api_key'
        ? 'Anthropic API Key 当前仅支持预览到 Pi。'
        : '该连接没有可识别的显式 Adapter 路由标记。',
    [evidence('Adapter Phase 0 compatibility scope', 'https://www.kimi.com/code/docs/')],
  );
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
  return {
    analysis,
    targetAgentId: request.targetAgentId,
    canApply: (analysis.route === 'native_endpoint' && request.targetAgentId === 'claude')
      || (analysis.route === 'local_bridge' && request.targetAgentId === 'codex'),
    serviceImpact: analysis.route === 'local_bridge' ? 'requires_local_bridge' : 'none',
    changes,
  };
}

/** Browser-only mirror of the core's explicit routing rules. */
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
        .map((profile) => ({ ...profile }));
    },
    async apply(request: AdapterApplyRequest): Promise<AdapterApplyResult> {
      await delay(20);
      const plan = buildPlan(request, analyze(resolver, request));
      if (!plan.canApply) throw new Error('当前适配路径尚不可应用');
      const existing = state.profiles.find(
        (profile) =>
          profile.sourceKind === request.sourceKind &&
          profile.sourceId === request.sourceId &&
          profile.targetAgentId === request.targetAgentId,
      );
      const now = new Date().toISOString();
      const safeId = request.sourceId.replace(/[^a-zA-Z0-9_-]/g, '-').slice(0, 40) || 'source';
      const isLocalBridge = plan.analysis.route === 'local_bridge';
      const profile: AdapterProfile = existing ?? {
        id: `adapter-kimi-${isLocalBridge ? 'codex-bridge' : 'claude'}-${safeId}`,
        name: `Kimi → ${isLocalBridge ? 'Codex 本地桥接' : 'Claude'} (${safeId})`,
        sourceKind: request.sourceKind,
        sourceId: request.sourceId,
        targetAgentId: request.targetAgentId,
        route: isLocalBridge ? 'local_bridge' : 'native_endpoint',
        status: 'active',
        ruleId: isLocalBridge ? 'kimi-membership-to-codex-bridge-v1' : 'kimi-membership-to-claude-v1',
        ruleVersion: '1',
        generatedProviderId: isLocalBridge
          ? `codex-kimi-bridge-${safeId}`
          : `claude-kimi-adapter-${safeId}`,
        localPort: isLocalBridge ? 32123 : null,
        // Match desktop apply: local bridges are opt-in for auto-start.
        autoStart: false,
        createdAt: now,
        updatedAt: now,
      };
      if (!existing) state.profiles.push(profile);
      if (isLocalBridge) {
        state.bridgeStatuses.set(profile.id, runningBridgeStatus(profile));
      }
      const provider: Provider = isLocalBridge ? {
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
      } : {
        id: profile.generatedProviderId!,
        agentId: 'claude',
        name: profile.name,
        preset: 'anthropic-compatible',
        configText: JSON.stringify({
          env: {
            ANTHROPIC_BASE_URL: 'https://api.kimi.com/coding/',
            ANTHROPIC_AUTH_TOKEN: '$AGENTHUB_CONNECTION_SECRET$',
          },
        }),
        configFormat: 'json',
        isCurrent: true,
      };
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
      if (index < 0) throw new Error(`adapter profile not found: ${profileId}`);
      const profile = state.profiles[index];
      const providerId = profile.generatedProviderId;
      const generated = providerId
        ? resolver.getProviderById(providerId) ?? state.generatedProviders.get(providerId)
        : undefined;
      if (!generated) throw new Error('适配生成的 Connection 不存在，无法安全删除');
      if (generated.isCurrent) {
        throw new Error('请先在 Connections 切换到其他连接，再删除此适配');
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
        // Desktop bridge DTO currently only emits upstream "unknown"; keep mock
        // aligned so dogfood does not invent richer health than Tauri returns.
        upstreamStatus: 'unknown',
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
        upstreamStatus: 'unknown',
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
  if (!profile) throw new Error(`adapter profile not found: ${profileId}`);
  if (profile.route !== 'local_bridge') throw new Error('此适配不需要本地桥接');
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
