/**
 * Agent-native config text: generate/edit the file the CLI actually reads,
 * and reject shapes that are not that file.
 *
 * Official references:
 * - Claude Code `~/.claude/settings.json` — env block, string values
 *   https://code.claude.com/docs/en/settings
 *   https://code.claude.com/docs/en/llm-gateway-connect
 * - Codex `~/.codex/config.toml` — model_providers.*.base_url
 * - Grok `~/.grok/config.toml` — [model."alias"] base_url
 * - Kimi `~/.kimi-code/config.toml` — [providers.*] base_url
 * - Pi `~/.pi/agent/models.json` — providers.*.baseUrl
 */

import type { AgentId } from '@/lib/types';
import { REDACTED_MARKER } from './types';

function parseJsonObject(configText: string):
  | { ok: true; value: Record<string, unknown> }
  | { ok: false; message: string } {
  const trimmed = configText.trim();
  if (!trimmed) return { ok: true, value: {} };
  try {
    const parsed: unknown = JSON.parse(trimmed);
    if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return { ok: false, message: '配置 JSON 必须是对象' };
    }
    return { ok: true, value: parsed as Record<string, unknown> };
  } catch (e) {
    const detail = e instanceof Error ? e.message : String(e);
    return { ok: false, message: `配置 JSON 解析失败：${detail}` };
  }
}

/** OpenAI-compat / route aliases that are not Claude settings.json keys. */
export const CLAUDE_FOREIGN_ROOT_KEYS = [
  'baseURL',
  'baseUrl',
  'base_url',
  'apiKey',
  'api_key',
  'vendor',
  'endpoints',
  'listedModels',
  'contextWindowTokens',
] as const;

export type ClaudeForeignRootKey = (typeof CLAUDE_FOREIGN_ROOT_KEYS)[number];

export function firstNonEmptyString(...values: unknown[]): string {
  for (const value of values) {
    if (typeof value === 'string' && value.trim()) return value.trim();
    if (typeof value === 'number' && Number.isFinite(value)) return String(value);
  }
  return '';
}

export function stripClaudeForeignRootKeys(
  root: Record<string, unknown>,
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...root };
  for (const key of CLAUDE_FOREIGN_ROOT_KEYS) delete out[key];
  return out;
}

export function claudeForeignRootKeysPresent(root: Record<string, unknown>): string[] {
  return CLAUDE_FOREIGN_ROOT_KEYS.filter((key) =>
    Object.prototype.hasOwnProperty.call(root, key),
  );
}

export type NativeConfigIssue = {
  code:
    | 'json_must_be_object'
    | 'json_parse'
    | 'toml_parse'
    | 'expect_toml'
    | 'claude_env_object'
    | 'claude_env_string'
    | 'claude_foreign_keys';
  detail?: string;
  keys?: string[];
};

function jsonIssue(
  parsed: ReturnType<typeof parseJsonObject>,
): NativeConfigIssue | null {
  if (parsed.ok) return null;
  if (parsed.message === '配置 JSON 必须是对象') {
    return { code: 'json_must_be_object' };
  }
  const prefix = '配置 JSON 解析失败：';
  const detail = parsed.message.startsWith(prefix)
    ? parsed.message.slice(prefix.length)
    : parsed.message;
  return { code: 'json_parse', detail };
}

function validateClaudeSettings(root: Record<string, unknown>): NativeConfigIssue | null {
  const foreign = claudeForeignRootKeysPresent(root);
  if (foreign.length) {
    return { code: 'claude_foreign_keys', keys: foreign };
  }
  if (!Object.prototype.hasOwnProperty.call(root, 'env')) return null;
  const env = root.env;
  if (env == null || typeof env !== 'object' || Array.isArray(env)) {
    return { code: 'claude_env_object' };
  }
  for (const [key, value] of Object.entries(env as Record<string, unknown>)) {
    if (value == null) continue;
    if (typeof value === 'string') continue;
    return { code: 'claude_env_string', detail: key };
  }
  return null;
}

/**
 * Quote-aware TOML subset check for the files we generate (tables, keys,
 * basic/literal/triple strings, comments, inline tables).
 */
export function tomlSyntaxIssue(text: string): string | null {
  const trimmed = text.trim();
  if (!trimmed) return null;
  if (trimmed.startsWith('{')) {
    return 'json-object';
  }

  type Mode = 'code' | 'basic' | 'literal' | 'basic3' | 'literal3';
  let mode: Mode = 'code';
  let line = 1;
  let lineHasEquals = false;
  let lineIsTable = false;
  let lineNonEmpty = false;
  let lineStart = true;
  let i = 0;
  const n = text.length;

  const commitLine = (): string | null => {
    if (mode !== 'code') return null;
    if (!lineNonEmpty || lineHasEquals || lineIsTable) return null;
    return `${line}`;
  };

  while (i < n) {
    const c = text[i]!;
    const next3 = text.slice(i, i + 3);

    if (mode === 'code') {
      if (c === '#') {
        while (i < n && text[i] !== '\n') i += 1;
        continue;
      }
      if (lineStart && c === '[') {
        lineIsTable = true;
        lineNonEmpty = true;
        lineStart = false;
        i += 1;
        while (i < n && text[i] !== '\n' && text[i] !== '\r') {
          if (text[i] === ']') {
            i += 1;
            break;
          }
          i += 1;
        }
        continue;
      }
      if (next3 === '"""') {
        mode = 'basic3';
        i += 3;
        lineNonEmpty = true;
        lineStart = false;
        continue;
      }
      if (next3 === "'''") {
        mode = 'literal3';
        i += 3;
        lineNonEmpty = true;
        lineStart = false;
        continue;
      }
      if (c === '"') {
        mode = 'basic';
        i += 1;
        lineNonEmpty = true;
        lineStart = false;
        continue;
      }
      if (c === "'") {
        mode = 'literal';
        i += 1;
        lineNonEmpty = true;
        lineStart = false;
        continue;
      }
      if (c === '\n') {
        const err = commitLine();
        if (err) return err;
        line += 1;
        lineHasEquals = false;
        lineIsTable = false;
        lineNonEmpty = false;
        lineStart = true;
        i += 1;
        continue;
      }
      if (c === '\r') {
        i += 1;
        continue;
      }
      if (c === '=' && !lineIsTable) {
        lineHasEquals = true;
        lineNonEmpty = true;
        lineStart = false;
        i += 1;
        continue;
      }
      if (!/\s/.test(c)) {
        lineNonEmpty = true;
        if (lineStart && c === '[') lineIsTable = true;
        lineStart = false;
      }
      i += 1;
      continue;
    }

    if (mode === 'basic' || mode === 'basic3') {
      if (c === '\\' && i + 1 < n) {
        i += 2;
        continue;
      }
      if (mode === 'basic3' && next3 === '"""') {
        mode = 'code';
        i += 3;
        continue;
      }
      if (mode === 'basic' && c === '"') {
        mode = 'code';
        i += 1;
        continue;
      }
      if (c === '\n') line += 1;
      i += 1;
      continue;
    }

    if (mode === 'literal3' && next3 === "'''") {
      mode = 'code';
      i += 3;
      continue;
    }
    if (mode === 'literal' && c === "'") {
      mode = 'code';
      i += 1;
      continue;
    }
    if (c === '\n') line += 1;
    i += 1;
  }

  if (mode !== 'code') return 'unclosed-string';
  return commitLine();
}

export function validateNativeConfigText(
  agentId: AgentId,
  configText: string,
  format: 'json' | 'toml',
): NativeConfigIssue | null {
  const trimmed = configText.trim();
  if (!trimmed || trimmed === REDACTED_MARKER) return null;

  const jsonLike = format === 'json' || agentId === 'claude' || agentId === 'pi' || agentId === 'workbuddy';
  if (jsonLike) {
    const parsed = parseJsonObject(configText);
    const issue = jsonIssue(parsed);
    if (issue) return issue;
    if (parsed.ok && agentId === 'claude') {
      return validateClaudeSettings(parsed.value);
    }
    return null;
  }

  const tomlErr = tomlSyntaxIssue(configText);
  if (!tomlErr) return null;
  if (tomlErr === 'json-object') return { code: 'expect_toml' };
  return { code: 'toml_parse', detail: tomlErr };
}

export function nativeConfigIssueMessage(issue: NativeConfigIssue): string {
  switch (issue.code) {
    case 'json_must_be_object':
      return '配置 JSON 必须是对象';
    case 'json_parse':
      return `配置 JSON 解析失败：${issue.detail ?? ''}`;
    case 'toml_parse':
      return `配置 TOML 没法解析：${issue.detail ?? ''}`;
    case 'expect_toml':
      return '这个工具的配置是 TOML，不能用 JSON 对象';
    case 'claude_env_object':
      return 'Claude settings.json 的 env 必须是对象';
    case 'claude_env_string':
      return `Claude settings.json 的 env.${issue.detail ?? ''} 必须是字符串`;
    case 'claude_foreign_keys':
      return `这不是 Claude Code 的 settings.json。服务地址请写在 env.ANTHROPIC_BASE_URL，不要并用 ${issue.keys?.join('、') ?? ''}`;
  }
}
