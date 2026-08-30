/**
 * Shared JSON/TOML preview helpers.
 *
 * This layer never redacts and never restores secrets. Login files, backups,
 * and MCP snippets are the local file text. Session tool payloads are shown
 * as emitted. The supplier editor keeps its own keep-previous-secret masking.
 */

export type SourceFormat = 'json' | 'toml' | 'text';

/** Same cap as MCP snippet clipping (16 KiB). */
export const SOURCE_PREVIEW_MAX_CHARS = 16 * 1024;

const PLAIN_TEXT_MAX_CHARS = 4000;

export function looksLikeJsonObject(text: string): boolean {
  const trimmed = text.trim();
  return trimmed.startsWith('{') || trimmed.startsWith('[');
}

/** Pretty-print JSON objects/arrays. Invalid or non-object JSON is left as-is. */
export function tryPrettyJson(text: string): string | null {
  if (!looksLikeJsonObject(text)) return null;
  try {
    const value: unknown = JSON.parse(text);
    if (value === null || typeof value !== 'object') return null;
    return JSON.stringify(value, null, 2);
  } catch {
    return null;
  }
}

export function clipPreviewText(text: string, max = SOURCE_PREVIEW_MAX_CHARS): string {
  if (text.length <= max) return text;
  return `${text.slice(0, max)}\n…`;
}

export function inferSourceFormat(input: {
  text: string;
  fileName?: string | null;
  hint?: string | null;
}): SourceFormat {
  const hint = input.hint?.trim().toLowerCase();
  if (hint === 'json' || hint === 'toml' || hint === 'text') return hint;

  const name = (input.fileName ?? '').trim().toLowerCase();
  if (name.endsWith('.json') || name.endsWith('.jsonc')) return 'json';
  if (name.endsWith('.toml')) return 'toml';

  if (looksLikeJsonObject(input.text)) return 'json';
  return 'text';
}

/** Read-only display text: pretty JSON when parseable, then clip. */
export function prepareSourcePreview(
  text: string,
  format: SourceFormat,
  options?: { pretty?: boolean; maxChars?: number },
): string {
  const pretty = options?.pretty ?? true;
  const maxChars = options?.maxChars ?? SOURCE_PREVIEW_MAX_CHARS;
  const next = pretty && format === 'json' ? tryPrettyJson(text) ?? text : text;
  return clipPreviewText(next, maxChars);
}

/** Session tool args: pretty JSON when possible; never masked here. */
export function formatJsonPayload(input: unknown): string | null {
  if (input == null) return null;
  if (typeof input === 'string') {
    const pretty = tryPrettyJson(input);
    if (pretty) return clipPreviewText(pretty);
    return clipPreviewText(input, PLAIN_TEXT_MAX_CHARS);
  }
  try {
    return clipPreviewText(JSON.stringify(input, null, 2));
  } catch {
    return clipPreviewText(String(input), PLAIN_TEXT_MAX_CHARS);
  }
}
