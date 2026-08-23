import {
  useEffect,
  useId,
  useRef,
  useState,
  type RefObject,
  type TransitionEvent as ReactTransitionEvent,
} from 'react';
import { Code2, Eye, FolderOpen, PanelRightClose } from 'lucide-react';
import { MarkdownView } from '@/components/shared/MarkdownView';
import { Button } from '@/components/ui/button';
import { segmentedItemClass, segmentedTrackClass } from '@/components/ui/segmented-styles';
import { Tip } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { agentDisplayName } from '@/config/agents';
import { readSkillMarkdown } from '@/lib/api/skill';
import {
  hasEscPriorityOverlay,
  skillPreviewActiveKey,
} from '@/lib/skills/preview-keys';
import { splitSkillMarkdown } from '@/lib/skills/skill-markdown';
import type { AgentId } from '@/lib/types';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { cn } from '@/lib/utils';

export type SkillPreviewTarget = {
  skillId: string;
  name?: string;
  /** Private agent skill root; omit for shared library. */
  privateAgent?: AgentId | null;
  /** Optional dir path for “open folder”. */
  sourceDir?: string | null;
};

function PreviewSkeleton() {
  return (
    <div className="space-y-3 py-1" aria-hidden>
      <div className="h-5 w-2/5 max-w-[12rem] animate-pulse rounded-btn bg-hover" />
      <div className="h-3.5 w-full animate-pulse rounded-btn bg-hover/80" />
      <div className="h-3.5 w-[92%] animate-pulse rounded-btn bg-hover/80" />
      <div className="h-3.5 w-[88%] animate-pulse rounded-btn bg-hover/70" />
      <div className="mt-4 h-3.5 w-1/3 max-w-[8rem] animate-pulse rounded-btn bg-hover/60" />
      <div className="h-3.5 w-full animate-pulse rounded-btn bg-hover/70" />
      <div className="h-3.5 w-[85%] animate-pulse rounded-btn bg-hover/60" />
    </div>
  );
}

/**
 * Right-side SKILL.md preview pane (fills parent height; width controlled by parent).
 * Open via name click / Enter / context menu; collapse with header button or Esc
 * (Esc yields to open Dialog / Menu).
 */
export function SkillMarkdownPreviewPanel({
  target,
  open,
  onClose,
  onOpenDir,
  width,
  className,
  contentRef,
  onWidthTransitionEnd,
}: {
  target: SkillPreviewTarget | null;
  open: boolean;
  onClose: () => void;
  onOpenDir?: (path: string) => void;
  /** Pixel width when open (parent + resize handle own the split). */
  width?: number;
  className?: string;
  /** Optional ref for keyboard focus into the document pane. */
  contentRef?: RefObject<HTMLDivElement | null>;
  /** Parent uses this to unmount after close width animation. */
  onWidthTransitionEnd?: (e: ReactTransitionEvent<HTMLElement>) => void;
}) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [content, setContent] = useState('');
  const [path, setPath] = useState<string | null>(null);
  const [name, setName] = useState('');
  const [truncated, setTruncated] = useState(false);
  const [mode, setMode] = useState<'preview' | 'source'>('preview');
  const { t } = useI18n();
  const requestSeq = useRef(0);
  const titleId = useId();
  const bodyRef = useRef<HTMLDivElement>(null);
  const resolvedBodyRef = contentRef ?? bodyRef;

  const activeKey = target ? skillPreviewActiveKey(target) : null;

  useEffect(() => {
    if (!open || !target || !activeKey) return;

    const seq = ++requestSeq.current;
    const expectedKey = activeKey;
    const displayName = target.name ?? target.skillId;

    // Header switches immediately; body becomes skeleton (no stale body + new title).
    setLoading(true);
    setError(null);
    setContent('');
    setPath(null);
    setName(displayName);
    setTruncated(false);
    // Mode is session-stable across skill switches (not reset here).

    void readSkillMarkdown(target.skillId, target.privateAgent ?? null)
      .then((row) => {
        if (requestSeq.current !== seq) return;
        // Guard against stale responses if parent swaps target mid-flight.
        if (skillPreviewActiveKey(target) !== expectedKey) return;
        setContent(row.content);
        setPath(row.path);
        setName(row.name || displayName);
        setTruncated(row.truncated);
        setError(null);
      })
      .catch((e) => {
        if (requestSeq.current !== seq) return;
        if (skillPreviewActiveKey(target) !== expectedKey) return;
        setError(e instanceof Error ? e.message : String(e));
        setContent('');
      })
      .finally(() => {
        if (requestSeq.current === seq) setLoading(false);
      });
  }, [open, activeKey, target]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      if (hasEscPriorityOverlay()) return;
      e.preventDefault();
      onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open || !target) return null;

  const openFolderPath =
    target.sourceDir ?? (path ? path.replace(/[\\/][^\\/]+$/, '') : null);
  const originLabel = target.privateAgent
    ? agentDisplayName(target.privateAgent)
    : t('skills.preview.sharedOrigin');
  // 预览态剥 YAML frontmatter，避免 --- / name: / description: 被渲染成假正文
  const mdParts = !loading && !error && content ? splitSkillMarkdown(content) : null;
  const previewBody = mdParts?.body ?? '';
  const previewDescription = mdParts?.description?.trim() || null;

  return (
    <aside
      className={cn(
        // 卡片面：圆角 + 边框 + 轻阴影；min-w-0 允许被父级压窄
        'flex h-full min-h-0 min-w-0 shrink-0 flex-col overflow-hidden rounded-card border border-border bg-panel shadow-xs',
        className,
      )}
      style={width != null ? { width } : undefined}
      aria-labelledby={titleId}
      onTransitionEnd={onWidthTransitionEnd}
    >
      {/* Single-row chrome；过窄时横向滚动工具区，避免按钮被裁切 */}
      <header className="flex h-10 shrink-0 items-center gap-1.5 overflow-x-auto border-b border-border px-3">
        <div className="min-w-0 flex-1 basis-16">
          <div className="flex min-w-0 items-baseline gap-2">
            <h2
              id={titleId}
              className="truncate text-sm font-semibold leading-tight text-primary"
            >
              {name || t('skills.preview.titleFallback')}
            </h2>
            <span className="shrink-0 text-meta text-muted">{originLabel}</span>
          </div>
        </div>

        {/* chrome 特例：比页内筛选 sm 更扁（h-6），不与列表筛选同高 */}
        <div className={cn(segmentedTrackClass, 'shrink-0 flex-nowrap')}>
          <button
            type="button"
            className={cn(segmentedItemClass(mode === 'preview', 'sm'), 'h-6 gap-1 px-2')}
            onClick={() => setMode('preview')}
          >
            <Eye className="h-3 w-3" />
            {t('skills.preview.modePreview')}
          </button>
          <button
            type="button"
            className={cn(segmentedItemClass(mode === 'source', 'sm'), 'h-6 gap-1 px-2')}
            onClick={() => setMode('source')}
          >
            <Code2 className="h-3 w-3" />
            {t('skills.preview.modeSource')}
          </button>
        </div>

        {openFolderPath && onOpenDir ? (
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7 shrink-0"
            title={t('skills.preview.openDir')}
            aria-label={t('skills.preview.openDir')}
            onClick={() => onOpenDir(openFolderPath)}
          >
            <FolderOpen className="h-3.5 w-3.5" />
          </Button>
        ) : null}

        <Button
          size="icon"
          variant="ghost"
          className="h-7 w-7 shrink-0"
          aria-label={t('skills.preview.collapse')}
          title={t('skills.preview.collapse')}
          onClick={onClose}
        >
          <PanelRightClose className="h-4 w-4" />
        </Button>
      </header>

      {/* Top 1px progress while loading */}
      <div
        className="relative h-px shrink-0 bg-border"
        aria-hidden={!loading}
      >
        {loading ? (
          <div className="absolute inset-y-0 left-0 w-1/3 animate-pulse bg-accent/70" />
        ) : null}
      </div>

      {/* 双向滚动：宽表/代码块可横向滑，不再被 overflow-hidden 裁切 */}
      <div
        ref={resolvedBodyRef as RefObject<HTMLDivElement>}
        tabIndex={-1}
        className="min-h-0 min-w-0 flex-1 overflow-auto px-4 py-3 outline-none"
        aria-busy={loading}
      >
        {loading ? (
          <PreviewSkeleton />
        ) : error ? (
          <div className="space-y-2 py-6">
            <p className="text-sm font-medium text-primary">{name}</p>
            <p className="text-sm text-danger">{error}</p>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => {
                // Re-trigger effect by bumping seq via state nudge on parent key;
                // local re-fetch:
                const t = target;
                const seq = ++requestSeq.current;
                const expectedKey = skillPreviewActiveKey(t);
                setLoading(true);
                setError(null);
                void readSkillMarkdown(t.skillId, t.privateAgent ?? null)
                  .then((row) => {
                    if (requestSeq.current !== seq) return;
                    if (skillPreviewActiveKey(t) !== expectedKey) return;
                    setContent(row.content);
                    setPath(row.path);
                    setName(row.name || t.name || t.skillId);
                    setTruncated(row.truncated);
                  })
                  .catch((e) => {
                    if (requestSeq.current !== seq) return;
                    setError(e instanceof Error ? e.message : String(e));
                  })
                  .finally(() => {
                    if (requestSeq.current === seq) setLoading(false);
                  });
              }}
            >
              {t('skills.preview.retry')}
            </Button>
          </div>
        ) : mode === 'preview' ? (
          <div className="min-w-0">
            {previewDescription ? (
              <div className="mb-3 border-b border-border/70 pb-3">
                <p className={pageRhythm.sectionEyebrow}>
                  {t('skills.preview.description')}
                </p>
                <p className="mt-1 text-sm leading-relaxed text-secondary">
                  {previewDescription}
                </p>
              </div>
            ) : null}
            {previewBody.trim() ? (
              <MarkdownView content={previewBody} variant="document" />
            ) : (
              <p className="py-6 text-sm text-muted">{t('skills.preview.emptyBody')}</p>
            )}
          </div>
        ) : (
          <pre className="min-w-0 overflow-x-auto whitespace-pre-wrap break-words rounded-card border border-border/60 bg-subtle p-3 font-mono text-xs leading-relaxed text-primary">
            {content}
          </pre>
        )}
      </div>

      <footer className="flex shrink-0 items-center gap-2 border-t border-border px-3 py-1.5">
        <Tip
          className="min-w-0 flex-1 truncate font-mono text-meta text-muted"
          label={path ?? target.sourceDir ?? undefined}
        >
          {path ?? target.sourceDir ?? target.skillId}
          {truncated ? t('skills.preview.truncatedSuffix') : ''}
        </Tip>
      </footer>
    </aside>
  );
}
