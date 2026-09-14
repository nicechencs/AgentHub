/**
 * Local file targets opened from chat markdown links.
 * Supports GitHub-style `#L12` / `#L12-L20` and trailing `:12` (not Windows drives).
 */

const MARKDOWN_EXTS = /\.(md|mdx|markdown)$/i;

const PREVIEWABLE_EXTS =
  /\.(md|mdx|markdown|txt|text|log|json|jsonc|json5|toml|ya?ml|xml|html?|css|scss|less|jsx?|mjs|cjs|tsx?|mts|cts|py|pyi|rs|go|java|kts?|c|h|cc|cpp|cxx|hpp|hxx|cs|swift|rb|php|sh|bash|zsh|fish|ps1|bat|cmd|sql|graphql|gql|r|lua|pl|pm|vue|svelte|astro|ini|cfg|conf|env|properties|svg|csv|tsv)$/i;

const PREVIEWABLE_BASENAMES = new Set([
  'dockerfile',
  'makefile',
  'gemfile',
  'rakefile',
  'procfile',
  'cmakelists.txt',
  '.gitignore',
  '.dockerignore',
  '.editorconfig',
]);

export type ChatLocalFileTarget = {
  path: string;
  /** 1-based line to reveal/highlight in the detail pane. */
  line?: number;
};

function fileBaseName(path: string): string {
  const parts = path.trim().split(/[/\\]/).filter(Boolean);
  return (parts[parts.length - 1] ?? path).toLowerCase();
}

export function isMarkdownFilePath(path: string): boolean {
  return MARKDOWN_EXTS.test(path.trim().split(/[#?]/)[0] ?? '');
}

export function isPreviewableChatFilePath(path: string): boolean {
  const clean = (path.trim().split(/[#?]/)[0] ?? '').trim();
  if (!clean) return false;
  const base = fileBaseName(clean);
  if (PREVIEWABLE_BASENAMES.has(base)) return true;
  return PREVIEWABLE_EXTS.test(clean);
}

/** Parse `#L12`, `#L12-L20`, `#line=12`, or trailing `:12` (not `C:`). */
export function parseChatFileLineRef(href: string): number | undefined {
  const raw = href.trim();
  if (!raw) return undefined;

  const hash = raw.includes('#') ? raw.slice(raw.indexOf('#') + 1) : '';
  if (hash) {
    const github = hash.match(/^L(\d+)(?:-L?\d+)?$/i);
    if (github) {
      const line = Number(github[1]);
      return line > 0 ? line : undefined;
    }
    const named = hash.match(/^line=(\d+)$/i);
    if (named) {
      const line = Number(named[1]);
      return line > 0 ? line : undefined;
    }
  }

  // `path/file.ts:42` — skip Windows drive `C:\...`
  const noHash = raw.split('#')[0] ?? raw;
  if (/^[A-Za-z]:[\\/]/.test(noHash)) {
    const afterDrive = noHash.slice(2);
    const m = afterDrive.match(/:(\d+)$/);
    if (m) {
      const line = Number(m[1]);
      return line > 0 ? line : undefined;
    }
    return undefined;
  }
  const colon = noHash.match(/:(\d+)$/);
  if (colon) {
    const line = Number(colon[1]);
    return line > 0 ? line : undefined;
  }
  return undefined;
}

/** Strip a trailing `:line` that is not a Windows drive letter. */
export function stripChatFileLineSuffix(path: string): string {
  const trimmed = path.trim();
  if (/^[A-Za-z]:[\\/]/.test(trimmed)) {
    return trimmed.slice(0, 2) + trimmed.slice(2).replace(/:(\d+)$/, '');
  }
  return trimmed.replace(/:(\d+)$/, '');
}
