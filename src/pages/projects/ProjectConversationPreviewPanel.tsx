import { useEffect, useId, useRef, useState, type RefObject } from 'react';
import { FolderOpen, MessageSquarePlus, PanelRightClose } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { useI18n } from '@/components/shared/LanguageProvider';
import { MarkdownView } from '@/components/shared/MarkdownView';
import { Button } from '@/components/ui/button';
import { Tip } from '@/components/ui/tooltip';
import { AGENT_MAP } from '@/config/agents';
import { getAgentProjectExcerpts } from '@/lib/api/project';
import { normalizeOpenPath } from '@/lib/path-open';
import { hasEscPriorityOverlay } from '@/lib/skills/preview-keys';
import type { AgentSession } from '@/lib/types';
import { cn } from '@/lib/utils';
import { relativeTime, shortPath } from './project-format';
import { classifyExcerptRows, splitExcerptTurns } from './session-excerpt';

function PreviewSkeleton() {
  return (
    <div className="space-y-3 py-1" aria-hidden>
      <div className="ml-auto h-12 w-4/5 max-w-[18rem] animate-pulse rounded-composer bg-hover" />
      <div className="h-16 w-[92%] animate-pulse rounded-btn bg-hover/80" />
      <div className="ml-auto h-10 w-3/5 max-w-[14rem] animate-pulse rounded-composer bg-hover/70" />
      <div className="h-20 w-[88%] animate-pulse rounded-btn bg-hover/60" />
    </div>
  );
}

export function ProjectConversationPreviewPanel({
  session,
  open,
  onClose,
  onContinue,
  onOpenRecord,
  busy,
  width,
  className,
  contentRef,
}: {
  session: AgentSession;
  open: boolean;
  onClose: () => void;
  onContinue: (session: AgentSession) => void;
  onOpenRecord: (session: AgentSession) => void;
  busy?: boolean;
  width?: number;
  className?: string;
  contentRef?: RefObject<HTMLDivElement | null>;
}) {
  const { t } = useI18n();
  const titleId = useId();
  const requestSeq = useRef(0);
  const bodyRef = useRef<HTMLDivElement>(null);
  const resolvedBodyRef = contentRef ?? bodyRef;
  const [phase, setPhase] = useState<'loading' | 'ready' | 'empty' | 'error'>('loading');
  const [excerpt, setExcerpt] = useState('');

  const applyRows = (sessionId: string, rows: { id: string; excerpt?: string | null }[]) => {
    const result = classifyExcerptRows(sessionId, rows);
    if (result.status === 'ready') {
      setExcerpt(result.excerpt);
      setPhase('ready');
      return;
    }
    setExcerpt('');
    setPhase(result.status);
  };

  useEffect(() => {
    if (!open) return;
    const seq = ++requestSeq.current;
    const expectedId = session.id;
    setPhase('loading');
    setExcerpt('');
    void getAgentProjectExcerpts([session.id]).then(
      (rows) => {
        if (requestSeq.current !== seq) return;
        if (session.id !== expectedId) return;
        applyRows(expectedId, rows);
      },
      () => {
        if (requestSeq.current !== seq) return;
        setPhase('error');
      },
    );
  }, [open, session.id]);

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

  if (!open) return null;

  const turns = phase === 'ready' ? splitExcerptTurns(excerpt) : [];
  const agentMeta = AGENT_MAP[session.agentId];
  const record = normalizeOpenPath(session.path);
  const cwd = session.cwd?.trim() || null;

  return (
    <aside
      className={cn(
        'flex h-full min-h-0 min-w-0 shrink-0 flex-col overflow-hidden rounded-card border border-border bg-panel shadow-xs',
        className,
      )}
      style={width != null ? { width } : undefined}
      aria-labelledby={titleId}
    >
      <header className="flex h-10 shrink-0 items-center gap-1.5 overflow-x-auto border-b border-border px-3">
        <AgentDot agentId={session.agentId} color={agentMeta?.color} className="shrink-0" />
        <div className="min-w-0 flex-1 basis-16">
          <div className="flex min-w-0 items-baseline gap-2">
            <h2 id={titleId} className="truncate text-body font-semibold leading-tight text-primary">
              {session.title || t('projects.preview.titleFallback')}
            </h2>
            <span className="shrink-0 text-meta text-muted">
              {agentMeta?.name ?? session.agentId}
              {' · '}
              {relativeTime(session.updatedAt, t)}
            </span>
          </div>
        </div>
        <Button
          size="sm"
          variant="outline"
          className="shrink-0"
          disabled={busy}
          onClick={() => onContinue(session)}
        >
          <MessageSquarePlus className="h-3.5 w-3.5" />
          {t('projects.tree.continue')}
        </Button>
        {record ? (
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7 shrink-0"
            aria-label={t('projects.preview.openRecord')}
            title={t('projects.preview.openRecord')}
            onClick={() => onOpenRecord(session)}
          >
            <FolderOpen className="h-3.5 w-3.5" />
          </Button>
        ) : null}
        <Button
          size="icon"
          variant="ghost"
          className="h-7 w-7 shrink-0"
          aria-label={t('projects.preview.collapse')}
          title={t('projects.preview.collapse')}
          onClick={onClose}
        >
          <PanelRightClose className="h-4 w-4" />
        </Button>
      </header>

      <div className="relative h-px shrink-0 bg-border" aria-hidden={phase !== 'loading'}>
        {phase === 'loading' ? (
          <div className="absolute inset-y-0 left-0 w-1/3 animate-pulse bg-accent/70" />
        ) : null}
      </div>

      <div
        ref={resolvedBodyRef as RefObject<HTMLDivElement>}
        tabIndex={-1}
        className="min-h-0 min-w-0 flex-1 overflow-auto px-4 py-3 outline-none"
        aria-busy={phase === 'loading'}
      >
        {phase === 'loading' ? (
          <>
            <p className="sr-only" aria-live="polite">
              {t('projects.preview.loading')}
            </p>
            <PreviewSkeleton />
          </>
        ) : null}
        {phase === 'empty' ? (
          <p className="py-6 text-body text-muted">{t('projects.preview.empty')}</p>
        ) : null}
        {phase === 'error' ? (
          <div className="space-y-2 py-6">
            <p className="text-body text-danger">{t('projects.preview.failed')}</p>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => {
                const seq = ++requestSeq.current;
                const expectedId = session.id;
                setPhase('loading');
                void getAgentProjectExcerpts([session.id]).then(
                  (rows) => {
                    if (requestSeq.current !== seq) return;
                    if (session.id !== expectedId) return;
                    applyRows(expectedId, rows);
                  },
                  () => {
                    if (requestSeq.current === seq) setPhase('error');
                  },
                );
              }}
            >
              {t('projects.preview.retry')}
            </Button>
          </div>
        ) : null}
        {phase === 'ready' ? (
          <div className="space-y-2">
            <p className="text-meta text-muted">{t('projects.preview.turns', { n: turns.length })}</p>
            <ol className="space-y-2">
              {turns.map((turn, index) => {
                const userish = turn.role === 'user';
                return (
                  <li
                    key={`${index}:${turn.role}:${turn.text.slice(0, 24)}`}
                    className={cn('flex', userish && 'justify-end')}
                  >
                    <div
                      className={cn(
                        'min-w-0 max-w-[92%] text-body text-primary',
                        userish
                          ? 'rounded-composer bg-subtle px-3 py-2'
                          : 'leading-relaxed',
                      )}
                    >
                      <MarkdownView content={turn.text} variant="chat" />
                    </div>
                  </li>
                );
              })}
            </ol>
          </div>
        ) : null}
      </div>

      <footer className="flex shrink-0 items-center gap-2 border-t border-border px-3 py-1.5">
        <Tip
          className="min-w-0 flex-1 truncate font-mono text-meta text-muted"
          label={cwd || record || session.path}
        >
          {shortPath(cwd || record || session.path, 48)}
        </Tip>
      </footer>
    </aside>
  );
}
