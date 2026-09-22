import {
  useEffect,
  useId,
  useRef,
  useState,
} from 'react';
import { ChevronLeft, Code2, Eye, PanelRightClose } from 'lucide-react';
import {
  MarkdownView,
  isMarkdownFilePath,
  localParentDir,
  type MarkdownOpenLocalOptions,
} from '@/components/shared/MarkdownView';
import { SourcePreview } from '@/components/shared/SourcePreview';
import { CopyableFileName } from '@/components/shared/CopyableFileName';
import { previewHeaderParts } from '@/components/shared/file-name-label';
import { OpenDirButton } from '@/components/shared/OpenDirButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { Tip } from '@/components/ui/tooltip';
import { segmentedItemClass, segmentedTrackClass } from '@/components/ui/segmented-styles';
import { readMarkdownPreview } from '@/lib/api/chat';
import { openLocalPath } from '@/lib/open-external';
import { CHAT_FILE_PREVIEW_MAX_CHARS } from '@/lib/source-preview';
import { hasEscPriorityOverlay } from '@/lib/skills/preview-keys';
import { cn } from '@/lib/utils';
import { isPreviewableChatFilePath } from './chat-file-preview';

function fileName(path: string): string {
  const parts = path.trim().split(/[/\\]/).filter(Boolean);
  return parts[parts.length - 1] ?? path.trim();
}

function PreviewSkeleton() {
  return (
    <div className="space-y-3 py-1" aria-hidden>
      <div className="h-5 w-2/5 max-w-[12rem] animate-pulse rounded-btn bg-hover" />
      <div className="h-3.5 w-full animate-pulse rounded-btn bg-hover/80" />
      <div className="h-3.5 w-[92%] animate-pulse rounded-btn bg-hover/80" />
      <div className="h-3.5 w-[88%] animate-pulse rounded-btn bg-hover/70" />
      <div className="mt-4 h-3.5 w-1/3 max-w-[8rem] animate-pulse rounded-btn bg-hover/60" />
      <div className="h-3.5 w-full animate-pulse rounded-btn bg-hover/70" />
    </div>
  );
}

export function ChatMarkdownPreviewPanel({
  path,
  cwd,
  open,
  width,
  line,
  onClose,
  onBack,
  onOpenLocal,
  canBack = false,
  className,
}: {
  path: string;
  cwd: string;
  open: boolean;
  width?: number;
  /** 1-based line to highlight after load (from chat file URL). */
  line?: number;
  canBack?: boolean;
  onBack?: () => void;
  onClose: () => void;
  onOpenLocal: (nextPath: string, options?: MarkdownOpenLocalOptions) => void;
  className?: string;
}) {
  const { t } = useI18n();
  const titleId = useId();
  const requestSeq = useRef(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [content, setContent] = useState('');
  const [resolvedPath, setResolvedPath] = useState(path);
  const [name, setName] = useState(fileName(path));
  const [truncated, setTruncated] = useState(false);
  const markdown = isMarkdownFilePath(path);
  const [mode, setMode] = useState<'preview' | 'source'>(markdown ? 'preview' : 'source');

  useEffect(() => {
    setMode(isMarkdownFilePath(path) ? (line && line > 0 ? 'source' : 'preview') : 'source');
  }, [path, line]);

  useEffect(() => {
    if (!open || !path) return;
    const seq = ++requestSeq.current;
    setLoading(true);
    setError(null);
    setName(fileName(path));
    setContent('');
    setTruncated(false);

    const workingDir = cwd.trim();
    if (!workingDir) {
      setError(t('chat.header.cwdUnset'));
      setLoading(false);
      return;
    }

    void readMarkdownPreview(path, workingDir)
      .then((row) => {
        if (requestSeq.current !== seq) return;
        setContent(row.content);
        setResolvedPath(row.path);
        setName(row.name || fileName(path));
        setTruncated(row.truncated);
      })
      .catch((err) => {
        if (requestSeq.current !== seq) return;
        setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (requestSeq.current === seq) setLoading(false);
      });
  }, [open, path, cwd, t]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      if (hasEscPriorityOverlay()) return;
      e.preventDefault();
      if (canBack && onBack) onBack();
      else onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, canBack, onBack, onClose]);

  if (!open) return null;

  const folder = localParentDir(resolvedPath || path);
  const showModeToggle = markdown;

  const reload = () => {
    const seq = ++requestSeq.current;
    const workingDir = cwd.trim();
    setLoading(true);
    setError(null);
    if (!workingDir) {
      setError(t('chat.header.cwdUnset'));
      setLoading(false);
      return;
    }
    void readMarkdownPreview(path, workingDir)
      .then((row) => {
        if (requestSeq.current !== seq) return;
        setContent(row.content);
        setResolvedPath(row.path);
        setName(row.name || fileName(path));
        setTruncated(row.truncated);
      })
      .catch((err) => {
        if (requestSeq.current !== seq) return;
        setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (requestSeq.current === seq) setLoading(false);
      });
  };

  return (
    <aside
      className={cn(
        'flex h-full min-h-0 min-w-0 shrink-0 flex-col overflow-hidden rounded-card border border-border bg-panel shadow-xs',
        className,
      )}
      style={width != null ? { width } : undefined}
      aria-labelledby={titleId}
      data-chat-file-preview
      data-preview-line={line ?? undefined}
    >
      <header className="shrink-0 border-b border-border">
        <div className="flex h-10 items-center gap-1.5 overflow-x-auto px-3">
          {canBack ? (
            <Button
              size="icon"
              variant="ghost"
              className="h-7 w-7 shrink-0"
              aria-label={t('chat.preview.back')}
              title={t('chat.preview.back')}
              onClick={onBack}
            >
              <ChevronLeft className="h-4 w-4" />
            </Button>
          ) : null}
          <div className="min-w-0 flex-1 basis-16">
            <Tip
              label={resolvedPath || path}
              className="inline-flex min-w-0 max-w-full items-baseline gap-2"
            >
              <h2
                id={titleId}
                className="max-w-[70%] shrink-0 truncate text-sm font-semibold leading-tight text-primary"
              >
                {name || t('chat.preview.titleFallback')}
              </h2>
              {folder ? (
                <span className="min-w-0 truncate text-meta text-muted">
                  {previewHeaderParts(resolvedPath || path, name).directoryLabel}
                </span>
              ) : null}
              {line && line > 0 ? (
                <span className="shrink-0 text-meta text-muted" data-preview-line-label>
                  :{line}
                </span>
              ) : null}
            </Tip>
          </div>
          {showModeToggle ? (
            <div className={cn(segmentedTrackClass, 'shrink-0 flex-nowrap')}>
              <button
                type="button"
                className={cn(segmentedItemClass(mode === 'preview', 'sm'), 'h-6 gap-1 px-2')}
                onClick={() => setMode('preview')}
              >
                <Eye className="h-3.5 w-3.5" />
                {t('chat.preview.modePreview')}
              </button>
              <button
                type="button"
                className={cn(segmentedItemClass(mode === 'source', 'sm'), 'h-6 gap-1 px-2')}
                onClick={() => setMode('source')}
              >
                <Code2 className="h-3.5 w-3.5" />
                {t('chat.preview.modeSource')}
              </button>
            </div>
          ) : null}
          {folder ? (
            <OpenDirButton
              title={t('chat.preview.openDir')}
              onClick={() => {
                void openLocalPath(folder).catch(() => {});
              }}
            />
          ) : null}
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7 shrink-0"
            aria-label={t('chat.preview.collapse')}
            title={t('chat.preview.collapse')}
            onClick={onClose}
          >
            <PanelRightClose className="h-4 w-4" />
          </Button>
        </div>
      </header>

      <div className="relative h-px shrink-0 bg-border" aria-hidden={!loading}>
        {loading ? <div className="absolute inset-y-0 left-0 w-1/3 animate-pulse bg-accent/70" /> : null}
      </div>

      <div
        className={cn(
          'min-h-0 min-w-0 flex-1',
          mode === 'preview' && markdown ? 'overflow-auto px-4 py-3' : 'overflow-hidden p-0',
        )}
        aria-busy={loading}
      >
        {loading ? (
          <div className="px-4 py-3">
            <PreviewSkeleton />
          </div>
        ) : error ? (
          <div className="space-y-2 px-4 py-6">
            <p className="text-sm font-medium text-primary">{name}</p>
            <p className="text-sm text-danger">{t('chat.preview.failed')}</p>
            <p className="text-meta text-secondary">{error}</p>
            <Button size="sm" variant="secondary" onClick={reload}>
              {t('chat.preview.retry')}
            </Button>
          </div>
        ) : mode === 'preview' && markdown ? (
          content.trim() ? (
            <MarkdownView
              content={content}
              variant="document"
              localBasePath={folder || cwd}
              onOpenLocal={(next, options) => {
                if (!isPreviewableChatFilePath(next)) return false;
                onOpenLocal(next, options);
                return true;
              }}
            />
          ) : (
            <p className="py-6 text-sm text-muted">{t('chat.preview.emptyBody')}</p>
          )
        ) : content.trim() ? (
          <SourcePreview
            value={content}
            fileName={name}
            readOnly
            pretty={false}
            compressBlankLines={false}
            density="document"
            highlightLine={line}
            maxChars={CHAT_FILE_PREVIEW_MAX_CHARS}
            className="h-full rounded-none"
          />
        ) : (
          <p className="px-4 py-6 text-sm text-muted">{t('chat.preview.emptyBody')}</p>
        )}
      </div>

      <footer className="flex shrink-0 items-center gap-2 border-t border-border px-3 py-1.5">
        <CopyableFileName path={resolvedPath || path} className="min-w-0 flex-1" />
        {truncated ? (
          <span className="shrink-0 text-meta text-muted">{t('chat.preview.truncatedSuffix')}</span>
        ) : null}
      </footer>
    </aside>
  );
}
