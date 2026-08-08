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
          '',
          '[providers.custom]',
          'base_url = "https://your-relay.example.com/v1"',
          'api_key = "sk-xxxxxxxx"',
          '',
        ].join('\n'),
      };
    case 'grok':
      return {
        format: 'toml',
        preset: 'custom',
        text: [
          'model = "grok-code-fast-1"',
          'base_url = "https://your-relay.example.com/v1"',
          'api_key = "sk-xxxxxxxx"',
          '',
        ].join('\n'),
      };
    default:
      return {
        format: 'json',
        preset: 'custom',
        text: JSON.stringify({ env: {} }, null, 2),
      };
  }
}

/**
 * 切换供应商 / 账号时相关的 live 路径（展示用，与 core adapter 对齐）。
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
        auth: '官方登录态 / 文件型凭据（以 detect 为准）',
        extra: ['~/.claude.json（MCP / 全局）'],
        openDir: '~/.claude（或 CLAUDE_CONFIG_DIR）',
        hint: 'API/供应商写入 settings.json 的 env；官方登录态由 CLI 管理，未必在单一文件中',
      };
    case 'codex':
      return {
        config: '~/.codex/config.toml',
        auth: '~/.codex/auth.json',
        openDir: '~/.codex',
        hint: 'TOML 写入 config.toml；认证写入 auth 文件',
      };
    case 'kimi':
      return {
        config: '~/.kimi-code/config.toml（旧 ~/.kimi）',
        auth: 'credentials 目录（以 adapter 为准）',
        openDir: '~/.kimi-code 或 ~/.kimi',
        hint: '供应商/API Key 写 config.toml；OAuth 凭据在 credentials 目录',
      };
    case 'grok':
      return {
        config: '~/.grok/config.toml',
        auth: '~/.grok/auth.json',
        openDir: '~/.grok',
        hint: 'API Key 可写 config.toml；OAuth 使用 auth 文件',
      };
    case 'pi':
      return {
        config: '~/.pi/agent/settings.json',
        auth: '~/.pi/agent/auth.json',
        extra: ['~/.pi/agent/models.json'],
        openDir: '~/.pi/agent（或 PI_CODING_AGENT_DIR）',
        hint: '账号 import/switch 读写 auth 文件；供应商写回暂 fail-closed',
      };
    case 'workbuddy':
      return {
        config: '~/.workbuddy/settings.json',
        auth: '桌面登录态（不由 AgentHub 切换）',
        extra: ['~/.workbuddy/models.json', '~/.workbuddy/.mcp.json'],
        openDir: '~/.workbuddy（或 WORKBUDDY_CONFIG_DIR）',
        hint: '账号切换 unsupported；备份含 settings/models/mcp',
      };
    case 'cursor':
      return {
        config: '无稳定 provider 配置文件',
        auth: 'CURSOR_API_KEY 或 agent login',
        openDir: '~/.cursor',
        hint: '供应商/账号池 live 写回 unsupported；技能目录由 adapter 声明',
      };
    default:
      return {
        config: '（该 agent 暂无 live 供应商写回）',
        openDir: '~',
        hint: '仅保存到供应商池，不一定写入 live',
      };
  }
}
