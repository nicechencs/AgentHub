/**
 * 各 agent 默认空配置骨架（写回 settings_config 用，不依赖预设列表）。
 * Live 落盘路径见 {@link liveConfigPaths}。
 */
export function defaultConfigScaffold(agentId: string): {
  format: 'json' | 'toml';
  text: string;
  preset: string;
} {
  switch (agentId) {
    case 'claude':
      return {
        format: 'json',
        preset: 'custom',
        text: JSON.stringify({ env: {} }, null, 2),
      };
    case 'codex':
      return {
        format: 'toml',
        preset: 'custom',
        text: [
          'model_provider = "custom"',
          'model = "gpt-5.1-codex"',
          'model_reasoning_effort = "high"',
          'disable_response_storage = true',
          'preferred_auth_method = "apikey"',
          '',
          '[model_providers.custom]',
          'name = "custom"',
          'base_url = "https://your-relay.example.com/v1"',
          'wire_api = "responses"',
          '',
        ].join('\n'),
      };
    case 'kimi':
      return {
        format: 'toml',
        preset: 'custom',
        text: [
          'default_model = "kimi-k2"',
          'default_provider = "custom"',
          '',
          '[providers.custom]',
          'type = "openai"',
          'base_url = "https://your-relay.example.com/v1"',
          'api_key = "sk-xxxxxxxx"',
          '',
          '[models."kimi-k2"]',
          'provider = "custom"',
          'model = "kimi-k2"',
          'max_context_size = 131072',
          '',
        ].join('\n'),
      };
    case 'grok':
      return {
        format: 'toml',
        preset: 'custom',
        text: [
          '[models]',
          'default = "grok"',
          'web_search = "grok"',
          '',
          '[model."grok"]',
          'model = "grok-4.5"',
          'base_url = "https://your-relay.example.com/v1"',
          'env_key = "XAI_API_KEY"',
          'api_backend = "responses"',
          'context_window = 1000000',
          'supports_backend_search = true',
          '',
        ].join('\n'),
      };
    case 'pi':
      return {
        format: 'json',
        preset: 'custom',
        text: JSON.stringify(
          {
            providers: {
              custom: {
                baseUrl: 'https://your-relay.example.com/v1',
                api: 'openai-completions',
                apiKey: '',
                models: [
                  {
                    id: 'custom-model',
                    name: 'Custom Model',
                    input: ['text'],
                    contextWindow: 128000,
                    maxTokens: 16384,
                  },
                ],
              },
            },
          },
          null,
          2,
        ),
      };
    case 'workbuddy':
      return {
        format: 'json',
        preset: 'custom',
        text: JSON.stringify(
          {
            models: [
              {
                id: 'custom-model',
                name: 'Custom Model',
                vendor: 'custom',
                url: 'https://your-relay.example.com/v1/chat/completions',
                apiKey: '',
                maxInputTokens: 128000,
                maxOutputTokens: 8192,
                supportsToolCall: true,
              },
            ],
            availableModels: ['custom-model'],
          },
          null,
          2,
        ),
      };
    case 'cursor':
      return {
        format: 'json',
        preset: 'custom',
        text: JSON.stringify({ note: 'cursor-pool-only' }, null, 2),
      };
    case 'dsh':
      return {
        format: 'json',
        preset: 'custom',
        text: JSON.stringify(
          {
            provider: 'deepseek-official',
            model: 'deepseek-v4-flash',
            baseUrl: 'https://api.deepseek.com',
            baseURL: 'https://api.deepseek.com',
            apiKeyEnv: 'DEEPSEEK_API_KEY',
            apiKey: '',
          },
          null,
          2,
        ),
      };
    case 'zcode':
      return {
        format: 'json',
        preset: 'custom',
        text: JSON.stringify(
          {
            apiKey: '',
            baseURL: 'https://open.bigmodel.cn/api/anthropic',
            kind: 'anthropic',
            name: 'BigModel',
            providerId: 'builtin:bigmodel',
            models: ['GLM-5.3', 'GLM-5.3-Flash', 'GLM-5-Turbo'],
          },
          null,
          2,
        ),
      };
    default:
      return {
        format: 'json',
        preset: 'custom',
        text: JSON.stringify({ env: {} }, null, 2),
      };
  }
}

/** 编辑页「登录凭据」只展示真实路径；说明文字放 hint。 */
export function isLiveFilePath(value: string | undefined): value is string {
  if (!value) return false;
  const trimmed = value.trim();
  return trimmed.startsWith('~') || trimmed.startsWith('/') || /^[A-Za-z]:[\\/]/.test(trimmed);
}

/**
 * 切换服务 / 账号时相关的本机配置路径（展示用，与 core adapter 对齐）。
 * 打开目录请用 `openAgentConfigDir(agentId)`（会解析 CLAUDE_CONFIG_DIR 等覆盖）。
 * 完整读写规则以 adapter 为准；此处仅给用户「打开目录 / 备份」提示。
 */
export function liveConfigPaths(agentId: string): {
  /** 主配置文件 */
  config: string;
  /** 凭据 / 账号文件（若有） */
  auth?: string;
  /** 其它相关文件（MCP 等） */
  extra?: string[];
  /** 建议一键打开的配置目录（展示用） */
  openDir: string;
  hint: string;
} {
  switch (agentId) {
    case 'claude':
      return {
        config: '~/.claude/settings.json',
        auth: '~/.claude/.credentials.json',
        extra: ['~/.claude.json'],
        openDir: '~/.claude',
        hint: '保存后会写进 settings.json。用 Claude 官方账号登录的，写在 .credentials.json。目录也可能由 CLAUDE_CONFIG_DIR 覆盖。',
      };
    case 'codex':
      return {
        config: '~/.codex/config.toml',
        auth: '~/.codex/auth.json',
        openDir: '~/.codex',
        hint: '服务设置写在 config.toml，登录信息写在 auth.json。',
      };
    case 'kimi':
      return {
        config: '~/.kimi-code/config.toml',
        auth: '~/.kimi-code/credentials/kimi-code.json',
        openDir: '~/.kimi-code',
        hint: 'API Key 写在 config.toml。用官方登录的，凭据在 credentials/kimi-code.json。旧目录是 ~/.kimi。',
      };
    case 'grok':
      return {
        config: '~/.grok/config.toml',
        auth: '~/.grok/auth.json',
        openDir: '~/.grok',
        hint: 'API Key 可以写在 config.toml；官方登录写在 auth.json。',
      };
    case 'pi':
      return {
        config: '~/.pi/agent/settings.json',
        auth: '~/.pi/agent/auth.json',
        extra: ['~/.pi/agent/models.json'],
        openDir: '~/.pi/agent（或 PI_CODING_AGENT_DIR）',
        hint: '官方厂商的密钥写在 auth.json，自己的服务地址写在 models.json。保存到列表后，切换才会写到本机。',
      };
    case 'workbuddy':
      return {
        config: '~/.workbuddy/settings.json',
        extra: ['~/.workbuddy/models.json', '~/.workbuddy/.mcp.json'],
        openDir: '~/.workbuddy（或 WORKBUDDY_CONFIG_DIR）',
        hint: '服务设置写在 models.json。暂不支持在这里切换账号。',
      };
    case 'cursor':
      return {
        config: '无稳定 provider 配置文件',
        openDir: '~/.cursor',
        hint: '暂时不能把连接写回 Cursor 的本机配置。',
      };
    case 'dsh':
      return {
        config: '~/.dsh/cordis.patch.yml',
        auth: '~/.dsh/.credentials.yaml',
        openDir: '~/.dsh',
        hint: '服务设置写在 cordis.patch.yml，密钥写在 .credentials.yaml。切换后才会写到本机。官方 DeepSeek 用 https://api.deepseek.com，不要加 /anthropic。',
      };
    case 'zcode':
      return {
        config: '~/.zcode/v2/config.json',
        auth: '~/.zcode/v2/config.json',
        extra: ['~/.zcode/cli/config.json'],
        openDir: '~/.zcode（或 ZCODE_HOME）',
        hint: '会作为一条供应商出现在 ZCode 的模型列表里，原来的条目还在。官方智谱地址写入已有的 BigModel 或 Z.ai 槽；自定义必须带模型名单。账号登录请在 ZCode 应用内完成。',
      };
    default:
      return {
        config: '（该工具暂无本机服务配置写回）',
        openDir: '~',
        hint: '只会保存在 AgentHub 里，不一定写到本机。',
      };
  }
}
