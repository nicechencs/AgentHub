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

export function extractGrokDetectFields(text: string): GrokDetectFields | null {
  if (!isGrokTomlPaste(text)) return null;
  const alias = modelAlias(text);
  const body = sectionBody(text, modelTableHeader(text, alias));
  const model = body ? stringValue(body, 'model') || undefined : undefined;
  const baseUrl = body ? stringValue(body, 'base_url') || undefined : undefined;
  const rawKey = body ? stringValue(body, 'api_key') : '';
  const apiKey = rawKey && rawKey !== '***' ? rawKey : undefined;
  return {
    model,
    baseUrl,
    apiKey,
    rawConfigText: text.trim(),
    suggestedName: baseUrl ? hostAsName(baseUrl) : undefined,
  };
}
