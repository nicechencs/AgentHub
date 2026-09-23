import { json } from '@codemirror/lang-json';
import {
  HighlightStyle,
  StreamLanguage,
  bracketMatching,
  syntaxHighlighting,
} from '@codemirror/language';
import { EditorView } from '@uiw/react-codemirror';
import { css } from '@codemirror/legacy-modes/mode/css';
import { diff } from '@codemirror/legacy-modes/mode/diff';
import { dockerFile } from '@codemirror/legacy-modes/mode/dockerfile';
import { go } from '@codemirror/legacy-modes/mode/go';
import { javascript, typescript } from '@codemirror/legacy-modes/mode/javascript';
import { powerShell } from '@codemirror/legacy-modes/mode/powershell';
import { properties } from '@codemirror/legacy-modes/mode/properties';
import { python } from '@codemirror/legacy-modes/mode/python';
import { ruby } from '@codemirror/legacy-modes/mode/ruby';
import { rust } from '@codemirror/legacy-modes/mode/rust';
import { shell } from '@codemirror/legacy-modes/mode/shell';
import { standardSQL } from '@codemirror/legacy-modes/mode/sql';
import { toml } from '@codemirror/legacy-modes/mode/toml';
import { c, cpp, java } from '@codemirror/legacy-modes/mode/clike';
import { html, xml } from '@codemirror/legacy-modes/mode/xml';
import { yaml } from '@codemirror/legacy-modes/mode/yaml';
import { tags as t } from '@lezer/highlight';
import type { SourceFormat } from '@/lib/source-preview';

/**
 * Highlight colors use design tokens so the editor follows light/dark
 * with the rest of the app instead of CodeMirror's VS Code palette.
 */
/**
 * Keyword, string, number, and names stay apart.
 * The same roles are repeated as `.tok-*` in globals.css for inline snippets.
 */
const sourceHighlight = HighlightStyle.define([
  { tag: t.keyword, color: 'var(--danger)', fontWeight: '600' },
  { tag: t.propertyName, color: 'var(--accent)', fontWeight: '600' },
  { tag: t.attributeName, color: 'var(--accent)' },
  { tag: t.string, color: 'var(--info)' },
  { tag: t.special(t.string), color: 'var(--info)' },
  { tag: t.regexp, color: 'var(--info)' },
  { tag: t.number, color: 'var(--accent-text)' },
  { tag: t.bool, color: 'var(--success)' },
  { tag: t.atom, color: 'var(--success)' },
  { tag: t.null, color: 'var(--text-muted)', fontStyle: 'italic' },
  { tag: t.comment, color: 'var(--text-muted)', fontStyle: 'italic' },
  { tag: t.lineComment, color: 'var(--text-muted)', fontStyle: 'italic' },
  { tag: t.punctuation, color: 'var(--text-secondary)' },
  { tag: t.bracket, color: 'var(--text-secondary)' },
  { tag: t.squareBracket, color: 'var(--text-secondary)' },
  { tag: t.brace, color: 'var(--text-secondary)' },
  { tag: t.separator, color: 'var(--text-secondary)' },
  { tag: t.operator, color: 'var(--text-secondary)' },
  { tag: t.definition(t.variableName), color: 'var(--accent)' },
  { tag: t.function(t.variableName), color: 'var(--accent)' },
  { tag: t.typeName, color: 'var(--warning)' },
  { tag: t.className, color: 'var(--warning)' },
  { tag: t.tagName, color: 'var(--success)' },
  { tag: t.inserted, color: 'var(--success)' },
  { tag: t.deleted, color: 'var(--danger)' },
  { tag: t.meta, color: 'var(--info)' },
  { tag: t.invalid, color: 'var(--danger)' },
]);

export function sourceLanguageParser(format: SourceFormat) {
  const [ext] = languageExtension(format);
  if (!ext || typeof ext !== 'object') return null;
  const record = ext as {
    parser?: { parse: (input: string) => unknown };
    language?: { parser?: { parse: (input: string) => unknown } };
  };
  return record.language?.parser ?? record.parser ?? null;
}

function languageExtension(format: SourceFormat) {
  switch (format) {
    case 'json':
      return [json()];
    case 'toml':
      return [StreamLanguage.define(toml)];
    case 'yaml':
      return [StreamLanguage.define(yaml)];
    case 'javascript':
      return [StreamLanguage.define(javascript)];
    case 'typescript':
      return [StreamLanguage.define(typescript)];
    case 'python':
      return [StreamLanguage.define(python)];
    case 'rust':
      return [StreamLanguage.define(rust)];
    case 'go':
      return [StreamLanguage.define(go)];
    case 'java':
      return [StreamLanguage.define(java)];
    case 'c':
      return [StreamLanguage.define(c)];
    case 'cpp':
      return [StreamLanguage.define(cpp)];
    case 'css':
      return [StreamLanguage.define(css)];
    case 'html':
      return [StreamLanguage.define(html)];
    case 'xml':
      return [StreamLanguage.define(xml)];
    case 'sql':
      return [StreamLanguage.define(standardSQL)];
    case 'shell':
      return [StreamLanguage.define(shell)];
    case 'powershell':
      return [StreamLanguage.define(powerShell)];
    case 'dockerfile':
      return [StreamLanguage.define(dockerFile)];
    case 'diff':
      return [StreamLanguage.define(diff)];
    case 'ruby':
      return [StreamLanguage.define(ruby)];
    case 'properties':
      return [StreamLanguage.define(properties)];
    case 'text':
    default:
      return [];
  }
}

export function sourcePreviewExtensions(format: SourceFormat) {
  return [
    ...languageExtension(format),
    syntaxHighlighting(sourceHighlight),
    bracketMatching(),
  ];
}

/** Snippets size to the document so empty/short JSON does not paint leftover line numbers. */
export const sourcePreviewFitContentTheme = EditorView.theme({
  '&': {
    maxWidth: '100%',
  },
  '& .cm-scroller': {
    height: 'auto !important',
    maxWidth: '100%',
  },
  '& .cm-content': {
    minHeight: '0px !important',
  },
  '& .cm-gutters': {
    height: 'auto',
  },
  '& .cm-gutter': {
    minHeight: '0px',
  },
});

export const SOURCE_PREVIEW_CHROME = [
  '[&_.cm-editor]:w-full [&_.cm-editor]:max-w-full [&_.cm-editor]:bg-canvas [&_.cm-editor]:font-mono [&_.cm-editor]:text-meta [&_.cm-editor]:text-primary',
  '[&_.cm-gutters]:bg-canvas [&_.cm-gutters]:text-muted [&_.cm-gutters]:border-border',
  '[&_.cm-activeLine]:bg-hover [&_.cm-activeLineGutter]:bg-hover',
  '[&_.cm-matchingBracket]:bg-hover',
  '[&_.cm-cursor]:border-primary',
  '[&_.cm-foldGutter]:text-muted',
  // Explicit reveal target (chat file open with line) — stronger than activeLine.
  '[&_.cm-line.ah-source-line-target]:bg-accent/15',
  '[&_.cm-gutters_.ah-source-line-target]:bg-accent/15',
].join(' ');
