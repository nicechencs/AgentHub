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
function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
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
  if (fromDefault) return fromDefault;
  const match = text.match(/^\[model\.(?:"([^"]+)"|([^\]]+))\]\s*$/m);
  return (match?.[1] ?? match?.[2] ?? 'grok').trim() || 'grok';
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
  // field (model / base_url / api_key) is missing.
  return (
    /^\s*\[models\]\s*$/im.test(text) &&
    /^\s*\[model\.(?:"[^"]+"|[^\]]+)\]\s*$/im.test(text)
  );
}

/** Drop shell `export KEY=...` lines so the advanced editor never echoes secrets. */
export function stripGrokPasteNoise(text: string): string {
  return text
    .replace(/\r\n/g, '\n')
    .split('\n')
    .filter((line) => !SECRET_EXPORT_RE.test(line.trim()))
    .join('\n');
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
  const cleaned = stripGrokPasteNoise(text);
  if (!isGrokTomlPaste(cleaned) && !isGrokTomlPaste(text)) return null;
  const source = isGrokTomlPaste(cleaned) ? cleaned : text;
  const alias = modelAlias(source);
  const body = sectionBody(source, modelTableHeader(source, alias));
  const model = body ? stringValue(body, 'model') || undefined : undefined;
  const baseUrl = pickPreferredBaseUrl(collectGrokBaseUrls(source));
  const rawKey = body ? stringValue(body, 'api_key') : '';
  const apiKey = rawKey && rawKey !== '***' ? rawKey : undefined;
  const rawConfigText = stripGrokPasteNoise(source).trim();
  return {
    model,
    baseUrl,
    apiKey,
    rawConfigText: rawConfigText.endsWith('\n') ? rawConfigText : `${rawConfigText}\n`,
    suggestedName: baseUrl ? hostAsName(baseUrl) : undefined,
  };
}
