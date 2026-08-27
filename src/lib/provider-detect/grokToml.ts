/**
 * Grok Build config.toml detection helpers.
 *
 * Grok's user config is a model registry (`[models]` plus
 * `[model."<alias>"]`), not the legacy top-level model/base_url/api_key shape.
 * This intentionally remains a small, tolerant reader like codexToml.ts; the
 * complete TOML is preserved and the Rust projector is the authoritative
 * parser on save.
 */

export interface GrokDetectFields {
  model?: string;
  baseUrl?: string;
  apiKey?: string;
  rawConfigText: string;
  suggestedName?: string;
}

const OFFICIAL_XAI_HOST = /api\.x\.ai/i;
const SECRET_EXPORT_RE =
  /^(export\s+|set\s+|\$env:)?[A-Za-z_][A-Za-z0-9_]*(_API_KEY|_AUTH_TOKEN|_TOKEN|_SECRET|API_KEY)\s*=/i;
const ANY_EXPORT_RE = /^(export\s+|set\s+|\$env:)/i;
const KEY_ASSIGN_RE =
  /(?:export\s+|set\s+|\$env:)?(?:XAI_API_KEY|GROK_API_KEY|OPENAI_API_KEY|API_KEY)\s*=\s*["']?([^\s"',;]+)/i;
const BASE_EXPORT_RE =
  /(?:export\s+|set\s+|\$env:)?(?:GROK_MODELS_BASE_URL|XAI_BASE_URL|OPENAI_BASE_URL)\s*=\s*["']?(https?:\/\/[^\s"',;]+)/i;

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** Pull the TOML document out of mixed paste (prose, 格式1：, export lines). */
export function extractGrokTomlDocument(text: string): string {
  const normalized = text.replace(/\r\n/g, '\n');
  const header = normalized.search(/^\s*\[[^\]]+\]\s*$/m);
  const body = header >= 0 ? normalized.slice(header) : normalized;
  return body
    .split('\n')
    .filter((line) => {
      const trimmed = line.trim();
      if (!trimmed) return true;
      if (SECRET_EXPORT_RE.test(trimmed) || ANY_EXPORT_RE.test(trimmed)) return false;
      if (/^格式\s*\d+\s*[：:]/.test(trimmed)) return false;
      if (/^#/.test(trimmed) || /^\s*\[/.test(trimmed)) return true;
      if (/^[A-Za-z0-9_."-]+\s*=/.test(trimmed)) return true;
      if (/[\u4e00-\u9fff]/.test(trimmed) && !trimmed.includes('=')) return false;
      return true;
    })
    .join('\n');
}

function sectionBody(text: string, header: string): string {
  const re = new RegExp(`^\\[${escapeRegExp(header)}\\]\\s*$`, 'm');
  const match = re.exec(text);
  if (!match) return '';
  const after = text.slice(match.index + match[0].length);
  const next = after.search(/^\s*\[/m);
  return next < 0 ? after : after.slice(0, next);
}

function stringValue(body: string, key: string): string {
  const re = new RegExp(`^\\s*${escapeRegExp(key)}\\s*=\\s*["']([^"']*)["']`, 'm');
  return body.match(re)?.[1]?.trim() ?? '';
}

function modelAlias(text: string): string {
  const fromDefault = stringValue(sectionBody(text, 'models'), 'default');
  if (fromDefault && sectionBody(text, modelTableHeader(text, fromDefault))) {
    return fromDefault;
  }
  const match = text.match(/^\[model\.(?:"([^"]+)"|([^\]]+))\]\s*$/m);
  return (match?.[1] ?? match?.[2] ?? fromDefault ?? 'grok').trim() || 'grok';
}

function modelTableHeader(text: string, alias: string): string {
  const quoted = `model."${alias}"`;
  if (sectionBody(text, quoted)) return quoted;
  const bare = `model.${alias}`;
  if (sectionBody(text, bare)) return bare;
  return quoted;
}

function hostAsName(url: string): string | undefined {
  try {
    return new URL(url).host || undefined;
  } catch {
    return undefined;
  }
}

export function isGrokTomlPaste(text: string): boolean {
  // Registry paste: [models] + [model."alias"] is enough even if one overlay
  // field (model / base_url / api_key) is missing. Mixed paste with 格式1： /
  // export lines still counts once the TOML body is extracted.
  const body = extractGrokTomlDocument(text);
  const source = body.trim() ? body : text;
  return (
    /^\s*\[models\]\s*$/im.test(source) &&
    /^\s*\[model\.(?:"[^"]+"|[^\]]+)\]\s*$/im.test(source)
  );
}

/** Drop shell export lines and prose so the advanced editor never echoes secrets. */
export function stripGrokPasteNoise(text: string): string {
  return extractGrokTomlDocument(text);
}

function modelTableHeaders(text: string): string[] {
  const headers: string[] = [];
  const re = /^\[model\.(?:"([^"]+)"|([^\]]+))\]\s*$/gm;
  let match: RegExpExecArray | null;
  while ((match = re.exec(text))) {
    const alias = (match[1] ?? match[2] ?? '').trim();
    if (!alias) continue;
    headers.push(text.includes(`[model."${alias}"]`) ? `model."${alias}"` : `model.${alias}`);
  }
  return headers;
}

function pickPreferredBaseUrl(urls: string[]): string | undefined {
  const cleaned = urls.map((url) => url.trim().replace(/\/+$/, '')).filter(Boolean);
  const custom = cleaned.find((url) => !OFFICIAL_XAI_HOST.test(url));
  return custom ?? cleaned[0];
}

function collectGrokBaseUrls(text: string): string[] {
  const urls: string[] = [];
  const push = (url: string) => {
    const trimmed = url.trim();
    if (trimmed && !urls.includes(trimmed)) urls.push(trimmed);
  };
  for (const header of modelTableHeaders(text)) {
    const url = stringValue(sectionBody(text, header), 'base_url');
    if (url) push(url);
  }
  const top = stringValue(text, 'base_url');
  if (top.startsWith('http')) push(top);
  const loose = text.match(/https?:\/\/[^\s"'<>\\]+/gi) ?? [];
  for (const raw of loose) {
    push(raw.replace(/[),.;]+$/g, ''));
  }
  return urls;
}

export function extractGrokDetectFields(text: string): GrokDetectFields | null {
  const exportKey = text.match(KEY_ASSIGN_RE)?.[1]?.trim();
  const exportBase = text.match(BASE_EXPORT_RE)?.[1]?.trim();
  const cleaned = stripGrokPasteNoise(text);
  if (!isGrokTomlPaste(cleaned) && !isGrokTomlPaste(text)) return null;
  const source = isGrokTomlPaste(cleaned) ? cleaned : extractGrokTomlDocument(text);
  const alias = modelAlias(source);
  const body = sectionBody(source, modelTableHeader(source, alias));
  const model = body ? stringValue(body, 'model') || undefined : undefined;
  const collected = collectGrokBaseUrls(source);
  if (exportBase) collected.unshift(exportBase);
  const baseUrl = pickPreferredBaseUrl(collected);
  const tableKey = body ? stringValue(body, 'api_key') : '';
  const rawKey =
    (tableKey && tableKey !== '***' ? tableKey : '') ||
    (exportKey && exportKey !== '***' ? exportKey : '');
  const apiKey = rawKey || undefined;
  const rawConfigText = stripGrokPasteNoise(source).trim();
  return {
    model,
    baseUrl,
    apiKey,
    rawConfigText: rawConfigText.endsWith('\n') ? rawConfigText : `${rawConfigText}\n`,
    suggestedName: baseUrl ? hostAsName(baseUrl) : undefined,
  };
}
