/**
 * 【测试专用】Codex 配置**形态**样例。
 * - 不进生产入口；URL/Key 为占位
 * - live 路径形态：~/.codex/config.toml + ~/.codex/auth.json
 */

const RELAY = 'https://relay.example.com';
const RELAY_V1 = 'https://relay.example.com/v1';
const KEY = 'sk-test-sample-codex-key-abcdefghijklmnopqrstuvwxyz012345';
const KEY2 = 'sk-test-sample-codex-key-zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz';

export const CODEX_TOML_OPENAI_PROVIDER = `
model_provider = "OpenAI"
model = "gpt-5.5"
review_model = "gpt-5.5"
model_reasoning_effort = "xhigh"
disable_response_storage = true
network_access = "enabled"
windows_wsl_setup_acknowledged = true

[model_providers.OpenAI]
name = "OpenAI"
base_url = "${RELAY}"
wire_api = "responses"
supports_websockets = true
requires_openai_auth = true

[features]
responses_websockets_v2 = true
goals = true
`.trim();

export const CODEX_TOML_OPENAI_WITH_KEY_LINE = `
${CODEX_TOML_OPENAI_PROVIDER}

# auth.json
OPENAI_API_KEY=${KEY}
`.trim();

export const CODEX_AUTH_JSON = `
{
  "OPENAI_API_KEY": "${KEY}"
}
`.trim();

export const CODEX_AUTH_JSON_FRAGMENT = `
  "OPENAI_API_KEY": "${KEY}"
}
`.trim();

export const CODEX_DUAL_BLOCK = `
${CODEX_TOML_OPENAI_PROVIDER}

${CODEX_AUTH_JSON}
`.trim();

export const CODEX_TOML_ENV_KEY_PROVIDER = `
model_provider = "sub2api_grok"
model = "example-model-id"
review_model = "example-model-id"
model_reasoning_effort = "xhigh"
model_context_window = 1000000

[model_providers.sub2api_grok]
name = "Sub2API Grok"
base_url = "${RELAY_V1}"
env_key = "SUB2API_API_KEY"
wire_api = "responses"
supports_websockets = true

[features]
responses_websockets_v2 = true
`.trim();

export const CODEX_DUAL_ENV_KEY = `
${CODEX_TOML_ENV_KEY_PROVIDER}

export SUB2API_API_KEY="${KEY2}"
`.trim();

/** 兼容旧测试名 */
export const CODEX_TOML_QOOO_OPENAI = CODEX_TOML_OPENAI_PROVIDER;
export const CODEX_TOML_QOOO_OPENAI_WITH_KEY = CODEX_TOML_OPENAI_WITH_KEY_LINE;
export const CODEX_DUAL_BLOCK_QOOO = CODEX_DUAL_BLOCK;
export const CODEX_TOML_SUB2API_GROK = CODEX_TOML_ENV_KEY_PROVIDER;
export const CODEX_DUAL_SUB2API_GROK = CODEX_DUAL_ENV_KEY;

export type CodexSample = {
  id: string;
  description: string;
  text: string;
  expect: {
    baseUrl?: string;
    model?: string;
    providerSlug?: string;
    reasoningEffort?: string;
    wireApi?: string;
    apiKeyPrefix?: string;
    preserveSnippets?: string[];
    authOnly?: boolean;
  };
};

export const CODEX_SAMPLES: CodexSample[] = [
  {
    id: 'toml-openai-provider',
    description: 'config.toml OpenAI provider + features',
    text: CODEX_TOML_OPENAI_PROVIDER,
    expect: {
      baseUrl: RELAY,
      model: 'gpt-5.5',
      providerSlug: 'OpenAI',
      reasoningEffort: 'xhigh',
      wireApi: 'responses',
      preserveSnippets: [
        'review_model = "gpt-5.5"',
        'supports_websockets = true',
        'responses_websockets_v2 = true',
        'goals = true',
      ],
    },
  },
  {
    id: 'toml-openai-with-key-line',
    description: 'config.toml + OPENAI_API_KEY 行',
    text: CODEX_TOML_OPENAI_WITH_KEY_LINE,
    expect: {
      baseUrl: RELAY,
      model: 'gpt-5.5',
      providerSlug: 'OpenAI',
      reasoningEffort: 'xhigh',
      wireApi: 'responses',
      apiKeyPrefix: 'sk-',
      preserveSnippets: ['supports_websockets = true', '[model_providers.OpenAI]'],
    },
  },
  {
    id: 'auth-json',
    description: 'auth.json 完整',
    text: CODEX_AUTH_JSON,
    expect: { apiKeyPrefix: 'sk-', authOnly: true },
  },
  {
    id: 'auth-json-fragment',
    description: 'auth.json 残缺片段',
    text: CODEX_AUTH_JSON_FRAGMENT,
    expect: { apiKeyPrefix: 'sk-', authOnly: true },
  },
  {
    id: 'dual-block',
    description: 'config.toml + auth.json 双块',
    text: CODEX_DUAL_BLOCK,
    expect: {
      baseUrl: RELAY,
      model: 'gpt-5.5',
      providerSlug: 'OpenAI',
      reasoningEffort: 'xhigh',
      wireApi: 'responses',
      apiKeyPrefix: 'sk-',
      preserveSnippets: ['supports_websockets = true', 'goals = true'],
    },
  },
  {
    id: 'env-key-dual',
    description: 'env_key + export SUB2API_API_KEY',
    text: CODEX_DUAL_ENV_KEY,
    expect: {
      baseUrl: RELAY_V1,
      model: 'example-model-id',
      providerSlug: 'sub2api_grok',
      reasoningEffort: 'xhigh',
      wireApi: 'responses',
      apiKeyPrefix: 'sk-',
      preserveSnippets: [
        'env_key = "SUB2API_API_KEY"',
        'name = "Sub2API Grok"',
        'model_context_window = 1000000',
      ],
    },
  },
];
