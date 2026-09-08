import {
  useEffect,
  useId,
  useRef,
  useState,
} from 'react';
import { ChevronLeft, Code2, Eye, PanelRightClose } from 'lucide-react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { MarkdownView, isMarkdownFilePath, localParentDir } from '@/components/shared/MarkdownView';
import { CopyableFileName } from '@/components/shared/CopyableFileName';
import { OpenDirButton } from '@/components/shared/OpenDirButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { segmentedItemClass, segmentedTrackClass } from '@/components/ui/segmented-styles';
import { readMarkdownPreview } from '@/lib/api/chat';
import { openLocalPath } from '@/lib/open-external';
import { hasEscPriorityOverlay } from '@/lib/skills/preview-keys';
import { cn } from '@/lib/utils';

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
  canBack?: boolean;
  onBack?: () => void;
  onClose: () => void;
  onOpenLocal: (nextPath: string) => void;
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
  const [mode, setMode] = useState<'preview' | 'source'>('preview');

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

  return (
    <aside
      className={cn(
        pageRhythm.inspectPane,
        className,
      )}
      style={width != null ? { width } : undefined}
      aria-labelledby={titleId}
    >
      <header className={pageRhythm.inspectHeader}>
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
          <h2
            id={titleId}
            className={cn(pageRhythm.inspectTitle, 'flex-1')}
          >
            {name || t('chat.preview.titleFallback')}
          </h2>
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
      </header>

      <div className="relative h-px shrink-0 bg-border" aria-hidden={!loading}>
        {loading ? <div className="absolute inset-y-0 left-0 w-1/3 animate-pulse bg-accent/70" /> : null}
      </div>

      <div className="min-h-0 min-w-0 flex-1 overflow-auto px-3 py-2" aria-busy={loading}>
        {loading ? (
          <PreviewSkeleton />
        ) : error ? (
          <div className="space-y-2 py-6">
            <p className="text-sm font-medium text-primary">{name}</p>
            <p className="text-sm text-danger">{t('chat.preview.failed')}</p>
            <p className="text-meta text-secondary">{error}</p>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => {
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
              }}
            >
              {t('chat.preview.retry')}
            </Button>
          </div>
        ) : mode === 'preview' ? (
          content.trim() ? (
            <MarkdownView
              content={content}
              variant="document"
              localBasePath={folder || cwd}
              onOpenLocal={(next) => {
                if (!isMarkdownFilePath(next)) return false;
                onOpenLocal(next);
                return true;
              }}
            />
          ) : (
            <p className="py-6 text-sm text-muted">{t('chat.preview.emptyBody')}</p>
          )
        ) : (
          <pre className="min-w-0 overflow-x-auto whitespace-pre-wrap break-words rounded-card border border-border/60 bg-subtle p-3 font-mono text-xs leading-relaxed text-primary">
            {content}
          </pre>
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
