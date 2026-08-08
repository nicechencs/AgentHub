/**
 * Codex config.toml 解析辅助（容错：UI 文案、路径标签、双块顺序、残缺 JSON）。
 */

/** 去掉中转弹窗/截图带来的噪音行，便于后续 toml 识别 */
export function stripCodexPasteNoise(text: string): string {
  return text
    .replace(/\r\n/g, '\n')
    .split('\n')
    .filter((line) => {
      const t = line.trim();
      if (!t) return true;
      // 路径标签 / 复制按钮
      if (/^~\.?\/?\.codex\/(config\.toml|auth\.json)\s*$/i.test(t)) return false;
      if (/^(复制|关闭|Terminal|macOS\s*\/\s*Linux|Windows)\s*$/i.test(t)) return false;
      if (/使用\s*API\s*密钥/i.test(t)) return false;
      if (/请确保|请备份|配置目录|mkdir/i.test(t)) return false;
      if (/如已有\s*config\.toml/i.test(t)) return false;
      if (/将\s*config\.toml\s*保存到/i.test(t)) return false;
      if (/Codex\s*CLI/i.test(t) && t.length < 40) return false;
      return true;
    })
    .join('\n');
}

export function isCodexTomlPaste(text: string): boolean {
  const t = stripCodexPasteNoise(text).trim();
  if (!t) return false;
  // 典型：model_provider + [model_providers.xxx] 或 model + base_url 表
  if (/model_provider\s*=/i.test(t) && /\[model_providers\./i.test(t)) return true;
  if (/\[model_providers\./i.test(t) && /base_url\s*=/i.test(t)) return true;
  return false;
}

/** 从粘贴文本抽出可作为 config.toml 的正文（去掉 auth.json / export key / UI 标签行） */
export function extractCodexTomlBody(text: string): string {
  let src = stripCodexPasteNoise(text);

  // 截掉 auth.json 对象（完整或残缺）
  const jsonBlock = src.search(
    /\n\s*\{[\s\S]*"OPENAI_API_KEY"\s*:/i,
  );
  if (jsonBlock >= 0) {
    // 仅当 { 不在 toml 表结构中间：出现在 features 等之后更常见
    const after = src.slice(jsonBlock);
    if (/"OPENAI_API_KEY"/i.test(after) && !/model_provider\s*=/i.test(after.slice(0, 80))) {
      src = src.slice(0, jsonBlock);
    }
  }

  const lines = src.split('\n');
  const kept: string[] = [];
  let inAuthJson = false;
  for (const line of lines) {
    const trimmed = line.trim();
    // auth.json 花括号块
    if (
      !inAuthJson &&
      /^\{\s*$/.test(trimmed) &&
      /OPENAI_API_KEY/i.test(src.slice(src.indexOf(line)))
    ) {
      inAuthJson = true;
      continue;
    }
    if (inAuthJson) {
      if (/^\}\s*$/.test(trimmed)) inAuthJson = false;
      continue;
    }
    // 密钥行 / 标签
    if (/^(export\s+|set\s+|\$env:)?[A-Za-z_][A-Za-z0-9_]*API_KEY\s*=/i.test(trimmed))
      continue;
    if (/"OPENAI_API_KEY"\s*:/i.test(trimmed)) continue;
    if (/^#\s*auth\.json/i.test(trimmed)) continue;
    if (/^#\s*Terminal/i.test(trimmed)) continue;
    if (/^sk-[A-Za-z0-9_\-]{16,}\s*$/.test(trimmed)) continue;
    // 空行保留（toml 段落间距）
    kept.push(line);
  }
  while (kept.length && !kept[0]!.trim()) kept.shift();
  while (kept.length && !kept[kept.length - 1]!.trim()) kept.pop();
  return kept.join('\n') + (kept.length ? '\n' : '');
}

export function tomlTopGet(text: string, key: string): string {
  const re = new RegExp(`^\\s*${escapeRe(key)}\\s*=\\s*(?:"([^"]*)"|'([^']*)'|(\\S+))`, 'm');
  const m = text.match(re);
  return (m?.[1] ?? m?.[2] ?? m?.[3] ?? '').replace(/,$/, '').trim();
}

export function firstProviderTableSlug(text: string): string {
  const m = text.match(/^\s*\[model_providers\.([^\]]+)\]/im);
  if (m?.[1]) return m[1].trim();
  const top = tomlTopGet(text, 'model_provider');
  return top || 'custom';
}

export function tomlTableGet(text: string, table: string, key: string): string {
  // 容错：表头大小写
  const re = new RegExp(
    `^\\s*\\[${escapeRe(table).replace(/\\\./g, '\\.')}\\]\\s*$`,
    'im',
  );
  const lines = text.replace(/\r\n/g, '\n').split('\n');
  let start = -1;
  for (let i = 0; i < lines.length; i++) {
    if (re.test(lines[i]!)) {
      start = i;
      break;
    }
  }
  // 回退精确 indexOf
  if (start < 0) {
    const header = `[${table}]`;
    const idx = text.indexOf(header);
    if (idx < 0) return '';
    const after = text.slice(idx + header.length);
    const next = after.search(/^\s*\[/m);
    const body = next < 0 ? after : after.slice(0, next);
    return tomlTopGet(body, key);
  }
  const bodyLines: string[] = [];
  for (let i = start + 1; i < lines.length; i++) {
    if (/^\s*\[/.test(lines[i]!)) break;
    bodyLines.push(lines[i]!);
  }
  return tomlTopGet(bodyLines.join('\n'), key);
}

function escapeRe(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** 从 Codex 粘贴文本识别表单字段 + 可选 API Key（容错入口） */
export function extractCodexDetectFields(text: string): {
  baseUrl?: string;
  apiKey?: string;
  model?: string;
  reasoningEffort?: string;
  wireApi?: string;
  providerSlug?: string;
  /** [model_providers.x].env_key，如 SUB2API_API_KEY */
  envKey?: string;
  tomlBody?: string;
} {
  const cleaned = stripCodexPasteNoise(text);
  const isToml = isCodexTomlPaste(cleaned);
  const body = isToml ? extractCodexTomlBody(cleaned) : cleaned;
  const slug = firstProviderTableSlug(body);
  const table = `model_providers.${slug}`;
  const baseUrl =
    tomlTableGet(body, table, 'base_url') ||
    tomlTopGet(body, 'base_url') ||
    undefined;
  const model = tomlTopGet(body, 'model') || undefined;
  const reasoningEffort = tomlTopGet(body, 'model_reasoning_effort') || undefined;
  const wireApi = tomlTableGet(body, table, 'wire_api') || undefined;
  const envKey = tomlTableGet(body, table, 'env_key') || undefined;

  // Key：auth.json / export OPENAI_API_KEY / export <env_key> / 纯 sk-
  // 用原始 text，避免 strip 掉 export 行
  const apiKey = extractOpenAiApiKey(text, envKey || undefined);

  return {
    baseUrl: baseUrl || undefined,
    apiKey,
    model,
    reasoningEffort,
    wireApi,
    providerSlug: isToml ? slug : undefined,
    envKey: envKey || undefined,
    tomlBody: isToml ? body : undefined,
  };
}

/** 是否像 ~/.codex/auth.json（仅密钥、无 config.toml 结构） */
export function isCodexAuthJsonPaste(text: string): boolean {
  const t = text.trim();
  if (!t || isCodexTomlPaste(t)) return false;
  if (/OPENAI_API_KEY/i.test(t) && !/model_provider|model_providers/i.test(t)) {
    return true;
  }
  return false;
}

/**
 * 抽出 API Key：
 * - auth.json: { "OPENAI_API_KEY": "sk-..." }
 * - export / set / $env: OPENAI_API_KEY=...
 * - export / set / $env: <envKey>=...（如 SUB2API_API_KEY，来自 model_providers.env_key）
 * - 纯 sk- 行
 */
export function extractOpenAiApiKey(
  text: string,
  envKeyName?: string,
): string | undefined {
  // JSON 完整或残缺
  try {
    let candidate = text.trim();
    if (!candidate.startsWith('{') && /"OPENAI_API_KEY"\s*:/.test(candidate)) {
      candidate = `{${candidate}}`;
    }
    if (candidate.startsWith('{')) {
      const obj = JSON.parse(candidate) as Record<string, unknown>;
      if (typeof obj.OPENAI_API_KEY === 'string' && obj.OPENAI_API_KEY.trim()) {
        return obj.OPENAI_API_KEY.trim();
      }
      if (
        envKeyName &&
        typeof obj[envKeyName] === 'string' &&
        String(obj[envKeyName]).trim()
      ) {
        return String(obj[envKeyName]).trim();
      }
    }
  } catch {
    /* fall through */
  }

  const jsonField = text.match(/"OPENAI_API_KEY"\s*:\s*"([^"]+)"/i);
  if (jsonField?.[1]) return jsonField[1].trim();

  // 优先匹配 toml 里声明的 env_key（Sub2API 等）
  if (envKeyName && /^[A-Za-z_][A-Za-z0-9_]*$/.test(envKeyName)) {
    const named = text.match(
      new RegExp(
        `(?:export\\s+|set\\s+|\\$env:)?${envKeyName}\\s*=\\s*["']?([^\\s"'#]+)`,
        'i',
      ),
    );
    if (named?.[1]) return named[1].trim();
  }

  const keyLine = text.match(
    /(?:export\s+|set\s+|\$env:)?OPENAI_API_KEY\s*=\s*["']?([^\s"'#]+)/i,
  );
  if (keyLine?.[1]) return keyLine[1].trim();

  // 任意 *API_KEY= 导出行
  const anyApiKey = text.match(
    /(?:export\s+|set\s+|\$env:)?([A-Za-z_][A-Za-z0-9_]*API_KEY)\s*=\s*["']?([^\s"'#]+)/i,
  );
  if (anyApiKey?.[2]) return anyApiKey[2].trim();

  const sk = text.match(/\b(sk-[A-Za-z0-9_\-]{16,})\b/);
  return sk?.[1];
}
