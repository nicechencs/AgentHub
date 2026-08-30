import { useMemo, useState } from 'react';
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

/**
 * Read-only (or small editor) JSON/TOML view: token colors, fold, line numbers.
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
  density = 'preview',
  className,
  id,
}: {
  value: string;
  format?: SourceFormat | string | null;
  fileName?: string | null;
  readOnly?: boolean;
  pretty?: boolean;
  onChange?: (value: string) => void;
  showCopy?: boolean;
  /** preview = compact snippet; editor = supplier advanced config. */
  density?: 'preview' | 'editor' | 'compact';
  className?: string;
  id?: string;
}) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);
  const format = inferSourceFormat({
    text: value,
    fileName,
    hint: formatHint,
  });
  const displayed = readOnly ? prepareSourcePreview(value, format, { pretty }) : value;
  const extensions = useMemo(() => sourcePreviewExtensions(format), [format]);
  const foldable = format === 'json' || format === 'toml';

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
      className={cn('min-w-0 overflow-hidden rounded-card border border-border bg-canvas', className)}
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
            {copied ? <Check className="h-3 w-3 text-success" /> : <Copy className="h-3 w-3" />}
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
          SOURCE_PREVIEW_CHROME,
        )}
      >
        <CodeMirror
          value={displayed}
          height="auto"
          minHeight={density === 'editor' ? '96px' : '0'}
          theme="none"
          editable={!readOnly}
          readOnly={readOnly}
          extensions={extensions}
          onChange={readOnly ? undefined : onChange}
          onCreateEditor={
            readOnly && format === 'json'
              ? (view) => {
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
              : undefined
          }
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
