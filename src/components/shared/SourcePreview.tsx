import { useEffect, useMemo, useRef, useState } from 'react';
import { Check, Copy } from 'lucide-react';
import CodeMirror from '@uiw/react-codemirror';
import { Button } from '@/components/ui/button';
import { useI18n } from '@/components/shared/LanguageProvider';
import {
  inferSourceFormat,
  prepareSourcePreview,
  type SourceFormat,
} from '@/lib/source-preview';
import { cn } from '@/lib/utils';
import { foldJsonBeyondDepth } from './source-preview-fold';
import { SOURCE_PREVIEW_CHROME, sourcePreviewExtensions } from './source-preview-theme';

type EditorViewLike = {
  state: {
    doc: {
      lines: number;
      line: (n: number) => { from: number; to: number; number: number };
    };
  };
  dispatch: (spec: {
    selection?: { anchor: number; head?: number };
  }) => void;
  lineBlockAt: (pos: number) => { top: number };
  scrollDOM: HTMLElement;
  contentDOM: HTMLElement;
};

/**
 * Read-only (or small editor) source view: token colors, fold, line numbers.
 * Does not redact; displays the text the caller already prepared.
 */
export function SourcePreview({
  value,
  format: formatHint,
  fileName,
  readOnly = true,
  pretty = readOnly,
  onChange,
  showCopy = false,
  compressBlankLines = true,
  density = 'preview',
  className,
  id,
  highlightLine,
  maxChars,
}: {
  value: string;
  format?: SourceFormat | string | null;
  fileName?: string | null;
  readOnly?: boolean;
  pretty?: boolean;
  onChange?: (value: string) => void;
  showCopy?: boolean;
  /** Read-only JSON details drop blank lines. Editable buffers keep authored spacing. */
  compressBlankLines?: boolean;
  /** preview = compact snippet; editor = supplier advanced config; document = chat file pane. */
  density?: 'preview' | 'editor' | 'compact' | 'document';
  className?: string;
  id?: string;
  /** 1-based line to select, scroll into view, and mark (chat file open). */
  highlightLine?: number | null;
  /** Override clip length (chat file preview uses a larger cap). */
  maxChars?: number;
}) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);
  const editorRef = useRef<EditorViewLike | null>(null);
  const format = inferSourceFormat({
    text: value,
    fileName,
    hint: formatHint,
  });
  const displayed = readOnly
    ? prepareSourcePreview(value, format, { pretty, compressBlankLines, maxChars })
    : value;
  const extensions = useMemo(() => sourcePreviewExtensions(format), [format]);
  const foldable = format === 'json' || format === 'toml';

  const revealLine = (view: EditorViewLike, line: number) => {
    if (line < 1 || line > view.state.doc.lines) return;
    const row = view.state.doc.line(line);
    view.dispatch({ selection: { anchor: row.from, head: row.to } });
    const block = view.lineBlockAt(row.from);
    view.scrollDOM.scrollTop = Math.max(0, block.top - 48);
    view.contentDOM.querySelectorAll('.ah-source-line-target').forEach((el) => {
      el.classList.remove('ah-source-line-target');
    });
    const lineEl = view.contentDOM.querySelector(`.cm-line:nth-child(${line})`);
    lineEl?.classList.add('ah-source-line-target');
    const gutter = view.scrollDOM.querySelector(`.cm-gutterElement:nth-child(${line + 1})`);
    gutter?.classList.add('ah-source-line-target');
  };

  useEffect(() => {
    const view = editorRef.current;
    if (!view || highlightLine == null || highlightLine < 1) return;
    window.requestAnimationFrame(() => revealLine(view, highlightLine));
  }, [highlightLine, displayed]);

  const onCopy = () => {
    if (!displayed.trim()) return;
    void navigator.clipboard.writeText(displayed).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    }).catch(() => {});
  };

  return (
    <div
      id={id}
      className={cn(
        'min-w-0 overflow-hidden rounded-card border border-border bg-canvas text-primary',
        density === 'document' && 'h-full border-0 bg-transparent',
        className,
      )}
      data-highlight-line={highlightLine ?? undefined}
    >
      {showCopy ? (
        <div className="flex justify-end border-b border-border px-1.5 py-0.5">
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="h-7 shrink-0 px-2"
            title={t('common.copy')}
            aria-label={t('common.copy')}
            onClick={onCopy}
          >
            {copied ? <Check className="h-3.5 w-3.5 text-success" /> : <Copy className="h-3.5 w-3.5" />}
            {copied ? t('common.copied') : t('common.copy')}
          </Button>
        </div>
      ) : null}
      <div
        className={cn(
          'overflow-auto',
          density === 'editor' && 'max-h-80 min-h-24 [&_.cm-editor]:min-h-24',
          density === 'preview' && 'max-h-64',
          density === 'compact' && 'max-h-36',
          density === 'document' && 'h-full max-h-none',
          density === 'compact' ? '[&_.cm-editor]:leading-snug' : '[&_.cm-editor]:leading-relaxed',
          SOURCE_PREVIEW_CHROME,
        )}
      >
        <CodeMirror
          value={displayed}
          height={density === 'document' ? '100%' : 'auto'}
          minHeight={density === 'editor' ? '96px' : '0'}
          theme="none"
          editable={!readOnly}
          readOnly={readOnly}
          extensions={extensions}
          onChange={readOnly ? undefined : onChange}
          onCreateEditor={(view) => {
            editorRef.current = view as unknown as EditorViewLike;
            if (readOnly && format === 'json') {
              const fold = () =>
                foldJsonBeyondDepth({
                  state: view.state,
                  dispatch: (spec) => view.dispatch(spec),
                });
              window.requestAnimationFrame(() => {
                fold();
                window.requestAnimationFrame(fold);
              });
            }
            if (highlightLine != null && highlightLine > 0) {
              window.requestAnimationFrame(() => {
                revealLine(view as unknown as EditorViewLike, highlightLine);
              });
            }
          }}
          basicSetup={{
            lineNumbers: true,
            foldGutter: foldable,
            highlightActiveLine: true,
            highlightSelectionMatches: false,
            autocompletion: false,
            bracketMatching: true,
            closeBrackets: !readOnly,
            tabSize: 2,
            syntaxHighlighting: false,
            history: !readOnly,
            dropCursor: !readOnly,
          }}
        />
      </div>
    </div>
  );
}
