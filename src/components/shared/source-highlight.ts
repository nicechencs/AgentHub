import { classHighlighter, highlightTree } from '@lezer/highlight';
import { sourceLanguageParser } from '@/components/shared/source-preview-theme';
import { inferSourceFormat, looksLikeJsonObject, type SourceFormat } from '@/lib/source-preview';

export type SourceToken = {
  text: string;
  className?: string;
};

const HIGHLIGHT_MAX_CHARS = 8000;

function looksLikeDiff(text: string): boolean {
  if (/^(?:diff --git|@@ |--- |\+\+\+ )/m.test(text)) return true;
  return /^[+](?![+])/m.test(text) && /^[-](?![-\d])/m.test(text);
}

/** Tokenize source with the same grammars as the file preview. Plain text stays null. */
export function highlightSourceTokens(text: string, format: SourceFormat): SourceToken[] | null {
  if (!text || format === 'text' || text.length > HIGHLIGHT_MAX_CHARS) return null;
  const parser = sourceLanguageParser(format);
  if (!parser) return null;
  const tree = parser.parse(text) as Parameters<typeof highlightTree>[0];
  const tokens: SourceToken[] = [];
  let pos = 0;
  highlightTree(tree, classHighlighter, (from, to, classes) => {
    if (from > pos) tokens.push({ text: text.slice(pos, from) });
    if (to > from) tokens.push({ text: text.slice(from, to), className: classes });
    pos = to;
  });
  if (pos < text.length) tokens.push({ text: text.slice(pos) });
  return tokens.some((token) => token.className) ? tokens : null;
}

function tokensByLine(text: string, format: SourceFormat): SourceToken[][] | null {
  const tokens = highlightSourceTokens(text, format);
  if (!tokens) return text.length === 0 ? [[]] : null;
  const lines: SourceToken[][] = [[]];
  for (const token of tokens) {
    const parts = token.text.split('\n');
    parts.forEach((part, index) => {
      if (index > 0) lines.push([]);
      if (part) lines[lines.length - 1].push({ text: part, className: token.className });
    });
  }
  return lines;
}

type DiffKind = 'add' | 'remove' | 'context' | 'meta';

function classifyDiffLine(line: string): { kind: DiffKind; prefix: string; code: string } {
  if (
    line.startsWith('+++')
    || line.startsWith('---')
    || line.startsWith('diff ')
    || line.startsWith('index ')
    || line.startsWith('@@')
  ) {
    return { kind: 'meta', prefix: '', code: line };
  }
  if (line.startsWith('+')) return { kind: 'add', prefix: '+', code: line.slice(1) };
  if (line.startsWith('-')) return { kind: 'remove', prefix: '-', code: line.slice(1) };
  if (line.startsWith(' ')) return { kind: 'context', prefix: ' ', code: line.slice(1) };
  return { kind: 'context', prefix: '', code: line };
}

/**
 * Color `+` / `-` and, when the path has a language, the keywords inside the line.
 * Falls back to diff-mode line colors when the language cannot be parsed.
 */
export function highlightDiffTokens(text: string, fileName?: string | null): SourceToken[] | null {
  const codeFormat = inferSourceFormat({ text: '', fileName });
  if (codeFormat !== 'text' && codeFormat !== 'diff') {
    const coded = highlightDiffCode(text, codeFormat);
    if (coded) return coded;
  }
  return highlightSourceTokens(text, 'diff');
}

function highlightDiffCode(text: string, format: SourceFormat): SourceToken[] | null {
  const rawLines = text.split('\n');
  const classified = rawLines.map(classifyDiffLine);
  const oldParts: string[] = [];
  const newParts: string[] = [];
  const indexOf = classified.map((line) => {
    if (line.kind === 'context') {
      const pos = { old: oldParts.length, neu: newParts.length };
      oldParts.push(line.code);
      newParts.push(line.code);
      return pos;
    }
    if (line.kind === 'remove') {
      const pos = { old: oldParts.length, neu: -1 };
      oldParts.push(line.code);
      return pos;
    }
    if (line.kind === 'add') {
      const pos = { old: -1, neu: newParts.length };
      newParts.push(line.code);
      return pos;
    }
    return { old: -1, neu: -1 };
  });
  const oldLines = oldParts.length > 0 ? tokensByLine(oldParts.join('\n'), format) : [];
  const newLines = newParts.length > 0 ? tokensByLine(newParts.join('\n'), format) : [];
  if (oldLines && oldLines.length < oldParts.length) return null;
  if (newLines && newLines.length < newParts.length) return null;
  if (!oldLines && !newLines) return null;

  const out: SourceToken[] = [];
  classified.forEach((line, index) => {
    if (index > 0) out.push({ text: '\n' });
    if (line.kind === 'meta') {
      out.push({ text: line.code, className: 'tok-meta' });
      return;
    }
    if (line.prefix) {
      out.push({
        text: line.prefix,
        className: line.kind === 'add' ? 'tok-inserted' : line.kind === 'remove' ? 'tok-deleted' : undefined,
      });
    }
    const codeTokens = line.kind === 'remove'
      ? oldLines?.[indexOf[index].old]
      : newLines?.[indexOf[index].neu];
    if (codeTokens && codeTokens.length > 0) out.push(...codeTokens);
    else if (line.code) out.push({ text: line.code });
  });
  return out.some((token) => token.className) ? out : null;
}

/** Highlight a detail snippet. JSON stays with the editor preview. */
export function highlightDetailTokens(text: string, fileName?: string | null): SourceToken[] | null {
  if (!text.trim() || looksLikeJsonObject(text)) return null;
  if (looksLikeDiff(text)) return highlightDiffTokens(text, fileName);
  const format = inferSourceFormat({ text, fileName });
  if (format === 'text') return null;
  return highlightSourceTokens(text, format);
}
