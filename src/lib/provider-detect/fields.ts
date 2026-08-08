/**
 * 供应商表单字段 ↔ 配置原文（JSON/TOML）读写。
 * 与 detectors 同属 provider-detect，供编辑弹窗统一调用。
 */
import type { AgentId } from '@/lib/types';
import {
  CLAUDE_MODEL_ROLE_ENV,
  EMPTY_FORM_VARS,
  REDACTED_MARKER,
  type FormFieldKey,
  type ProviderFormVars,
} from './types';

export { EMPTY_FORM_VARS, REDACTED_MARKER, CLAUDE_MODEL_ROLE_ENV };
export type { FormFieldKey, ProviderFormVars };

/** 脱敏 / 占位 / 后端 redaction marker */
export function looksRedactedOrPlaceholder(v: string): boolean {
  if (!v) return false;
  if (v === REDACTED_MARKER) return true;
  return /[•…]|\*{3,}|xxxx|XXXX|xxxxxxxx|your-api-key|sk-x{4,}/i.test(v);
}

function sanitizeSecretForForm(v: string): string {
  return looksRedactedOrPlaceholder(v) ? '' : v;
}

function tomlGet(text: string, key: string): string {
  const re = new RegExp(`^\\s*${escapeRegExp(key)}\\s*=\\s*"([^"]*)"`, 'm');
  return text.match(re)?.[1] ?? '';
}

function tomlSet(text: string, key: string, value: string): string {
  const re = new RegExp(`^(\\s*${escapeRegExp(key)}\\s*=\\s*)"[^"]*"`, 'm');
  if (re.test(text)) return text.replace(re, `$1"${value}"`);
  // 插到第一个 [section] 之前；没有 section 则追加文末
  const sectionIdx = text.search(/^\[/m);
  const line = `${key} = "${value}"\n`;
  if (sectionIdx >= 0) {
    return text.slice(0, sectionIdx) + line + text.slice(sectionIdx);
  }
  const pad = text.endsWith('\n') || text.length === 0 ? '' : '\n';
  return `${text}${pad}${line}`;
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** 取第一个 [model_providers.xxx] / [providers.xxx] 的 slug */
function firstTableSlug(text: string, prefix: 'model_providers' | 'providers'): string {
  const re = new RegExp(`^\\[(${prefix})\\.([^\\]]+)\\]`, 'm');
  const m = text.match(re);
  return m?.[2]?.trim() || 'custom';
}

/** 在指定 table 内读写 key（简单行扫描，不引入 TOML 解析器） */
function tomlTableGet(text: string, table: string, key: string): string {
  const header = `[${table}]`;
  const start = text.indexOf(header);
  if (start < 0) return '';
  const after = text.slice(start + header.length);
  const next = after.search(/^\s*\[/m);
  const body = next < 0 ? after : after.slice(0, next);
  return tomlGet(body, key);
}

function tomlTableSet(text: string, table: string, key: string, value: string): string {
  const header = `[${table}]`;
  const start = text.indexOf(header);
  if (start < 0) {
    const pad = text.endsWith('\n') || text.length === 0 ? '' : '\n';
    return `${text}${pad}${header}\n${key} = "${value}"\n`;
  }
  const bodyStart = start + header.length;
  const after = text.slice(bodyStart);
  const nextRel = after.search(/^\s*\[/m);
  const body = nextRel < 0 ? after : after.slice(0, nextRel);
  const rest = nextRel < 0 ? '' : after.slice(nextRel);
  const keyRe = new RegExp(`^(\\s*${escapeRegExp(key)}\\s*=\\s*)"[^"]*"`, 'm');
  let newBody: string;
  if (keyRe.test(body)) {
    newBody = body.replace(keyRe, `$1"${value}"`);
  } else {
    const pad = body.endsWith('\n') || body.length === 0 ? '' : '\n';
    newBody = `${body}${pad}${key} = "${value}"\n`;
  }
  return text.slice(0, bodyStart) + newBody + rest;
}

function isOpaqueRedactedToml(configText: string): boolean {
  return configText.trim() === REDACTED_MARKER;
}

/** 从配置原文提取表单变量 */
export function extractFormVars(
  agentId: AgentId,
  configText: string,
  format: 'json' | 'toml',
): ProviderFormVars {
  if (format === 'json' || agentId === 'claude') {
    if (configText.trim() === REDACTED_MARKER) {
      return { ...EMPTY_FORM_VARS };
    }
    try {
      const root = JSON.parse(configText || '{}') as {
        env?: Record<string, unknown>;
        model?: unknown;
      };
      const env = root.env && typeof root.env === 'object' ? root.env : {};
      const token = String(env.ANTHROPIC_AUTH_TOKEN ?? '');
      const apiKey = String(env.ANTHROPIC_API_KEY ?? '');
      const authEnv: ProviderFormVars['claudeAuthEnv'] = token
        ? 'ANTHROPIC_AUTH_TOKEN'
        : apiKey
          ? 'ANTHROPIC_API_KEY'
          : 'ANTHROPIC_AUTH_TOKEN';
      const rawKey = token || apiKey;
      const model =
        (typeof root.model === 'string' && root.model) ||
        String(env.ANTHROPIC_MODEL ?? '') ||
        '';
      return {
        ...EMPTY_FORM_VARS,
        baseUrl: String(env.ANTHROPIC_BASE_URL ?? ''),
        apiKey: sanitizeSecretForForm(rawKey),
        model,
        modelOpus: String(env.ANTHROPIC_DEFAULT_OPUS_MODEL ?? ''),
        modelSonnet: String(env.ANTHROPIC_DEFAULT_SONNET_MODEL ?? ''),
        modelHaiku: String(env.ANTHROPIC_DEFAULT_HAIKU_MODEL ?? ''),
        modelFable: String(env.ANTHROPIC_DEFAULT_FABLE_MODEL ?? ''),
        modelSubagent: String(env.CLAUDE_CODE_SUBAGENT_MODEL ?? ''),
        claudeAuthEnv: authEnv,
      };
    } catch {
      return { ...EMPTY_FORM_VARS };
    }
  }

  // TOML 全文脱敏时无法解析字段（后端 fail-closed）
  if (isOpaqueRedactedToml(configText)) {
    return { ...EMPTY_FORM_VARS, providerSlug: 'custom' };
  }

  if (agentId === 'codex') {
    // 优先顶层 model_provider，否则第一个 [model_providers.xxx]
    const topSlug = tomlGet(configText, 'model_provider');
    const slug = topSlug || firstTableSlug(configText, 'model_providers');
    const table = `model_providers.${slug}`;
    // API Key 走 settings.auth.OPENAI_API_KEY（见 Provider.authApiKey），不进 TOML
    return {
      ...EMPTY_FORM_VARS,
      model: tomlGet(configText, 'model'),
      baseUrl: tomlTableGet(configText, table, 'base_url'),
      apiKey: '',
      reasoningEffort: tomlGet(configText, 'model_reasoning_effort'),
      wireApi: tomlTableGet(configText, table, 'wire_api') || 'responses',
      providerSlug: slug || 'custom',
    };
  }

  if (agentId === 'kimi') {
    const slug = firstTableSlug(configText, 'providers');
    const table = `providers.${slug}`;
    const rawKey =
      tomlTableGet(configText, table, 'api_key') || tomlGet(configText, 'api_key');
    return {
      ...EMPTY_FORM_VARS,
      model: tomlGet(configText, 'default_model'),
      baseUrl: tomlTableGet(configText, table, 'base_url') || tomlGet(configText, 'base_url'),
      apiKey: sanitizeSecretForForm(rawKey),
      providerSlug: slug || 'custom',
    };
  }

  // grok / 其它
  return {
    ...EMPTY_FORM_VARS,
    model: tomlGet(configText, 'model'),
    baseUrl: tomlGet(configText, 'base_url'),
    apiKey: sanitizeSecretForForm(tomlGet(configText, 'api_key')),
  };
}

/**
 * 把表单变量写回配置原文。
 * - apiKey 为空：写 `"***"`（JSON 密钥字段）以触发 merge 保留
 * - TOML 全文若本就为 `***` 且无有效编辑：原样返回
 */
export function applyFormVars(
  agentId: AgentId,
  configText: string,
  format: 'json' | 'toml',
  vars: ProviderFormVars,
  opts?: { extraEnv?: Record<string, string> },
): string {
  if (format === 'json' || agentId === 'claude') {
    let root: Record<string, unknown>;
    try {
      const parsed = JSON.parse(
        configText.trim() === REDACTED_MARKER ? '{}' : configText || '{}',
      ) as unknown;
      root =
        parsed && typeof parsed === 'object' && !Array.isArray(parsed)
          ? { ...(parsed as Record<string, unknown>) }
          : {};
    } catch {
      root = {};
    }
    const envRaw = root.env;
    const env: Record<string, unknown> =
      envRaw && typeof envRaw === 'object' && !Array.isArray(envRaw)
        ? { ...(envRaw as Record<string, unknown>) }
        : {};

    if (vars.baseUrl.trim()) env.ANTHROPIC_BASE_URL = vars.baseUrl.trim();
    else delete env.ANTHROPIC_BASE_URL;

    // 只保留选定 auth 字段
    const other =
      vars.claudeAuthEnv === 'ANTHROPIC_AUTH_TOKEN'
        ? 'ANTHROPIC_API_KEY'
        : 'ANTHROPIC_AUTH_TOKEN';
    delete env[other];
    if (vars.apiKey.trim()) {
      env[vars.claudeAuthEnv] = vars.apiKey.trim();
    } else if (env[vars.claudeAuthEnv] != null || configText.includes('ANTHROPIC_')) {
      // 留空 = 保留：回写 redaction marker
      env[vars.claudeAuthEnv] = REDACTED_MARKER;
    }

    if (vars.model.trim()) {
      root.model = vars.model.trim();
      env.ANTHROPIC_MODEL = vars.model.trim();
    } else {
      delete root.model;
      delete env.ANTHROPIC_MODEL;
    }

    // 模型位：opus / sonnet / haiku / fable / subagent（自定义 id 均可）
    const roleForm: Record<keyof typeof CLAUDE_MODEL_ROLE_ENV, string> = {
      opus: vars.modelOpus,
      sonnet: vars.modelSonnet,
      haiku: vars.modelHaiku,
      fable: vars.modelFable,
      subagent: vars.modelSubagent,
    };
    for (const [role, envKey] of Object.entries(CLAUDE_MODEL_ROLE_ENV) as [
      keyof typeof CLAUDE_MODEL_ROLE_ENV,
      string,
    ][]) {
      const v = roleForm[role]?.trim() ?? '';
      if (v) env[envKey] = v;
      else delete env[envKey];
    }

    // Claude Code 附加 env（flags 等）；模型位已由表单接管，避免 extraEnv 覆盖清空
    const roleEnvKeys = new Set<string>([
      ...Object.values(CLAUDE_MODEL_ROLE_ENV),
      'ANTHROPIC_MODEL',
    ]);
    if (opts?.extraEnv) {
      for (const [k, v] of Object.entries(opts.extraEnv)) {
        if (roleEnvKeys.has(k)) {
          // 仅当表单对应位为空时才用 extraEnv 填
          const formHas =
            (k === 'ANTHROPIC_MODEL' && vars.model.trim()) ||
            (k === CLAUDE_MODEL_ROLE_ENV.opus && vars.modelOpus.trim()) ||
            (k === CLAUDE_MODEL_ROLE_ENV.sonnet && vars.modelSonnet.trim()) ||
            (k === CLAUDE_MODEL_ROLE_ENV.haiku && vars.modelHaiku.trim()) ||
            (k === CLAUDE_MODEL_ROLE_ENV.fable && vars.modelFable.trim()) ||
            (k === CLAUDE_MODEL_ROLE_ENV.subagent && vars.modelSubagent.trim());
          if (formHas) continue;
        }
        if (v !== undefined && v !== '') env[k] = v;
      }
    }

    root.env = env;
    return JSON.stringify(root, null, 2);
  }

  // TOML 完全脱敏且用户未填任何变量：保持 *** 让后端 merge 整段 content
  const touched =
    vars.baseUrl.trim() ||
    vars.apiKey.trim() ||
    vars.model.trim() ||
    vars.reasoningEffort.trim() ||
    vars.wireApi.trim();
  if (isOpaqueRedactedToml(configText) && !touched) {
    return REDACTED_MARKER;
  }

  // 从脱敏正文开始编辑时，用最小模板起笔
  let text =
    isOpaqueRedactedToml(configText) || !configText.trim()
      ? defaultTomlScaffold(agentId, vars)
      : configText;

  if (agentId === 'codex') {
    const slug = (vars.providerSlug || 'custom').trim() || 'custom';
    const table = `model_providers.${slug}`;
    if (vars.model.trim()) text = tomlSet(text, 'model', vars.model.trim());
    text = tomlSet(text, 'model_provider', slug);
    if (vars.reasoningEffort.trim()) {
      text = tomlSet(text, 'model_reasoning_effort', vars.reasoningEffort.trim());
    }
    // 仅当 name 缺失时补默认；勿覆盖 "Sub2API Grok" 等展示名
    if (!tomlTableGet(text, table, 'name')) {
      text = tomlTableSet(text, table, 'name', slug);
    }
    if (vars.baseUrl.trim()) {
      text = tomlTableSet(text, table, 'base_url', vars.baseUrl.trim());
    }
    if (vars.wireApi.trim()) {
      text = tomlTableSet(text, table, 'wire_api', vars.wireApi.trim());
    } else if (!tomlTableGet(text, table, 'wire_api')) {
      text = tomlTableSet(text, table, 'wire_api', 'responses');
    }
    // Codex API Key → settings_config.auth（Provider.authApiKey），不写进 TOML
    return text;
  }

  if (agentId === 'kimi') {
    const slug = (vars.providerSlug || 'custom').trim() || 'custom';
    const table = `providers.${slug}`;
    if (vars.model.trim()) text = tomlSet(text, 'default_model', vars.model.trim());
    if (vars.baseUrl.trim()) {
      text = tomlTableSet(text, table, 'base_url', vars.baseUrl.trim());
    }
    if (vars.apiKey.trim()) {
      text = tomlTableSet(text, table, 'api_key', vars.apiKey.trim());
    } else if (tomlTableGet(text, table, 'api_key')) {
      text = tomlTableSet(text, table, 'api_key', REDACTED_MARKER);
    }
    return text;
  }

  // grok 等顶层字段
  if (vars.model.trim()) text = tomlSet(text, 'model', vars.model.trim());
  if (vars.baseUrl.trim()) text = tomlSet(text, 'base_url', vars.baseUrl.trim());
  if (vars.apiKey.trim()) text = tomlSet(text, 'api_key', vars.apiKey.trim());
  else if (tomlGet(text, 'api_key')) text = tomlSet(text, 'api_key', REDACTED_MARKER);
  return text;
}

function defaultTomlScaffold(agentId: AgentId, vars: ProviderFormVars): string {
  const slug = (vars.providerSlug || 'custom').trim() || 'custom';
  if (agentId === 'codex') {
    return [
      `model_provider = "${slug}"`,
      `model = "${vars.model.trim() || 'gpt-5.1-codex'}"`,
      'model_reasoning_effort = "high"',
      'disable_response_storage = true',
      'preferred_auth_method = "apikey"',
      `[model_providers.${slug}]`,
      `name = "${slug}"`,
      `base_url = "${vars.baseUrl.trim() || 'https://your-relay.example.com/v1'}"`,
      'wire_api = "responses"',
      '',
    ].join('\n');
  }
  if (agentId === 'kimi') {
    return [
      `default_model = "${vars.model.trim() || 'kimi-k2'}"`,
      `[providers.${slug}]`,
      `base_url = "${vars.baseUrl.trim() || 'https://your-relay.example.com/v1'}"`,
      'api_key = "sk-xxxxxxxx"',
      '',
    ].join('\n');
  }
  return [
    `model = "${vars.model.trim() || 'grok-code-fast-1'}"`,
    `base_url = "${vars.baseUrl.trim() || 'https://your-relay.example.com/v1'}"`,
    'api_key = "sk-xxxxxxxx"',
    '',
  ].join('\n');
}

/**
 * 弹窗主字段显隐。用户配置场景：默认始终展示 URL / Key / Model，
 * 不再依赖「预设 id」切换显隐。
 */
export function formFieldVisibility(
  agentId: AgentId,
  _presetId = 'custom',
): Record<FormFieldKey, boolean> {
  const isClaude = agentId === 'claude';
  return {
    baseUrl: true,
    apiKey: true,
    model: true,
    modelOpus: isClaude,
    modelSonnet: isClaude,
    modelHaiku: isClaude,
    modelFable: isClaude,
    modelSubagent: isClaude,
    claudeAuthEnv: isClaude,
    reasoningEffort: agentId === 'codex',
    wireApi: agentId === 'codex',
  };
}

/** 字段中文标签 */
export const FORM_FIELD_LABELS: Record<FormFieldKey, string> = {
  baseUrl: 'Endpoint URL',
  apiKey: 'API Key',
  model: '主模型 (ANTHROPIC_MODEL)',
  modelOpus: 'Opus (ANTHROPIC_DEFAULT_OPUS_MODEL)',
  modelSonnet: 'Sonnet (ANTHROPIC_DEFAULT_SONNET_MODEL)',
  modelHaiku: 'Haiku (ANTHROPIC_DEFAULT_HAIKU_MODEL)',
  modelFable: 'Fable (ANTHROPIC_DEFAULT_FABLE_MODEL)',
  modelSubagent: 'Subagent (CLAUDE_CODE_SUBAGENT_MODEL)',
  claudeAuthEnv: 'Auth 字段',
  reasoningEffort: 'Reasoning effort',
  wireApi: 'Wire API',
};
