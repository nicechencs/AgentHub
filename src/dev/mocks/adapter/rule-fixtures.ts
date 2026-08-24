/**
 * mock 投影表：ruleId → 物化 / 动作 / endpoint。
 *
 * 不是规则真源。真源是 core 的 `ADAPTER_CAPABILITY_MATRIX` /
 * `AdapterRouteService` / `AdapterApplyService`。
 * 仅服务 `dev:mock` 与测试；不得打进生产 build。
 *
 * 覆盖重复 reshape 臂（同一模式、不同 URL/ruleId）。
 * local_bridge、subscription 互转拒绝、关闭 cell 等控制流仍留在 analyze/apply/plan。
 */
import type {
  AdapterAction,
  AdapterEvidence,
  AdapterRouteAnalysis,
} from '@/lib/backend/contracts/adapter';
import {
  DEEPSEEK_CLAUDE_BASE_URL,
  DEEPSEEK_CLAUDE_RULE_ID,
  DEEPSEEK_CODEX_BASE_URL,
  DEEPSEEK_CODEX_PROVIDER_SLUG,
  DEEPSEEK_CODEX_RULE_ID,
  DEEPSEEK_PI_BASE_URL,
  GLM_CLAUDE_BASE_URL,
  GLM_CLAUDE_RULE_ID,
  GLM_CODEX_BASE_URL,
  GLM_CODEX_PROVIDER_SLUG,
  GLM_CODEX_RULE_ID,
  GLM_PI_BASE_URL,
  KIMI_CLAUDE_BASE_URL,
  KIMI_GROK_BASE_URL,
  KIMI_GROK_RULE_ID,
  action,
  evidence,
  secretAction,
  type RouteSourceLabel,
} from './types';

export type MockMaterializeSpec =
  | {
      kind: 'claude_native';
      prefix: string;
      display: string;
      baseUrl: string;
    }
  | {
      kind: 'grok_chat';
      prefix: string;
      display: string;
      alias: string;
      model: string;
      baseUrl: string;
    }
  | {
      kind: 'codex_responses';
      prefix: string;
      display: string;
      slug: string;
      model: string;
      baseUrl: string;
    }
  | {
      kind: 'pi_slot';
      prefix: string;
      display: string;
      slot: string;
    }
  | {
      kind: 'pi_custom';
      prefix: string;
      display: string;
      slot: string;
      baseUrl: string;
      model: string;
    }
  | {
      kind: 'pi_subscription';
      prefix: string;
      display: string;
      slot: string;
    }
  | {
      kind: 'dsh';
    };

export interface MockRuleFixture {
  ruleId: string;
  source: RouteSourceLabel;
  targetAgentId: string;
  route: Exclude<AdapterRouteAnalysis['route'], 'unsupported'>;
  support: Exclude<AdapterRouteAnalysis['support'], 'unsupported'>;
  reason: string;
  /** When true, use the shared compatibility evidence list from analyze(). */
  evidence: 'compatibility' | ReadonlyArray<{ label: string; url: string }>;
  limitations: readonly string[];
  /** Build actions with shared helpers so wording stays identical to prior branches. */
  buildActions: () => AdapterAction[];
  materialize: MockMaterializeSpec;
}

const COMPAT_LIMIT_SECRET =
  '自动生成的配置只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。';

const CLAUDE_LIMIT_SWITCH =
  '应用后会切换当前 Claude Connection；请确认无其他进行中的配置写入。';

const CLAUDE_EXPERIMENTAL_LIMIT =
  '实验性：官方 Anthropic 兼容入口；部分扩展字段可能被忽略或不支持。';

const PI_SLOT_LIMITS = [
  '将写入 Pi models.json 对应 provider 位置；凭据只引用已保存的 Connection，不会读取或显示明文 Key。',
  '应用后会切换当前 Pi Connection。',
] as const;

const PI_DOCS = {
  label: 'Pi custom provider and model configuration',
  url: 'https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md',
} as const;

/** Reshape fixtures keyed for (source, target) lookup. */
export const MOCK_RULE_FIXTURES: readonly MockRuleFixture[] = [
  // —— Claude native_endpoint (Kimi / GLM / DeepSeek) ——
  {
    ruleId: 'kimi-membership-to-claude-v1',
    source: 'kimi_membership',
    targetAgentId: 'claude',
    route: 'native_endpoint',
    support: 'stable',
    reason: '用这份 Kimi Code 会员接到 Claude，只改地址和模型。',
    evidence: [
      {
        label: 'Kimi Code: Claude Code integration',
        url: 'https://www.kimi.com/code/docs/en/third-party-tools/claude-code.html',
      },
    ],
    limitations: [
      '将写入 Claude 的 base URL 与凭据引用标记；不会在预览中传输明文 Key。',
      CLAUDE_LIMIT_SWITCH,
    ],
    buildActions: () => [
      action('set_config', 'Claude Code', '设置 Kimi Code 官方 Anthropic-compatible Base URL。', KIMI_CLAUDE_BASE_URL),
      action('set_env', 'Claude Code', '使用 Claude Code 的认证环境变量名。', 'ANTHROPIC_AUTH_TOKEN'),
      secretAction('Claude Code', '从已选 Connection 引用 API Key；不会读取或显示它。'),
    ],
    materialize: {
      kind: 'claude_native',
      prefix: 'kimi',
      display: 'Kimi',
      baseUrl: KIMI_CLAUDE_BASE_URL,
    },
  },
  {
    ruleId: GLM_CLAUDE_RULE_ID,
    source: 'glm_coding_plan',
    targetAgentId: 'claude',
    route: 'native_endpoint',
    support: 'experimental',
    reason: '用这份 GLM Coding Plan 接到 Claude，只改地址和模型。',
    evidence: [
      {
        label: 'GLM Coding Plan 接入工具与双协议端点',
        url: 'https://docs.bigmodel.cn/cn/coding-plan/tool/others',
      },
    ],
    limitations: [
      '将写入 Claude 的 GLM Coding Plan Anthropic 兼容 Base URL 与凭据引用标记；不会在预览中传输明文 Key。',
      CLAUDE_LIMIT_SWITCH,
      CLAUDE_EXPERIMENTAL_LIMIT,
    ],
    buildActions: () => [
      action('set_config', 'Claude Code', '设置 GLM Coding Plan 官方 Anthropic-compatible Base URL。', GLM_CLAUDE_BASE_URL),
      action('set_env', 'Claude Code', '使用 Claude Code 的认证环境变量名。', 'ANTHROPIC_AUTH_TOKEN'),
      secretAction('Claude Code', '从已选 Connection 引用 API Key；不会读取或显示它。'),
    ],
    materialize: {
      kind: 'claude_native',
      prefix: 'glm',
      display: 'GLM',
      baseUrl: GLM_CLAUDE_BASE_URL,
    },
  },
  {
    ruleId: DEEPSEEK_CLAUDE_RULE_ID,
    source: 'deepseek_api',
    targetAgentId: 'claude',
    route: 'native_endpoint',
    support: 'experimental',
    reason: '用这份 DeepSeek API 接到 Claude，只改地址和模型。',
    evidence: [
      {
        label: 'DeepSeek 接入 Claude Code',
        url: 'https://api-docs.deepseek.com/quick_start/agent_integrations/claude_code/',
      },
    ],
    limitations: [
      '将写入 Claude 的 DeepSeek Anthropic 兼容 Base URL 与凭据引用标记；不会在预览中传输明文 Key。',
      CLAUDE_LIMIT_SWITCH,
      CLAUDE_EXPERIMENTAL_LIMIT,
    ],
    buildActions: () => [
      action('set_config', 'Claude Code', '设置 DeepSeek 官方 Anthropic-compatible Base URL。', DEEPSEEK_CLAUDE_BASE_URL),
      action('set_env', 'Claude Code', '使用 Claude Code 的认证环境变量名。', 'ANTHROPIC_AUTH_TOKEN'),
      secretAction('Claude Code', '从已选 Connection 引用 API Key；不会读取或显示它。'),
    ],
    materialize: {
      kind: 'claude_native',
      prefix: 'deepseek',
      display: 'DeepSeek',
      baseUrl: DEEPSEEK_CLAUDE_BASE_URL,
    },
  },

  // —— Grok native_endpoint (Kimi / OpenAI) ——
  {
    ruleId: KIMI_GROK_RULE_ID,
    source: 'kimi_membership',
    targetAgentId: 'grok',
    route: 'native_endpoint',
    support: 'experimental',
    reason: '用这份 Kimi Code 会员接到 Grok，只改地址和模型。',
    evidence: 'compatibility',
    limitations: [
      '只修改 Grok ~/.grok/config.toml 的官方 TOML provider；不会启动本机路由。',
      COMPAT_LIMIT_SECRET,
    ],
    buildActions: () => [
      action('set_config', 'Grok', '写入 Grok 官方 OpenAI Chat Completions TOML。', KIMI_GROK_BASE_URL),
      action('set_config', 'Grok', '使用 Grok Chat Completions 与 kimi-k2.5。', 'api_backend=chat_completions; model=kimi-k2.5'),
      secretAction('Grok', '从已选 Connection 引用 API Key；不会读取或显示它。'),
    ],
    materialize: {
      kind: 'grok_chat',
      prefix: 'kimi-grok',
      display: 'Kimi',
      alias: 'agenthub_kimi',
      model: 'kimi-k2.5',
      baseUrl: KIMI_GROK_BASE_URL,
    },
  },

  // —— Codex native_endpoint (GLM / DeepSeek) ——
  {
    ruleId: GLM_CODEX_RULE_ID,
    source: 'glm_coding_plan',
    targetAgentId: 'codex',
    route: 'native_endpoint',
    support: 'experimental',
    reason: '用这份 GLM 会员接到 Codex，只改地址和模型。',
    evidence: [
      {
        label: 'GLM Coding Plan Codex Responses integration',
        url: 'https://docs.bigmodel.cn/cn/coding-plan/tool/codex',
      },
    ],
    limitations: [
      '将把 Codex 配置为官方 Responses 端点；不会开本机转发。',
      COMPAT_LIMIT_SECRET,
      '当前未写入官方 ~/.codex/models.json；使用默认 model 与显式 Provider 配置。',
    ],
    buildActions: () => [
      action('set_config', 'Codex', 'GLM Coding Plan 官方 Responses Base URL；不会启动本机路由。', GLM_CODEX_BASE_URL),
      action('set_config', 'Codex', '使用 Codex Responses wire_api 与默认模型 glm-5.3。', 'wire_api=responses; model=glm-5.3'),
      secretAction('Codex', '从已选 Connection 引用 API Key；不会读取或显示它。'),
    ],
    materialize: {
      kind: 'codex_responses',
      prefix: 'glm',
      display: 'GLM Coding Plan',
      slug: GLM_CODEX_PROVIDER_SLUG,
      model: 'glm-5.3',
      baseUrl: GLM_CODEX_BASE_URL,
    },
  },
  {
    ruleId: DEEPSEEK_CODEX_RULE_ID,
    source: 'deepseek_api',
    targetAgentId: 'codex',
    route: 'native_endpoint',
    support: 'experimental',
    reason: '用这份 DeepSeek Key 接到 Codex，只改地址和模型。',
    evidence: [
      {
        label: 'DeepSeek API Codex Responses integration',
        url: 'https://api-docs.deepseek.com/quick_start/agent_integrations/codex/',
      },
    ],
    limitations: [
      '将把 Codex 配置为官方 Responses 端点；不会开本机转发。',
      COMPAT_LIMIT_SECRET,
      '当前未写入官方 ~/.codex/models.json；使用默认 model 与显式 Provider 配置。',
    ],
    buildActions: () => [
      action('set_config', 'Codex', 'DeepSeek API 官方 Responses Base URL；不会启动本机路由。', DEEPSEEK_CODEX_BASE_URL),
      action('set_config', 'Codex', '使用 Codex Responses wire_api 与默认模型 deepseek-v4-flash。', 'wire_api=responses; model=deepseek-v4-flash'),
      secretAction('Codex', '从已选 Connection 引用 API Key；不会读取或显示它。'),
    ],
    materialize: {
      kind: 'codex_responses',
      prefix: 'deepseek',
      display: 'DeepSeek',
      slug: DEEPSEEK_CODEX_PROVIDER_SLUG,
      model: 'deepseek-v4-flash',
      baseUrl: DEEPSEEK_CODEX_BASE_URL,
    },
  },

  // —— Pi config_sync (API / membership / custom) ——
  {
    ruleId: 'kimi-membership-to-pi-v1',
    source: 'kimi_membership',
    targetAgentId: 'pi',
    route: 'config_sync',
    support: 'stable',
    reason: '把这份 Kimi Code 会员写进 Pi 认的登录位置。',
    evidence: [
      {
        label: 'Kimi Code CLI provider configuration',
        url: 'https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/providers.html',
      },
    ],
    limitations: [...PI_SLOT_LIMITS],
    buildActions: () => [
      action('set_config', 'Pi', '选择 Pi 的 Kimi For Coding provider。', 'kimi-for-coding'),
      secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
    ],
    materialize: {
      kind: 'pi_slot',
      prefix: 'kimi',
      display: 'Kimi',
      slot: 'kimi-for-coding',
    },
  },
  {
    ruleId: 'anthropic-api-to-pi-v1',
    source: 'anthropic_api_key',
    targetAgentId: 'pi',
    route: 'config_sync',
    support: 'stable',
    reason: '把这份 Anthropic API Key 写进 Pi 认的登录位置。',
    evidence: [PI_DOCS],
    limitations: [...PI_SLOT_LIMITS],
    buildActions: () => [
      action('set_config', 'Pi', '选择 Pi 的 Anthropic provider。', 'anthropic'),
      secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
    ],
    materialize: {
      kind: 'pi_slot',
      prefix: 'anthropic',
      display: 'Anthropic',
      slot: 'anthropic',
    },
  },
  {
    ruleId: 'openai-api-to-pi-v1',
    source: 'openai_api_key',
    targetAgentId: 'pi',
    route: 'config_sync',
    support: 'stable',
    reason: '把这份 OpenAI API Key 写进 Pi 认的登录位置。',
    evidence: [PI_DOCS],
    limitations: [
      '将写入 Pi models.json 的 openai 位置与凭据引用标记；不会在预览中传输明文 Key。',
      '接上后会把自动生成的配置设成 Pi 当前在用的连接；请确认无其他进行中的配置写入。',
    ],
    buildActions: () => [
      action('set_config', 'Pi', '选择 Pi 的 OpenAI provider。', 'openai'),
      secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
    ],
    materialize: {
      kind: 'pi_slot',
      prefix: 'openai',
      display: 'OpenAI',
      slot: 'openai',
    },
  },
  {
    ruleId: 'xai-api-to-pi-v1',
    source: 'xai_api_key',
    targetAgentId: 'pi',
    route: 'config_sync',
    support: 'stable',
    reason: '把这份 xAI API Key 写进 Pi 认的登录位置。',
    evidence: [PI_DOCS],
    limitations: [
      '将写入 Pi models.json 的 xai 位置与凭据引用标记；不会在预览中传输明文 Key。',
      '接上后会把自动生成的配置设成 Pi 当前在用的连接；请确认无其他进行中的配置写入。',
    ],
    buildActions: () => [
      action('set_config', 'Pi', '选择 Pi 的 xAI provider。', 'xai'),
      secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
    ],
    materialize: {
      kind: 'pi_slot',
      prefix: 'xai',
      display: 'xAI',
      slot: 'xai',
    },
  },
  {
    ruleId: 'glm-coding-plan-to-pi-v1',
    source: 'glm_coding_plan',
    targetAgentId: 'pi',
    route: 'config_sync',
    support: 'experimental',
    reason: '把这份 GLM Coding Plan 写进 Pi 认的登录位置。',
    evidence: [PI_DOCS],
    limitations: [
      '将写入 Pi models.json 的 glm-coding-plan 自定义位置（baseUrl、api、models）与凭据引用标记；不会在预览中传输明文 Key。',
      COMPAT_LIMIT_SECRET,
    ],
    buildActions: () => [
      action('set_config', 'Pi', '写入 Pi 的 GLM Coding Plan 自定义 provider 位置。', 'glm-coding-plan'),
      secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
    ],
    materialize: {
      kind: 'pi_custom',
      prefix: 'glm',
      display: 'GLM Coding Plan',
      slot: 'glm-coding-plan',
      baseUrl: GLM_PI_BASE_URL,
      model: 'glm-4.6',
    },
  },
  {
    ruleId: 'deepseek-api-to-pi-v1',
    source: 'deepseek_api',
    targetAgentId: 'pi',
    route: 'config_sync',
    support: 'experimental',
    reason: '把这份 DeepSeek API 写进 Pi 认的登录位置。',
    evidence: [PI_DOCS],
    limitations: [
      '将写入 Pi models.json 的 deepseek 自定义位置（baseUrl、api、models）与凭据引用标记；不会在预览中传输明文 Key。',
      COMPAT_LIMIT_SECRET,
    ],
    buildActions: () => [
      action('set_config', 'Pi', '写入 Pi 的 DeepSeek 自定义 provider 位置。', 'deepseek'),
      secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
    ],
    materialize: {
      kind: 'pi_custom',
      prefix: 'deepseek',
      display: 'DeepSeek',
      slot: 'deepseek',
      baseUrl: DEEPSEEK_PI_BASE_URL,
      model: 'deepseek-chat',
    },
  },

  // —— Pi config_sync (native subscription reuse) ——
  {
    ruleId: 'claude-subscription-to-pi-v1',
    source: 'claude_subscription',
    targetAgentId: 'pi',
    route: 'config_sync',
    support: 'experimental',
    reason: '把这份 Claude 订阅写进 Pi 认的 Claude 登录。',
    evidence: 'compatibility',
    limitations: [
      '会把官方登录写进 Pi 认的位置；预览和日志不显示完整令牌。',
      '写进去之后由 Pi 自己续期；AgentHub 不会再刷一次。原来的工具和 Pi 一起续期可能互相踢下线。',
      '接上后会把自动生成的配置设成 Pi 当前在用的连接。',
    ],
    buildActions: () => [
      action('set_config', 'Pi', '选择 Pi 的订阅登录位置。', 'anthropic'),
      secretAction('Pi', '从已选 Connection 引用授权（OAuth）；不会读取或显示 token。'),
    ],
    materialize: {
      kind: 'pi_subscription',
      prefix: 'claude-oauth',
      display: 'Claude',
      slot: 'anthropic',
    },
  },
  {
    ruleId: 'codex-subscription-to-pi-v1',
    source: 'codex_subscription',
    targetAgentId: 'pi',
    route: 'config_sync',
    support: 'experimental',
    reason: '把这份 Codex / ChatGPT 订阅写进 Pi 认的 Codex 登录。',
    evidence: 'compatibility',
    limitations: [
      '会把官方登录写进 Pi 认的位置；预览和日志不显示完整令牌。',
      '写进去之后由 Pi 自己续期；AgentHub 不会再刷一次。原来的工具和 Pi 一起续期可能互相踢下线。',
      '接上后会把自动生成的配置设成 Pi 当前在用的连接。',
    ],
    buildActions: () => [
      action('set_config', 'Pi', '选择 Pi 的订阅登录位置。', 'openai-codex'),
      secretAction('Pi', '从已选 Connection 引用授权（OAuth）；不会读取或显示 token。'),
    ],
    materialize: {
      kind: 'pi_subscription',
      prefix: 'codex-oauth',
      display: 'Codex / ChatGPT',
      slot: 'openai-codex',
    },
  },
  {
    ruleId: 'codex-subscription-to-pi-v1',
    source: 'codex_subscription_oauth_other',
    targetAgentId: 'pi',
    route: 'config_sync',
    support: 'experimental',
    reason: '把这份 Codex / ChatGPT 订阅写进 Pi 认的 Codex 登录。',
    evidence: 'compatibility',
    limitations: [
      '会把官方登录写进 Pi 认的位置；预览和日志不显示完整令牌。',
      '写进去之后由 Pi 自己续期；AgentHub 不会再刷一次。原来的工具和 Pi 一起续期可能互相踢下线。',
      '接上后会把自动生成的配置设成 Pi 当前在用的连接。',
    ],
    buildActions: () => [
      action('set_config', 'Pi', '选择 Pi 的订阅登录位置。', 'openai-codex'),
      secretAction('Pi', '从已选 Connection 引用授权（OAuth）；不会读取或显示 token。'),
    ],
    materialize: {
      kind: 'pi_subscription',
      prefix: 'codex-oauth',
      display: 'Codex / ChatGPT',
      slot: 'openai-codex',
    },
  },
  {
    ruleId: 'grok-subscription-to-pi-v1',
    source: 'grok_xai_subscription',
    targetAgentId: 'pi',
    route: 'config_sync',
    support: 'experimental',
    reason: '把这份 Grok 订阅写进 Pi 认的 Grok 登录。',
    evidence: 'compatibility',
    limitations: [
      '会把官方登录写进 Pi 认的位置；预览和日志不显示完整令牌。',
      '写进去之后由 Pi 自己续期；AgentHub 不会再刷一次。原来的工具和 Pi 一起续期可能互相踢下线。',
      '接上后会把自动生成的配置设成 Pi 当前在用的连接。',
    ],
    buildActions: () => [
      action('set_config', 'Pi', '选择 Pi 的订阅登录位置。', 'xai'),
      secretAction('Pi', '从已选 Connection 引用授权（OAuth）；不会读取或显示 token。'),
    ],
    materialize: {
      kind: 'pi_subscription',
      prefix: 'grok-oauth',
      display: 'Grok / xAI',
      slot: 'xai',
    },
  },

  // —— DSH config_sync ——
  {
    ruleId: 'deepseek-api-to-dsh-v1',
    source: 'deepseek_api',
    targetAgentId: 'dsh',
    route: 'config_sync',
    support: 'stable',
    reason: '把这份 DeepSeek Key 写进 DeepSeek Harness 认的登录位置。',
    evidence: [
      {
        label: 'DeepSeek Harness LLM / credentials',
        url: 'https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/credentials',
      },
    ],
    limitations: [
      '将写入 DeepSeek Harness 的 home 级 provider 引用与凭据文件；不会把 API Key 写入 cordis.patch.yml。',
      '接上后会把自动生成的配置设成 DSH 当前在用的连接；请确认无其他进行中的配置写入。',
    ],
    buildActions: () => [
      action('set_config', 'DeepSeek Harness', '选择 DSH 的官方 DeepSeek provider。', 'deepseek-official'),
      secretAction('DeepSeek Harness', '从已选 Connection 引用 API Key；不会读取或显示它。'),
    ],
    materialize: { kind: 'dsh' },
  },
];

const bySourceTarget = new Map<string, MockRuleFixture>(
  MOCK_RULE_FIXTURES.map((fixture) => [`${fixture.source}::${fixture.targetAgentId}`, fixture]),
);

/** First fixture for a ruleId (codex oauth_other shares ruleId with codex_subscription). */
const byRuleId = new Map<string, MockRuleFixture>();
for (const fixture of MOCK_RULE_FIXTURES) {
  if (!byRuleId.has(fixture.ruleId)) {
    byRuleId.set(fixture.ruleId, fixture);
  }
}

export function findRuleFixture(
  source: RouteSourceLabel,
  targetAgentId: string,
): MockRuleFixture | undefined {
  return bySourceTarget.get(`${source}::${targetAgentId}`);
}

export function getRuleFixtureById(ruleId: string): MockRuleFixture | undefined {
  return byRuleId.get(ruleId);
}

export function analysisFromFixture(
  fixture: MockRuleFixture,
  compatibilityEvidence: AdapterEvidence[],
): AdapterRouteAnalysis {
  const evidenceItems: AdapterEvidence[] = fixture.evidence === 'compatibility'
    ? compatibilityEvidence
    : fixture.evidence.map((item) => evidence(item.label, item.url));
  return {
    route: fixture.route,
    support: fixture.support,
    reason: fixture.reason,
    actions: fixture.buildActions(),
    limitations: [...fixture.limitations],
    evidence: evidenceItems,
    ruleId: fixture.ruleId,
    gateKind: 'none',
  };
}
