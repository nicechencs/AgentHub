/**
 * Claude Code / settings 环境变量统一解析。
 * 支持：JSON "KEY":"v"、export KEY=、set KEY=、$env:KEY=
 */

/** 应写入 settings.json env 的 Claude 相关键（除 BASE_URL / AUTH 主字段外） */
export const CLAUDE_ENV_EXTRA_KEYS = [
  'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC',
  'CLAUDE_CODE_ATTRIBUTION_HEADER',
  'DISABLE_TELEMETRY',
  'CLAUDE_CODE_ENABLE_TELEMETRY',
  'ANTHROPIC_MODEL',
  // 模型位：opus / sonnet / haiku / fable / subagent（值任意，用户自定义）
  'ANTHROPIC_DEFAULT_OPUS_MODEL',
  'ANTHROPIC_DEFAULT_SONNET_MODEL',
  'ANTHROPIC_DEFAULT_HAIKU_MODEL',
  'ANTHROPIC_DEFAULT_FABLE_MODEL',
  'CLAUDE_CODE_SUBAGENT_MODEL',
  'CLAUDE_CODE_MAX_CONTEXT_TOKENS',
  'CLAUDE_CODE_AUTO_COMPACT_WINDOW',
] as const;

/** 模型位 env 键集合（表单可编辑） */
export const CLAUDE_MODEL_SLOT_ENV_KEYS = [
  'ANTHROPIC_MODEL',
  'ANTHROPIC_DEFAULT_OPUS_MODEL',
  'ANTHROPIC_DEFAULT_SONNET_MODEL',
  'ANTHROPIC_DEFAULT_HAIKU_MODEL',
  'ANTHROPIC_DEFAULT_FABLE_MODEL',
  'CLAUDE_CODE_SUBAGENT_MODEL',
] as const;

const AUTH_KEYS = ['ANTHROPIC_AUTH_TOKEN', 'ANTHROPIC_API_KEY'] as const;
const URL_KEYS = ['ANTHROPIC_BASE_URL'] as const;

/**
 * 从任意文本抽 KEY=VALUE / JSON 字段。
 * 返回全部命中的 env 映射（含主字段）。
 */
export function extractAllClaudeEnv(text: string): Record<string, string> {
  const out: Record<string, string> = {};

  // 1) 尝试 JSON 对象（完整 settings 或仅 env）
  const jsonEnv = tryParseJsonEnv(text);
  if (jsonEnv) {
    Object.assign(out, jsonEnv);
  }

  // 2) 行式赋值：export / set / $env: / plain KEY=
  const lineRe =
    /(?:^|[;\n\r])\s*(?:export\s+|set\s+|\$env:)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'#;]+))/gim;
  let m: RegExpExecArray | null;
  while ((m = lineRe.exec(text)) !== null) {
    const key = m[1];
    const val = (m[2] ?? m[3] ?? m[4] ?? '').trim();
    if (!key || !val) continue;
    if (
      key.startsWith('ANTHROPIC_') ||
      key.startsWith('CLAUDE_CODE_') ||
      key === 'DISABLE_TELEMETRY'
    ) {
      out[key] = val;
    }
  }

  // 3) JSON 单字段兜底 "KEY": "value"
  const jsonFieldRe = /"(ANTHROPIC_[A-Z0-9_]+|CLAUDE_CODE_[A-Z0-9_]+|DISABLE_TELEMETRY)"\s*:\s*"([^"]*)"/gi;
  while ((m = jsonFieldRe.exec(text)) !== null) {
    if (m[1] && m[2] !== undefined && out[m[1]] === undefined) {
      out[m[1]] = m[2];
    }
  }

  return out;
}

function tryParseJsonEnv(text: string): Record<string, string> | null {
  const trimmed = text.trim();
  // 允许缺少外层大括号的片段：以 "$schema" 或 "env" 开头
  let candidate = trimmed;
  if (!candidate.startsWith('{')) {
    if (/"env"\s*:/.test(candidate) || /"\$schema"\s*:/.test(candidate)) {
      candidate = `{${candidate}}`;
    } else {
      return null;
    }
  }
  try {
    const root = JSON.parse(candidate) as {
      env?: Record<string, unknown>;
      [k: string]: unknown;
    };
    const envObj =
      root.env && typeof root.env === 'object' && !Array.isArray(root.env)
        ? root.env
        : root;
    const out: Record<string, string> = {};
    for (const [k, v] of Object.entries(envObj)) {
      if (typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean') {
        if (
          k.startsWith('ANTHROPIC_') ||
          k.startsWith('CLAUDE_CODE_') ||
          k === 'DISABLE_TELEMETRY'
        ) {
          out[k] = String(v);
        }
      }
    }
    return Object.keys(out).length ? out : null;
  } catch {
    return null;
  }
}

export function pickClaudeDetectFields(env: Record<string, string>): {
  baseUrl?: string;
  apiKey?: string;
  model?: string;
  claudeAuthEnv?: 'ANTHROPIC_AUTH_TOKEN' | 'ANTHROPIC_API_KEY';
  extraEnv: Record<string, string>;
} {
  let baseUrl: string | undefined;
  for (const k of URL_KEYS) {
    if (env[k]) {
      baseUrl = env[k];
      break;
    }
  }

  let apiKey: string | undefined;
  let claudeAuthEnv: 'ANTHROPIC_AUTH_TOKEN' | 'ANTHROPIC_API_KEY' | undefined;
  if (env.ANTHROPIC_AUTH_TOKEN) {
    apiKey = env.ANTHROPIC_AUTH_TOKEN;
    claudeAuthEnv = 'ANTHROPIC_AUTH_TOKEN';
  } else if (env.ANTHROPIC_API_KEY) {
    apiKey = env.ANTHROPIC_API_KEY;
    claudeAuthEnv = 'ANTHROPIC_API_KEY';
  }

  const model = env.ANTHROPIC_MODEL || undefined;

  const extraEnv: Record<string, string> = {};
  for (const k of CLAUDE_ENV_EXTRA_KEYS) {
    if (env[k] !== undefined && env[k] !== '') {
      // ANTHROPIC_MODEL 同时作为主 model 字段；也进 extraEnv 写回 env
      extraEnv[k] = env[k];
    }
  }
  // 其它 ANTHROPIC_* 也并入（除 BASE_URL / AUTH 主密钥）
  for (const [k, v] of Object.entries(env)) {
    if (AUTH_KEYS.includes(k as (typeof AUTH_KEYS)[number])) continue;
    if (URL_KEYS.includes(k as (typeof URL_KEYS)[number])) continue;
    if (k.startsWith('ANTHROPIC_') || k.startsWith('CLAUDE_CODE_') || k === 'DISABLE_TELEMETRY') {
      extraEnv[k] = v;
    }
  }

  return { baseUrl, apiKey, model, claudeAuthEnv, extraEnv };
}
