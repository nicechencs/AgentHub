/**
 * 【测试专用】Claude Code 配置**形态**样例。
 * - 不进生产 bundle / 不被 `@/lib/provider-detect` 导出
 * - URL / Key 均为占位，只验证「长什么样能认出」；真实用户粘贴值任意
 */

const RELAY = 'https://relay.example.com';
const KEY_A = 'sk-test-sample-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const KEY_B = 'sk-test-sample-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
const MODEL = 'example-model-id';

/** bash / zsh export（基础） */
export const CLAUDE_CODE_BASH_EXPORT_BASIC = `
export ANTHROPIC_BASE_URL="${RELAY}"
export ANTHROPIC_AUTH_TOKEN="${KEY_A}"
export CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1
export CLAUDE_CODE_ATTRIBUTION_HEADER=0
`.trim();

/** bash / zsh export（含模型映射） */
export const CLAUDE_CODE_BASH_EXPORT_WITH_MODELS = `
export ANTHROPIC_BASE_URL="${RELAY}"
export ANTHROPIC_AUTH_TOKEN="${KEY_B}"
export ANTHROPIC_MODEL="${MODEL}"
export ANTHROPIC_DEFAULT_OPUS_MODEL="${MODEL}"
export ANTHROPIC_DEFAULT_SONNET_MODEL="${MODEL}"
export ANTHROPIC_DEFAULT_HAIKU_MODEL="${MODEL}"
export ANTHROPIC_DEFAULT_FABLE_MODEL="${MODEL}"
export CLAUDE_CODE_SUBAGENT_MODEL="${MODEL}"
export CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1"
export CLAUDE_CODE_ATTRIBUTION_HEADER="0"
`.trim();

export const CLAUDE_CODE_SETTINGS_JSON_BASIC = `
{
  "$schema": "https://json.schemastore.org/claude-code-settings.json",
  "env": {
    "ANTHROPIC_BASE_URL": "${RELAY}",
    "ANTHROPIC_AUTH_TOKEN": "${KEY_A}",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
    "CLAUDE_CODE_ATTRIBUTION_HEADER": "0"
  }
}
`.trim();

export const CLAUDE_CODE_SETTINGS_JSON_WITH_MODELS = `
{
  "$schema": "https://json.schemastore.org/claude-code-settings.json",
  "env": {
    "ANTHROPIC_BASE_URL": "${RELAY}",
    "ANTHROPIC_AUTH_TOKEN": "${KEY_B}",
    "ANTHROPIC_MODEL": "${MODEL}",
    "ANTHROPIC_DEFAULT_OPUS_MODEL": "${MODEL}",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "${MODEL}",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "${MODEL}",
    "ANTHROPIC_DEFAULT_FABLE_MODEL": "${MODEL}",
    "CLAUDE_CODE_SUBAGENT_MODEL": "${MODEL}",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
    "CLAUDE_CODE_ATTRIBUTION_HEADER": "0"
  }
}
`.trim();

export const CLAUDE_CODE_CMD_SET_BASIC = `
set ANTHROPIC_BASE_URL=${RELAY}
set ANTHROPIC_AUTH_TOKEN=${KEY_A}
set CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1
set CLAUDE_CODE_ATTRIBUTION_HEADER=0
`.trim();

export const CLAUDE_CODE_CMD_SET_WITH_MODELS = `
set ANTHROPIC_BASE_URL=${RELAY}
set ANTHROPIC_AUTH_TOKEN=${KEY_B}
set ANTHROPIC_MODEL=${MODEL}
set ANTHROPIC_DEFAULT_OPUS_MODEL=${MODEL}
set ANTHROPIC_DEFAULT_SONNET_MODEL=${MODEL}
set ANTHROPIC_DEFAULT_HAIKU_MODEL=${MODEL}
set ANTHROPIC_DEFAULT_FABLE_MODEL=${MODEL}
set CLAUDE_CODE_SUBAGENT_MODEL=${MODEL}
set CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1
set CLAUDE_CODE_ATTRIBUTION_HEADER=0
`.trim();

export const CLAUDE_CODE_POWERSHELL_ENV_BASIC = `
$env:ANTHROPIC_BASE_URL="${RELAY}"
$env:ANTHROPIC_AUTH_TOKEN="${KEY_A}"
$env:CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1
$env:CLAUDE_CODE_ATTRIBUTION_HEADER=0
`.trim();

export const CLAUDE_CODE_POWERSHELL_ENV_WITH_MODELS = `
$env:ANTHROPIC_BASE_URL="${RELAY}"
$env:ANTHROPIC_AUTH_TOKEN="${KEY_B}"
$env:ANTHROPIC_MODEL="${MODEL}"
$env:ANTHROPIC_DEFAULT_OPUS_MODEL="${MODEL}"
$env:ANTHROPIC_DEFAULT_SONNET_MODEL="${MODEL}"
$env:ANTHROPIC_DEFAULT_HAIKU_MODEL="${MODEL}"
$env:ANTHROPIC_DEFAULT_FABLE_MODEL="${MODEL}"
$env:CLAUDE_CODE_SUBAGENT_MODEL="${MODEL}"
$env:CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1"
$env:CLAUDE_CODE_ATTRIBUTION_HEADER="0"
`.trim();

export const CLAUDE_CODE_UI_DUAL_BLOCK = `
使用 API 密钥
Claude Code
macOS / Linux
Terminal
复制
${CLAUDE_CODE_BASH_EXPORT_BASIC}
~/.claude/settings.json
复制
${CLAUDE_CODE_SETTINGS_JSON_BASIC}
关闭
`.trim();

export const CLAUDE_CODE_UI_WIN_CMD_DUAL = `
使用 API 密钥
Windows CMD
Command Prompt
复制
${CLAUDE_CODE_CMD_SET_BASIC}
%USERPROFILE%\\.claude\\settings.json
复制
${CLAUDE_CODE_SETTINGS_JSON_BASIC}
关闭
`.trim();

export const CLAUDE_CODE_UI_POWERSHELL_DUAL = `
使用 API 密钥
PowerShell
复制
${CLAUDE_CODE_POWERSHELL_ENV_BASIC}
%USERPROFILE%\\.claude\\settings.json
复制
${CLAUDE_CODE_SETTINGS_JSON_BASIC}
关闭
`.trim();

/** @deprecated */
export const CLAUDE_CODE_SHELL_EXPORT_QOOO = CLAUDE_CODE_BASH_EXPORT_BASIC;
export const CLAUDE_CODE_UI_DUAL_BLOCK_QOOO = CLAUDE_CODE_UI_DUAL_BLOCK;
export const CLAUDE_CODE_UI_WIN_CMD_DUAL_QOOO = CLAUDE_CODE_UI_WIN_CMD_DUAL;
export const CLAUDE_CODE_UI_POWERSHELL_DUAL_QOOO = CLAUDE_CODE_UI_POWERSHELL_DUAL;

export type ClaudeCodeSample = {
  id: string;
  description: string;
  text: string;
  expect: {
    baseUrl: string;
    apiKeyPrefix: string;
    model?: string;
    hasExtraFlags?: boolean;
    hasModelMap?: boolean;
  };
};

export const CLAUDE_CODE_SAMPLES: ClaudeCodeSample[] = [
  {
    id: 'bash-export-basic',
    description: 'bash export 基础',
    text: CLAUDE_CODE_BASH_EXPORT_BASIC,
    expect: { baseUrl: RELAY, apiKeyPrefix: 'sk-', hasExtraFlags: true },
  },
  {
    id: 'bash-export-models',
    description: 'bash export + 模型映射',
    text: CLAUDE_CODE_BASH_EXPORT_WITH_MODELS,
    expect: {
      baseUrl: RELAY,
      apiKeyPrefix: 'sk-',
      model: MODEL,
      hasExtraFlags: true,
      hasModelMap: true,
    },
  },
  {
    id: 'settings-json-basic',
    description: 'settings.json + $schema 基础',
    text: CLAUDE_CODE_SETTINGS_JSON_BASIC,
    expect: { baseUrl: RELAY, apiKeyPrefix: 'sk-', hasExtraFlags: true },
  },
  {
    id: 'settings-json-models',
    description: 'settings.json + 模型映射',
    text: CLAUDE_CODE_SETTINGS_JSON_WITH_MODELS,
    expect: {
      baseUrl: RELAY,
      apiKeyPrefix: 'sk-',
      model: MODEL,
      hasExtraFlags: true,
      hasModelMap: true,
    },
  },
  {
    id: 'cmd-set-basic',
    description: 'Windows cmd set 基础',
    text: CLAUDE_CODE_CMD_SET_BASIC,
    expect: { baseUrl: RELAY, apiKeyPrefix: 'sk-', hasExtraFlags: true },
  },
  {
    id: 'cmd-set-models',
    description: 'Windows cmd set + 模型映射',
    text: CLAUDE_CODE_CMD_SET_WITH_MODELS,
    expect: {
      baseUrl: RELAY,
      apiKeyPrefix: 'sk-',
      model: MODEL,
      hasExtraFlags: true,
      hasModelMap: true,
    },
  },
  {
    id: 'powershell-env-basic',
    description: 'PowerShell $env: 基础',
    text: CLAUDE_CODE_POWERSHELL_ENV_BASIC,
    expect: { baseUrl: RELAY, apiKeyPrefix: 'sk-', hasExtraFlags: true },
  },
  {
    id: 'powershell-env-models',
    description: 'PowerShell $env: + 模型映射',
    text: CLAUDE_CODE_POWERSHELL_ENV_WITH_MODELS,
    expect: {
      baseUrl: RELAY,
      apiKeyPrefix: 'sk-',
      model: MODEL,
      hasExtraFlags: true,
      hasModelMap: true,
    },
  },
  {
    id: 'ui-dual-block',
    description: '弹窗双块 export + settings.json + UI',
    text: CLAUDE_CODE_UI_DUAL_BLOCK,
    expect: { baseUrl: RELAY, apiKeyPrefix: 'sk-', hasExtraFlags: true },
  },
  {
    id: 'ui-win-cmd-dual',
    description: 'Windows CMD 双块 + UI',
    text: CLAUDE_CODE_UI_WIN_CMD_DUAL,
    expect: { baseUrl: RELAY, apiKeyPrefix: 'sk-', hasExtraFlags: true },
  },
  {
    id: 'ui-powershell-dual',
    description: 'PowerShell 双块 + UI',
    text: CLAUDE_CODE_UI_POWERSHELL_DUAL,
    expect: { baseUrl: RELAY, apiKeyPrefix: 'sk-', hasExtraFlags: true },
  },
];
