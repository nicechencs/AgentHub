/**
 * Apply/preview projection keyed by golden ruleId.
 *
 * Does not decide route, support, reason, gateKind, or canApply — those
 * come from golden.expect. This only explains how to preview and write
 * a memory profile once the kernel row is known.
 */
import type {
  AdapterAction,
  AdapterApplyPlan,
  AdapterApplyRequest,
  AdapterEvidence,
  AdapterMaturity,
  AdapterPlanChange,
  AdapterProfile,
  AdapterProfileMode,
  AdapterRouteAnalysis,
  AdapterRouteRequest,
} from '@/lib/backend/contracts/adapter';
import type { Provider } from '@/lib/types';
import type { GoldenExpect } from './golden-lookup';
import {
  action,
  change,
  evidence,
  secretAction,
  secretChange,
} from './types';

export const CONNECTION_SECRET_MARKER = '$AGENTHUB_CONNECTION_SECRET$';

const COMPAT_EVIDENCE = evidence(
  'AgentHub：厂商、API 与 OAuth 适配规则',
  'https://github.com/nicechencs/AgentHub/blob/release/docs/provider-api-oauth-adaptation.md',
);

const UNKNOWN_REASON =
  'AgentHub 暂未提供此来源到所选目标的适配规则。当前不支持不等于连接失效。';

const PI_SUBSCRIPTION_LIMITS = [
  '会把官方登录写进 Pi 认的位置；预览和日志不显示完整令牌。',
  '写进去之后由 Pi 自己续期；AgentHub 不会再刷一次。原来的工具和 Pi 一起续期可能互相踢下线。',
  '接上后会把自动生成的配置设成 Pi 当前在用的连接。',
] as const;

export type MockMaterializeSpec =
  | { kind: 'claude_native'; prefix: string; display: string; baseUrl: string }
  | { kind: 'grok_chat'; prefix: string; display: string; alias: string; model: string; baseUrl: string }
  | { kind: 'codex_responses'; prefix: string; display: string; slug: string; model: string; baseUrl: string }
  | { kind: 'codex_login'; prefix: string; display: string }
  | { kind: 'pi_slot'; prefix: string; display: string; slot: string }
  | { kind: 'pi_custom'; prefix: string; display: string; slot: string; baseUrl: string; model: string }
  | { kind: 'pi_subscription'; prefix: string; display: string; slot: string }
  | { kind: 'dsh'; prefix: string; display: string }
  | { kind: 'local_bridge'; prefix: string; display: string; mode: AdapterProfileMode; codexLabel?: string };

const BY_RULE_ID: Record<string, MockMaterializeSpec> = {
  'kimi-membership-to-claude-v1': {
    kind: 'claude_native', prefix: 'kimi', display: 'Kimi', baseUrl: 'https://api.kimi.com/coding/',
  },
  'glm-coding-plan-to-claude-v1': {
    kind: 'claude_native', prefix: 'glm', display: 'GLM', baseUrl: 'https://open.bigmodel.cn/api/anthropic',
  },
  'deepseek-api-to-claude-v1': {
    kind: 'claude_native', prefix: 'deepseek', display: 'DeepSeek', baseUrl: 'https://api.deepseek.com/anthropic',
  },
  'kimi-membership-to-grok-v1': {
    kind: 'grok_chat',
    prefix: 'kimi-grok',
    display: 'Kimi',
    alias: 'agenthub_kimi',
    model: 'kimi-k2.5',
    baseUrl: 'https://api.kimi.com/coding/v1',
  },
  'openai-api-to-grok-v1': {
    kind: 'grok_chat',
    prefix: 'openai-grok',
    display: 'OpenAI',
    alias: 'agenthub_openai',
    model: 'gpt-4o',
    baseUrl: 'https://api.openai.com/v1',
  },
  'glm-coding-plan-to-codex-v1': {
    kind: 'codex_responses',
    prefix: 'glm',
    display: 'GLM Coding Plan',
    slug: 'agenthub_glm',
    model: 'glm-5.3',
    baseUrl: 'https://open.bigmodel.cn/api/v1',
  },
  'deepseek-api-to-codex-v1': {
    kind: 'codex_responses',
    prefix: 'deepseek',
    display: 'DeepSeek',
    slug: 'agenthub_deepseek',
    model: 'deepseek-v4-flash',
    baseUrl: 'https://api.deepseek.com',
  },
  'codex-subscription-to-codex-v1': { kind: 'codex_login', prefix: 'codex-login', display: 'Codex' },
  'kimi-membership-to-pi-v1': { kind: 'pi_slot', prefix: 'kimi', display: 'Kimi', slot: 'kimi-for-coding' },
  'anthropic-api-to-pi-v1': { kind: 'pi_slot', prefix: 'anthropic', display: 'Anthropic', slot: 'anthropic' },
  'openai-api-to-pi-v1': { kind: 'pi_slot', prefix: 'openai', display: 'OpenAI', slot: 'openai' },
  'xai-api-to-pi-v1': { kind: 'pi_slot', prefix: 'xai', display: 'xAI', slot: 'xai' },
  'glm-coding-plan-to-pi-v1': {
    kind: 'pi_custom',
    prefix: 'glm',
    display: 'GLM Coding Plan',
    slot: 'glm-coding-plan',
    baseUrl: 'https://open.bigmodel.cn/api/coding/paas/v4',
    model: 'glm-4.6',
  },
  'deepseek-api-to-pi-v1': {
    kind: 'pi_custom',
    prefix: 'deepseek',
    display: 'DeepSeek',
    slot: 'deepseek',
    baseUrl: 'https://api.deepseek.com',
    model: 'deepseek-chat',
  },
  'claude-subscription-to-pi-v1': {
    kind: 'pi_subscription', prefix: 'claude-oauth', display: 'Claude', slot: 'anthropic',
  },
  'codex-subscription-to-pi-v1': {
    kind: 'pi_subscription', prefix: 'codex-oauth', display: 'Codex / ChatGPT', slot: 'openai-codex',
  },
  'grok-subscription-to-pi-v1': {
    kind: 'pi_subscription', prefix: 'grok-oauth', display: 'Grok / xAI', slot: 'xai',
  },
  'deepseek-api-to-dsh-v1': { kind: 'dsh', prefix: 'deepseek', display: 'DeepSeek' },
  'kimi-membership-to-codex-v1': {
    kind: 'local_bridge', prefix: 'kimi', display: 'Kimi', mode: 'api', codexLabel: 'AgentHub Kimi 本机路由',
  },
  'anthropic-api-to-codex-v1': {
    kind: 'local_bridge',
    prefix: 'anthropic',
    display: 'Anthropic',
    mode: 'api',
    codexLabel: 'AgentHub Anthropic 本机路由',
  },
  'openai-api-to-codex-v1': {
    kind: 'local_bridge', prefix: 'openai', display: 'OpenAI', mode: 'api', codexLabel: 'AgentHub OpenAI 本机路由',
  },
  'grok-subscription-to-codex-v1': {
    kind: 'local_bridge', prefix: 'grok', display: 'Grok', mode: 'oauth', codexLabel: 'AgentHub Grok 本机路由',
  },
  'claude-subscription-to-codex-v1': {
    kind: 'local_bridge', prefix: 'claude', display: 'Claude', mode: 'oauth', codexLabel: 'AgentHub Claude 本机路由',
  },
  'openai-api-to-claude-v1': {
    kind: 'local_bridge', prefix: 'openai', display: 'OpenAI', mode: 'api',
  },
  'openai-api-to-grok-bridge-v1': {
    kind: 'local_bridge', prefix: 'openai', display: 'OpenAI', mode: 'api',
  },
  'codex-subscription-to-claude-responses-v1': {
    kind: 'local_bridge', prefix: 'codex', display: 'Codex', mode: 'oauth',
  },
  'grok-subscription-to-claude-v1': {
    kind: 'local_bridge', prefix: 'grok', display: 'Grok', mode: 'oauth',
  },
  'codex-subscription-to-grok-v1': {
    kind: 'local_bridge', prefix: 'codex', display: 'Codex', mode: 'oauth',
  },
  'codex-subscription-to-kimi-v1': {
    kind: 'local_bridge', prefix: 'codex', display: 'Codex', mode: 'oauth',
  },
  'codex-subscription-to-dsh-v1': {
    kind: 'local_bridge', prefix: 'codex', display: 'Codex', mode: 'oauth',
  },
};

export function getMaterializeSpec(ruleId: string | null | undefined): MockMaterializeSpec | undefined {
  if (!ruleId) return undefined;
  return BY_RULE_ID[ruleId];
}

function previewActions(spec: MockMaterializeSpec | undefined, target: string): AdapterAction[] {
  if (!spec) {
    if (target) {
      return [action('requires_local_bridge', target, `${target} 需要本机转发。`)];
    }
    return [];
  }
  switch (spec.kind) {
    case 'claude_native':
      return [
        action('set_config', 'Claude Code', `设置 ${spec.display} 官方 Anthropic-compatible Base URL。`, spec.baseUrl),
        action('set_env', 'Claude Code', '使用 Claude Code 的认证环境变量名。', 'ANTHROPIC_AUTH_TOKEN'),
        secretAction('Claude Code', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ];
    case 'grok_chat':
      return [
        action('set_config', 'Grok', '写入 Grok 官方 OpenAI Chat Completions TOML。', spec.baseUrl),
        action(
          'set_config',
          'Grok',
          `使用 Grok Chat Completions 与 ${spec.model}。`,
          `api_backend=chat_completions; model=${spec.model}`,
        ),
        secretAction('Grok', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ];
    case 'codex_responses':
      return [
        action('set_config', 'Codex', `${spec.display} 官方 Responses Base URL；不会启动本机路由。`, spec.baseUrl),
        action(
          'set_config',
          'Codex',
          `使用 Codex Responses wire_api 与默认模型 ${spec.model}。`,
          `wire_api=responses; model=${spec.model}`,
        ),
        secretAction('Codex', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ];
    case 'codex_login':
      return [action('set_config', 'Codex', '写入 Codex 官方登录，不改本机路由。', '官方登录')];
    case 'pi_slot':
      return [
        action('set_config', 'Pi', `选择 Pi 的 ${spec.display} provider。`, spec.slot),
        secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ];
    case 'pi_custom':
      return [
        action('set_config', 'Pi', `写入 Pi 的 ${spec.display} 自定义 provider 位置。`, spec.slot),
        secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ];
    case 'pi_subscription':
      return [
        action('set_config', 'Pi', '选择 Pi 的订阅登录位置。', spec.slot),
        secretAction('Pi', '从已选 Connection 引用授权（OAuth）；不会读取或显示 token。'),
      ];
    case 'dsh':
      return [
        action('set_config', 'DeepSeek Harness', '选择 DSH 的官方 DeepSeek provider。', 'deepseek-official'),
        secretAction('DeepSeek Harness', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ];
    case 'local_bridge': {
      const label = targetLabel(target);
      return [
        action('requires_local_bridge', label, `${label} 需要本机转发。`),
      ];
    }
  }
}

function previewLimitations(spec: MockMaterializeSpec | undefined): string[] {
  if (spec?.kind === 'pi_subscription') return [...PI_SUBSCRIPTION_LIMITS];
  return [
    '自动生成的配置只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。',
  ];
}

function targetLabel(target: string): string {
  if (target === 'claude') return 'Claude Code';
  if (target === 'dsh') return 'DeepSeek Harness';
  if (target === 'pi') return 'Pi';
  return target.charAt(0).toUpperCase() + target.slice(1);
}

export function unsupportedAnalysis(reason = UNKNOWN_REASON): AdapterRouteAnalysis {
  return {
    route: 'unsupported',
    support: 'unsupported',
    reason,
    actions: [],
    limitations: [
      '当前不支持此组合；不会改动来源连接、本机服务或配置。',
      '现在还写不上去；不会改配置，也不会开本机转发。',
    ],
    evidence: [COMPAT_EVIDENCE],
    ruleId: null,
    gateKind: 'unsupported',
  };
}

export function analysisFromExpect(
  expect: GoldenExpect,
  request: AdapterRouteRequest,
): AdapterRouteAnalysis {
  if (expect.route === 'unsupported') {
    return {
      ...unsupportedAnalysis(expect.reason),
      ruleId: expect.ruleId,
      gateKind: expect.gateKind,
    };
  }
  const spec = getMaterializeSpec(expect.ruleId);
  const evidenceItems: AdapterEvidence[] = [COMPAT_EVIDENCE];
  return {
    route: expect.route,
    support: expect.support,
    reason: expect.reason,
    actions: previewActions(spec, request.targetAgentId),
    limitations: previewLimitations(spec),
    evidence: evidenceItems,
    ruleId: expect.ruleId,
    gateKind: expect.gateKind,
  };
}

function maturityFromExpect(expect: GoldenExpect): AdapterMaturity {
  const matrixOpen = expect.route !== 'unsupported' && expect.support !== 'unsupported';
  if (expect.gateKind === 'preview_only') return 'preview';
  if (matrixOpen && expect.support === 'stable') return 'stable';
  if (matrixOpen && expect.support === 'experimental') return 'experimental';
  if (expect.gateKind === 'subscription_candidate') return 'preview';
  return 'none';
}

function changesFromExpect(expect: GoldenExpect, request: AdapterRouteRequest): AdapterPlanChange[] {
  const spec = getMaterializeSpec(expect.ruleId);
  const target = request.targetAgentId;
  if (expect.route === 'native_endpoint' && target === 'claude' && spec?.kind === 'claude_native') {
    return [
      change('claude', 'baseUrl', spec.baseUrl),
      change('claude', 'claudeAuthEnv', 'ANTHROPIC_AUTH_TOKEN'),
      secretChange('claude', 'apiKey'),
    ];
  }
  if (expect.route === 'native_endpoint' && target === 'grok' && spec?.kind === 'grok_chat') {
    return [
      change('grok', 'baseUrl', spec.baseUrl),
      change('grok', 'model', spec.model),
      change('grok', 'apiBackend', 'chat_completions'),
      secretChange('grok', 'apiKey'),
    ];
  }
  if (expect.route === 'native_endpoint' && target === 'codex' && spec?.kind === 'codex_login') {
    return [change('codex', 'login', '官方登录')];
  }
  if (expect.route === 'native_endpoint' && target === 'codex' && spec?.kind === 'codex_responses') {
    return [
      change('codex', 'provider', spec.display),
      change('codex', 'baseUrl', spec.baseUrl),
      change('codex', 'wireApi', 'responses'),
    ];
  }
  if (expect.route === 'config_sync' && target === 'pi' && spec && (spec.kind === 'pi_slot' || spec.kind === 'pi_custom' || spec.kind === 'pi_subscription')) {
    return [
      change('pi', 'provider', spec.slot),
      secretChange('pi', spec.kind === 'pi_subscription' ? 'auth' : 'apiKey'),
    ];
  }
  if (expect.route === 'config_sync' && target === 'dsh') {
    return [
      change('dsh', 'provider', 'deepseek-official'),
      change('dsh', 'apiKeyEnv', 'DEEPSEEK_API_KEY'),
      secretChange('dsh', 'apiKey'),
    ];
  }
  if (expect.route === 'local_bridge' && target === 'codex') {
    return [
      change(
        'codex',
        'provider',
        spec?.kind === 'local_bridge' && spec.codexLabel ? spec.codexLabel : 'AgentHub 本机路由',
      ),
      change('codex', 'baseUrl', 'http://127.0.0.1:<本机端口>/v1'),
    ];
  }
  if (expect.route === 'local_bridge' && target === 'claude') {
    return [
      change('claude', 'ANTHROPIC_BASE_URL', 'http://127.0.0.1:<本机端口>'),
      secretChange('claude', 'ANTHROPIC_AUTH_TOKEN'),
    ];
  }
  if (expect.route === 'local_bridge' && target === 'grok') {
    return [
      change('grok', 'baseUrl', 'http://127.0.0.1:<本机端口>/v1'),
      change('grok', 'apiBackend', 'responses'),
      secretChange('grok', 'apiKey'),
    ];
  }
  if (expect.route === 'local_bridge' && target === 'kimi') {
    return [
      change('kimi', 'baseUrl', 'http://127.0.0.1:<本机端口>/v1'),
      secretChange('kimi', 'apiKey'),
    ];
  }
  if (expect.route === 'local_bridge' && target === 'dsh') {
    return [
      change('dsh', 'baseURL', 'http://127.0.0.1:<本机端口>'),
      secretChange('dsh', 'apiKey'),
    ];
  }
  return [];
}

export function planFromExpect(
  expect: GoldenExpect,
  request: AdapterRouteRequest,
): AdapterApplyPlan {
  const analysis = analysisFromExpect(expect, request);
  return {
    analysis,
    targetAgentId: request.targetAgentId,
    canApply: expect.canApply,
    maturity: maturityFromExpect(expect),
    reusePath: expect.reusePath,
    reason: expect.reason,
    serviceImpact: expect.route === 'local_bridge' ? 'requires_local_bridge' : 'none',
    changes: changesFromExpect(expect, request),
  };
}

export function unsupportedPlan(request: AdapterRouteRequest, reason?: string): AdapterApplyPlan {
  const analysis = unsupportedAnalysis(reason);
  return {
    analysis,
    targetAgentId: request.targetAgentId,
    canApply: false,
    maturity: 'none',
    reusePath: 'none',
    reason: analysis.reason,
    serviceImpact: 'none',
    changes: [],
  };
}

function safeSourceId(sourceId: string): string {
  return sourceId.replace(/[^a-zA-Z0-9_-]/g, '-').slice(0, 40) || 'source';
}

export function materializeFromPlan(
  request: AdapterApplyRequest,
  plan: AdapterApplyPlan,
  existing: AdapterProfile | undefined,
  now: string,
): { profile: AdapterProfile; provider: Provider } {
  const safeId = safeSourceId(request.sourceId);
  const spec = getMaterializeSpec(plan.analysis.ruleId);
  const port = 32123;

  if (plan.analysis.route === 'local_bridge') {
    const prefix = spec?.prefix ?? 'bridge';
    const display = spec?.display ?? request.targetAgentId;
    const mode: AdapterProfileMode = spec?.kind === 'local_bridge' ? spec.mode : 'api';
    const profile: AdapterProfile = existing ?? {
      id: `adapter-${prefix}-${request.targetAgentId}-bridge-${safeId}`,
      name: `${display} → ${targetLabel(request.targetAgentId)} 本机路由 (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'local_bridge',
      mode,
      status: 'active',
      ruleId: plan.analysis.ruleId ?? '',
      ruleVersion: '1',
      generatedProviderId: `${request.targetAgentId}-${prefix}-bridge-${safeId}`,
      localPort: port,
      autoStart: false,
      createdAt: now,
      updatedAt: now,
    };
    return { profile, provider: localBridgeProvider(profile, request.targetAgentId, port) };
  }

  if (spec?.kind === 'claude_native') {
    const profile: AdapterProfile = existing ?? {
      id: `adapter-${spec.prefix}-claude-${safeId}`,
      name: `${spec.display} → Claude (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'native_endpoint',
      mode: 'api',
      status: 'active',
      ruleId: plan.analysis.ruleId!,
      ruleVersion: '1',
      generatedProviderId: `claude-${spec.prefix}-adapter-${safeId}`,
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
            ANTHROPIC_BASE_URL: spec.baseUrl,
            ANTHROPIC_AUTH_TOKEN: CONNECTION_SECRET_MARKER,
          },
        }),
        configFormat: 'json',
        isCurrent: true,
      },
    };
  }

  if (spec?.kind === 'grok_chat') {
    const profile: AdapterProfile = existing ?? {
      id: `adapter-${spec.prefix}-${safeId}`,
      name: `${spec.display} → Grok (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: 'grok',
      route: 'native_endpoint',
      mode: 'api',
      status: 'active',
      ruleId: plan.analysis.ruleId!,
      ruleVersion: '1',
      generatedProviderId: `grok-${spec.prefix}-adapter-${safeId}`,
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
          `default = "${spec.alias}"`,
          '',
          `[model."${spec.alias}"]`,
          `model = "${spec.model}"`,
          `base_url = "${spec.baseUrl}"`,
          `api_key = "${CONNECTION_SECRET_MARKER}"`,
          'api_backend = "chat_completions"',
        ].join('\n'),
        configFormat: 'toml',
        isCurrent: true,
      },
    };
  }

  if (spec?.kind === 'codex_responses') {
    const profile: AdapterProfile = existing ?? {
      id: `adapter-${spec.prefix}-codex-${safeId}`,
      name: `${spec.display} → Codex (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'native_endpoint',
      mode: 'api',
      status: 'active',
      ruleId: plan.analysis.ruleId!,
      ruleVersion: '1',
      generatedProviderId: `codex-${spec.prefix}-adapter-${safeId}`,
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
          `model_provider = "${spec.slug}"`,
          `model = "${spec.model}"`,
          'model_reasoning_effort = "high"',
          'preferred_auth_method = "apikey"',
          '',
          `[model_providers.${spec.slug}]`,
          `name = "${spec.display}"`,
          `base_url = "${spec.baseUrl}"`,
          'wire_api = "responses"',
          `experimental_bearer_token = "${CONNECTION_SECRET_MARKER}"`,
        ].join('\n'),
        configFormat: 'toml',
        isCurrent: true,
      },
    };
  }

  if (spec?.kind === 'codex_login') {
    const profile: AdapterProfile = existing ?? {
      id: `adapter-${spec.prefix}-${safeId}`,
      name: `${spec.display} → Codex (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: 'codex',
      route: 'native_endpoint',
      mode: 'oauth',
      status: 'active',
      ruleId: plan.analysis.ruleId!,
      ruleVersion: '1',
      generatedProviderId: `codex-${spec.prefix}-adapter-${safeId}`,
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
        configText: JSON.stringify({ login: '官方登录' }),
        configFormat: 'json',
        isCurrent: true,
      },
    };
  }

  if (spec?.kind === 'dsh') {
    const profile: AdapterProfile = existing ?? {
      id: `adapter-deepseek-dsh-${safeId}`,
      name: `DeepSeek → DSH (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'config_sync',
      mode: 'api',
      status: 'active',
      ruleId: plan.analysis.ruleId ?? 'deepseek-api-to-dsh-v1',
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

  if (spec && (spec.kind === 'pi_slot' || spec.kind === 'pi_custom' || spec.kind === 'pi_subscription')) {
    const subscription = spec.kind === 'pi_subscription';
    const slot = spec.slot;
    const profile: AdapterProfile = existing ?? {
      id: `adapter-${spec.prefix}-pi-${safeId}`,
      name: `${spec.display} → Pi (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'config_sync',
      mode: subscription ? 'oauth' : 'api',
      status: 'active',
      ruleId: plan.analysis.ruleId!,
      ruleVersion: '1',
      generatedProviderId: `pi-${spec.prefix}-adapter-${safeId}`,
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
          : spec.kind === 'pi_custom'
            ? {
                models: {
                  providers: {
                    [slot]: {
                      baseUrl: spec.baseUrl,
                      api: 'openai-completions',
                      models: [{ id: spec.model }],
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

  throw new Error(`mock apply has no projection for rule ${plan.analysis.ruleId ?? '(none)'}`);
}

function localBridgeProvider(profile: AdapterProfile, target: string, port: number): Provider {
  if (target === 'claude') {
    return {
      id: profile.generatedProviderId!,
      agentId: 'claude',
      name: profile.name,
      preset: 'anthropic',
      configText: JSON.stringify({
        env: {
          ANTHROPIC_BASE_URL: `http://127.0.0.1:${profile.localPort ?? port}`,
          ANTHROPIC_AUTH_TOKEN: CONNECTION_SECRET_MARKER,
        },
      }),
      configFormat: 'json',
      isCurrent: true,
    };
  }
  const loopback = target === 'dsh'
    ? `http://127.0.0.1:${profile.localPort ?? port}`
    : `http://127.0.0.1:${profile.localPort ?? port}/v1`;
  return {
    id: profile.generatedProviderId!,
    agentId: target,
    name: profile.name,
    preset: target === 'dsh' ? 'deepseek' : target === 'grok' || target === 'kimi' ? 'openai-chat' : 'openai-compatible',
    configText: JSON.stringify({ baseUrl: loopback }),
    configFormat: 'json',
    isCurrent: true,
  };
}
