/**
 * Shared source preview helpers (JSON / TOML / common languages / plain text).
 *
 * This layer never redacts and never restores secrets. Login files, backups,
 * and MCP snippets are the local file text. Session tool payloads are shown
 * as emitted. The supplier editor keeps its own keep-previous-secret masking.
 */

export type SourceFormat =
  | 'json'
  | 'toml'
  | 'yaml'
  | 'javascript'
  | 'typescript'
  | 'python'
  | 'rust'
  | 'go'
  | 'java'
  | 'c'
  | 'cpp'
  | 'css'
  | 'html'
  | 'xml'
  | 'sql'
  | 'shell'
  | 'powershell'
  | 'dockerfile'
  | 'diff'
  | 'ruby'
  | 'properties'
  | 'text';

/** Same cap as MCP snippet clipping (16 KiB). */
export const SOURCE_PREVIEW_MAX_CHARS = 16 * 1024;

/** Chat file preview may show larger buffers (matches Rust markdown preview cap usage on the wire). */
export const CHAT_FILE_PREVIEW_MAX_CHARS = 256 * 1024;

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

/** Drop whitespace-only lines so tool JSON snippets stay dense. */
export function compressBlankLines(text: string): string {
  return text
    .replace(/\r\n/g, '\n')
    .replace(/\n[^\S\n]*\n+/g, '\n')
    .replace(/^\n+/, '')
    .replace(/\n+$/, '');
}

const FORMAT_BY_EXT: Record<string, SourceFormat> = {
  json: 'json',
  jsonc: 'json',
  json5: 'json',
  toml: 'toml',
  yaml: 'yaml',
  yml: 'yaml',
  js: 'javascript',
  jsx: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  ts: 'typescript',
  tsx: 'typescript',
  mts: 'typescript',
  cts: 'typescript',
  py: 'python',
  pyi: 'python',
  rs: 'rust',
  go: 'go',
  java: 'java',
  kt: 'java',
  kts: 'java',
  c: 'c',
  h: 'c',
  cc: 'cpp',
  cpp: 'cpp',
  cxx: 'cpp',
  hpp: 'cpp',
  hxx: 'cpp',
  cs: 'java',
  css: 'css',
  scss: 'css',
  less: 'css',
  html: 'html',
  htm: 'html',
  xml: 'xml',
  svg: 'xml',
  sql: 'sql',
  sh: 'shell',
  bash: 'shell',
  zsh: 'shell',
  fish: 'shell',
  ps1: 'powershell',
  rb: 'ruby',
  diff: 'diff',
  patch: 'diff',
  properties: 'properties',
  ini: 'properties',
  cfg: 'properties',
  conf: 'properties',
  env: 'properties',
};

export function inferSourceFormat(input: {
  text: string;
  fileName?: string | null;
  hint?: string | null;
}): SourceFormat {
  const hint = input.hint?.trim().toLowerCase();
  if (hint && isSourceFormat(hint)) return hint;

  const name = (input.fileName ?? '').trim().toLowerCase();
  const base = name.split(/[/\\]/).pop() ?? name;
  if (base === 'dockerfile') return 'dockerfile';
  if (base === 'makefile') return 'shell';
  const ext = base.includes('.') ? base.slice(base.lastIndexOf('.') + 1) : '';
  if (ext && FORMAT_BY_EXT[ext]) return FORMAT_BY_EXT[ext];

  if (looksLikeJsonObject(input.text)) return 'json';
  return 'text';
}

function isSourceFormat(value: string): value is SourceFormat {
  return (
    value === 'json' ||
    value === 'toml' ||
    value === 'yaml' ||
    value === 'javascript' ||
    value === 'typescript' ||
    value === 'python' ||
    value === 'rust' ||
    value === 'go' ||
    value === 'java' ||
    value === 'c' ||
    value === 'cpp' ||
    value === 'css' ||
    value === 'html' ||
    value === 'xml' ||
    value === 'sql' ||
    value === 'shell' ||
    value === 'powershell' ||
    value === 'dockerfile' ||
    value === 'diff' ||
    value === 'ruby' ||
    value === 'properties' ||
    value === 'text'
  );
}

/** Read-only display text: pretty JSON when parseable, then clip. */
export function prepareSourcePreview(
  text: string,
  format: SourceFormat,
  options?: { pretty?: boolean; maxChars?: number; compressBlankLines?: boolean },
): string {
  const pretty = options?.pretty ?? true;
  const maxChars = options?.maxChars ?? SOURCE_PREVIEW_MAX_CHARS;
  const compress = options?.compressBlankLines ?? true;
  let next = pretty && format === 'json' ? tryPrettyJson(text) ?? text : text;
  if (compress && format === 'json') next = compressBlankLines(next);
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
