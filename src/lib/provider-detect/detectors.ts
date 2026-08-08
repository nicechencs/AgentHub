/**
 * 智能识别规则表 — 后续按「各种样式配置」在此追加 Detector。
 */
import type { ProviderConfigDetector } from './types';
import { extractAllClaudeEnv, pickClaudeDetectFields } from './claudeEnv';
import {
  extractCodexDetectFields,
  extractOpenAiApiKey,
  isCodexAuthJsonPaste,
  isCodexTomlPaste,
} from './codexToml';

const URL_RE = /https?:\/\/[^\s"'<>\\]+/gi;

const KEY_PATTERNS: RegExp[] = [
  /\b(sk-ant-[A-Za-z0-9_\-]{16,})\b/,
  /\b(sk-[A-Za-z0-9_\-]{16,})\b/,
  /\b(cr_[A-Za-z0-9]{16,})\b/,
  /\b(ak-[A-Za-z0-9_\-]{16,})\b/,
  /\b(xai-[A-Za-z0-9_\-]{16,})\b/,
  /\b([A-Fa-f0-9]{32,})\b/,
];

const NAMED_KEY_RES: RegExp[] = [
  /"OPENAI_API_KEY"\s*:\s*"([^"]+)"/i,
  /(?:export\s+|set\s+|\$env:)?OPENAI_API_KEY["']?\s*=\s*["']?([^\s"',}\\]+)/i,
  /(?:^|["\s])api_key["']?\s*[=:]\s*["']([^"']+)["']/im,
];

const NAMED_URL_RES: RegExp[] = [
  /"base_url"\s*:\s*"(https?:\/\/[^"]+)"/i,
  /(?:^|["\s]|export\s+|set\s+)base_url["']?\s*[=:]\s*["']?(https?:\/\/[^\s"',}\\]+)/im,
];

function cleanUrl(raw: string): string {
  return raw.replace(/[),.;]+$/g, '').replace(/\\+$/g, '').trim();
}

function looksLikeUrl(s: string): boolean {
  return /^https?:\/\/.+/i.test(s.trim());
}

function looksLikeKey(s: string): boolean {
  const t = s.trim();
  if (t.length < 12 || t.length > 512) return false;
  if (/^https?:\/\//i.test(t)) return false;
  if (/[\s{}"'=]/.test(t)) return false;
  return KEY_PATTERNS.some((re) => {
    const m = t.match(re);
    return Boolean(m?.[1] && m[1] === t);
  });
}

function hostAsName(url: string): string | undefined {
  try {
    return new URL(url).host || undefined;
  } catch {
    return undefined;
  }
}

function firstMatch(text: string, res: RegExp[]): string | undefined {
  for (const re of res) {
    const m = text.match(re);
    if (m?.[1]) {
      const v = m[1].trim();
      if (v && v !== '***' && !/^[•x*]+$/i.test(v)) return v;
    }
  }
  return undefined;
}

function scoreUrl(u: string): number {
  let s = 0;
  if (/api/i.test(u)) s += 2;
  if (/anthropic|openai|claude|codex|relay|proxy|api\./i.test(u)) s += 2;
  if (/\/v1\/?$/i.test(u) || /\/api\/?$/i.test(u)) s += 1;
  return s;
}

function firstUrl(text: string): string | undefined {
  const named = firstMatch(text, NAMED_URL_RES);
  if (named) return cleanUrl(named);
  const all = text.match(URL_RE);
  if (!all?.length) return undefined;
  return [...all]
    .map(cleanUrl)
    .filter(Boolean)
    .sort((a, b) => scoreUrl(b) - scoreUrl(a))[0];
}

function firstKey(text: string): string | undefined {
  const named = firstMatch(text, NAMED_KEY_RES);
  if (named && looksLikeKey(named)) return named;
  if (named && named.length >= 16 && !looksLikeUrl(named)) return named;
  for (const re of KEY_PATTERNS) {
    const m = text.match(re);
    if (m?.[1] && !looksLikeUrl(m[1])) return m[1];
  }
  return undefined;
}

function firstModel(text: string): string | undefined {
  const m =
    text.match(/"model"\s*:\s*"([^"]+)"/i) ||
    text.match(/^\s*model\s*=\s*"([^"]+)"/im) ||
    text.match(/^\s*default_model\s*=\s*"([^"]+)"/im);
  return m?.[1]?.trim() || undefined;
}

function extractClaudeBlock(text: string) {
  if (!/ANTHROPIC_/i.test(text) && !/CLAUDE_CODE_/i.test(text)) return null;
  const env = extractAllClaudeEnv(text);
  const picked = pickClaudeDetectFields(env);
  if (!picked.baseUrl && !picked.apiKey && Object.keys(picked.extraEnv).length === 0) {
    return null;
  }
  return {
    baseUrl: picked.baseUrl,
    apiKey: picked.apiKey,
    // settings 顶层 model 或 ANTHROPIC_MODEL
    model: picked.model || firstModel(text),
    extraEnv: Object.keys(picked.extraEnv).length ? picked.extraEnv : undefined,
    claudeAuthEnv: picked.claudeAuthEnv,
    suggestedName: picked.baseUrl ? hostAsName(picked.baseUrl) : undefined,
  };
}

/** 内置 detector 列表（顺序：专用样式优先） */
export const DETECTORS: ProviderConfigDetector[] = [
  {
    id: 'plain-url',
    description: '整段仅为 Endpoint URL',
    extract: (text) => {
      const t = text.trim();
      if (!looksLikeUrl(t) || t.includes('\n') || t.length >= 500) return null;
      const baseUrl = cleanUrl(t);
      return { baseUrl, suggestedName: hostAsName(baseUrl) };
    },
  },
  {
    id: 'plain-api-key',
    description: '整段仅为 API Key',
    extract: (text) => {
      const t = text.trim();
      if (!looksLikeKey(t) || t.includes('\n')) return null;
      return { apiKey: t };
    },
  },
  {
    id: 'claude-settings-json',
    description: 'Claude settings.json（$schema + env）',
    extract: (text) => {
      if (!/"env"\s*:/.test(text) && !/claude-code-settings/i.test(text)) return null;
      return extractClaudeBlock(text);
    },
  },
  {
    id: 'claude-shell-export',
    description: 'bash/zsh: export ANTHROPIC_* / CLAUDE_CODE_*',
    extract: (text) => {
      if (!/export\s+(?:ANTHROPIC_|CLAUDE_CODE_)/i.test(text)) return null;
      return extractClaudeBlock(text);
    },
  },
  {
    id: 'claude-cmd-set',
    description: 'Windows cmd: set ANTHROPIC_*=',
    extract: (text) => {
      if (!/^set\s+(?:ANTHROPIC_|CLAUDE_CODE_)/im.test(text)) return null;
      return extractClaudeBlock(text);
    },
  },
  {
    id: 'claude-powershell-env',
    description: 'PowerShell: $env:ANTHROPIC_*',
    extract: (text) => {
      if (!/\$env:(?:ANTHROPIC_|CLAUDE_CODE_)/i.test(text)) return null;
      return extractClaudeBlock(text);
    },
  },
  {
    id: 'claude-env-generic',
    description: '任意含 ANTHROPIC_* 的 env 块（兜底）',
    extract: (text) => {
      if (!/ANTHROPIC_/i.test(text)) return null;
      return extractClaudeBlock(text);
    },
  },
  {
    id: 'codex-auth-json',
    description: 'Codex auth.json：{ "OPENAI_API_KEY": "sk-..." } → ~/.codex/auth.json',
    extract: (text) => {
      if (!isCodexAuthJsonPaste(text) && !/"OPENAI_API_KEY"\s*:/.test(text)) {
        return null;
      }
      // 完整 toml 交给 codex-config-toml
      if (isCodexTomlPaste(text)) return null;
      const apiKey = extractOpenAiApiKey(text);
      if (!apiKey) return null;
      return { apiKey };
    },
  },
  {
    id: 'codex-config-toml',
    description:
      'Codex config.toml（model_provider + [model_providers.*] + base_url/wire_api/env_key）',
    extract: (text) => {
      if (!isCodexTomlPaste(text) && !/\[model_providers\./.test(text)) return null;
      const f = extractCodexDetectFields(text);
      if (!f.baseUrl && !f.model && !f.tomlBody) return null;
      return {
        baseUrl: f.baseUrl,
        apiKey: f.apiKey,
        model: f.model,
        reasoningEffort: f.reasoningEffort,
        wireApi: f.wireApi,
        providerSlug: f.providerSlug,
        envKey: f.envKey,
        rawConfigText: f.tomlBody,
        suggestedName: f.baseUrl ? hostAsName(f.baseUrl) : undefined,
      };
    },
  },
  {
    id: 'codex-toml-or-env',
    description: 'Codex 碎片：OPENAI_API_KEY / base_url 杂糅',
    extract: (text) => {
      if (
        !/model_provider|model_providers|OPENAI_API_KEY|wire_api/i.test(text)
      ) {
        return null;
      }
      // 完整 toml / 纯 auth.json 已由专用 detector 处理
      if (isCodexTomlPaste(text) || isCodexAuthJsonPaste(text)) return null;
      const f = extractCodexDetectFields(text);
      const baseUrl = f.baseUrl || firstUrl(text);
      const apiKey = f.apiKey || firstKey(text);
      const model = f.model || firstModel(text);
      if (!baseUrl && !apiKey) return null;
      return {
        baseUrl,
        apiKey,
        model,
        reasoningEffort: f.reasoningEffort,
        wireApi: f.wireApi,
        providerSlug: f.providerSlug,
        suggestedName: baseUrl ? hostAsName(baseUrl) : undefined,
      };
    },
  },
  {
    id: 'generic-mixed',
    description: '泛化兜底：任意文本中的 URL + Key',
    extract: (text) => {
      const baseUrl = firstUrl(text);
      const apiKey = firstKey(text);
      const model = firstModel(text);
      if (!baseUrl && !apiKey) return null;
      return {
        baseUrl,
        apiKey,
        model,
        suggestedName: baseUrl ? hostAsName(baseUrl) : undefined,
      };
    },
  },
];

export function registerDetector(detector: ProviderConfigDetector): void {
  const i = DETECTORS.findIndex((d) => d.id === detector.id);
  if (i >= 0) DETECTORS[i] = detector;
  else DETECTORS.push(detector);
}
