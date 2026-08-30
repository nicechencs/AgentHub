import { json } from '@codemirror/lang-json';
import {
  HighlightStyle,
  StreamLanguage,
  bracketMatching,
  syntaxHighlighting,
} from '@codemirror/language';
import { toml } from '@codemirror/legacy-modes/mode/toml';
import { tags as t } from '@lezer/highlight';
import type { SourceFormat } from '@/lib/source-preview';

/**
 * Highlight colors use design tokens so the editor follows light/dark
 * with the rest of the app instead of CodeMirror's VS Code palette.
 */
const sourceHighlight = HighlightStyle.define([
  { tag: t.propertyName, color: 'var(--accent)' },
  { tag: t.attributeName, color: 'var(--accent)' },
  { tag: t.keyword, color: 'var(--accent)' },
  { tag: t.string, color: 'var(--text-secondary)' },
  { tag: t.number, color: 'var(--info)' },
  { tag: t.bool, color: 'var(--success)' },
  { tag: t.atom, color: 'var(--success)' },
  { tag: t.null, color: 'var(--text-muted)', fontStyle: 'italic' },
  { tag: t.comment, color: 'var(--text-muted)', fontStyle: 'italic' },
  { tag: t.lineComment, color: 'var(--text-muted)', fontStyle: 'italic' },
  { tag: t.punctuation, color: 'var(--text-muted)' },
  { tag: t.bracket, color: 'var(--text-muted)' },
  { tag: t.squareBracket, color: 'var(--text-muted)' },
  { tag: t.brace, color: 'var(--text-muted)' },
  { tag: t.separator, color: 'var(--text-muted)' },
  { tag: t.invalid, color: 'var(--danger)' },
]);

function languageExtension(format: SourceFormat) {
  if (format === 'json') return [json()];
  if (format === 'toml') return [StreamLanguage.define(toml)];
  return [];
}

export function sourcePreviewExtensions(format: SourceFormat) {
  return [
    ...languageExtension(format),
    syntaxHighlighting(sourceHighlight),
    bracketMatching(),
  ];
}

export const SOURCE_PREVIEW_CHROME = [
  '[&_.cm-editor]:bg-canvas [&_.cm-editor]:font-mono [&_.cm-editor]:text-meta [&_.cm-editor]:text-primary [&_.cm-editor]:leading-relaxed',
  '[&_.cm-gutters]:bg-canvas [&_.cm-gutters]:text-muted [&_.cm-gutters]:border-border',
  '[&_.cm-activeLine]:bg-hover [&_.cm-activeLineGutter]:bg-hover',
  '[&_.cm-matchingBracket]:bg-hover',
  '[&_.cm-cursor]:border-primary',
  '[&_.cm-foldGutter]:text-muted',
].join(' ');
