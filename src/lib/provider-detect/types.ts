/**
 * 供应商配置智能识别 + 表单字段 — 公共类型。
 *
 * 扩展识别：在 `detectors.ts` 增加 Detector。
 * 扩展写回：在 `fields.ts` 调整 extract/applyFormVars。
 */

/** 与 src-tauri provider 命令 REDACTED_MARKER 一致 */
export const REDACTED_MARKER = '***';

export interface SmartDetectResult {
  baseUrl?: string;
  apiKey?: string;
  /** 可选：识别到的模型 id */
  model?: string;
  /** Codex: model_reasoning_effort */
  reasoningEffort?: string;
  /** Codex: wire_api */
  wireApi?: string;
  /** Codex: model_provider / [model_providers.slug] */
  providerSlug?: string;
  /** Codex: [model_providers.x].env_key（如 SUB2API_API_KEY） */
  envKey?: string;
  /**
   * 粘贴即为完整 config.toml 时，保留全文（去掉杂行 key）。
   * applySmartPaste 优先用此作为 configText，避免丢 review_model / features。
   */
  rawConfigText?: string;
  /**
   * 其它 env（如 CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC）。
   * Claude 写回时并入 settings.json env。
   */
  extraEnv?: Record<string, string>;
  /** Claude auth 字段名（若能从粘贴推断） */
  claudeAuthEnv?: 'ANTHROPIC_AUTH_TOKEN' | 'ANTHROPIC_API_KEY';
  /** 从 URL host 推导的建议名称 */
  suggestedName?: string;
  /** 识别到的线索，便于 UI 提示 */
  hints: string[];
  /** 命中的 detector id（调试 / 后续样本归类） */
  matchedDetectors?: string[];
}

/**
 * 一条可扩展的识别规则。
 * 新增样式：实现 extract，注册进 DETECTORS，并补单测样本。
 */
export interface ProviderConfigDetector {
  id: string;
  description: string;
  extract: (text: string) => Omit<SmartDetectResult, 'hints' | 'matchedDetectors'> | null;
}

/**
 * Claude Code 可自定义的模型位（env 键）：
 * - ANTHROPIC_MODEL：默认主模型
 * - ANTHROPIC_DEFAULT_{OPUS,SONNET,HAIKU,FABLE}_MODEL：分档模型
 * - CLAUDE_CODE_SUBAGENT_MODEL：子代理模型
 */
export const CLAUDE_MODEL_ROLE_ENV = {
  opus: 'ANTHROPIC_DEFAULT_OPUS_MODEL',
  sonnet: 'ANTHROPIC_DEFAULT_SONNET_MODEL',
  haiku: 'ANTHROPIC_DEFAULT_HAIKU_MODEL',
  fable: 'ANTHROPIC_DEFAULT_FABLE_MODEL',
  subagent: 'CLAUDE_CODE_SUBAGENT_MODEL',
} as const;

export type ClaudeModelRole = keyof typeof CLAUDE_MODEL_ROLE_ENV;

/** 编辑弹窗表单变量（写回各 agent 配置） */
export interface ProviderFormVars {
  baseUrl: string;
  apiKey: string;
  /** 主模型：Claude → ANTHROPIC_MODEL；Codex/其它 → model */
  model: string;
  /** Claude Code 分档模型（任意 id，不限于官方名） */
  modelOpus: string;
  modelSonnet: string;
  modelHaiku: string;
  modelFable: string;
  modelSubagent: string;
  /** `auto` / `200000` / `1048576` — empty means auto. */
  contextWindow: string;
  claudeAuthEnv: 'ANTHROPIC_AUTH_TOKEN' | 'ANTHROPIC_API_KEY';
  reasoningEffort: string;
  wireApi: string;
  providerSlug: string;
}

export const EMPTY_FORM_VARS: ProviderFormVars = {
  baseUrl: '',
  apiKey: '',
  model: '',
  modelOpus: '',
  modelSonnet: '',
  modelHaiku: '',
  modelFable: '',
  modelSubagent: '',
  contextWindow: '',
  claudeAuthEnv: 'ANTHROPIC_AUTH_TOKEN',
  reasoningEffort: '',
  wireApi: '',
  providerSlug: 'custom',
};

export type FormFieldKey =
  | 'baseUrl'
  | 'apiKey'
  | 'model'
  | 'modelOpus'
  | 'modelSonnet'
  | 'modelHaiku'
  | 'modelFable'
  | 'modelSubagent'
  | 'contextWindow'
  | 'claudeAuthEnv'
  | 'reasoningEffort'
  | 'wireApi'
  | 'providerSlug';

/** 粘贴 → 识别 → 合并进表单 + 配置正文 的一站式结果 */
export interface SmartPasteApplyResult {
  detect: SmartDetectResult;
  vars: ProviderFormVars;
  configText: string;
  configFormat: 'json' | 'toml';
  /** 建议显示名（可空） */
  suggestedName?: string;
}
