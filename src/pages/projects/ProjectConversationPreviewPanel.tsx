import { useEffect, useId, useRef, useState, type RefObject } from 'react';
import { ChevronLeft, Copy, MessageSquarePlus, PanelRightClose } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { CopyableFileName } from '@/components/shared/CopyableFileName';
import { copyTextToClipboard, CopyTextButton } from '@/components/shared/CopyTextButton';
import { OpenDirButton } from '@/components/shared/OpenDirButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Notice } from '@/components/shared/Notice';
import { MarkdownView } from '@/components/shared/MarkdownView';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/toast';
import { AGENT_MAP } from '@/config/agents';
import { getAgentProjectExcerpts } from '@/lib/api/project';
import { normalizeOpenPath } from '@/lib/path-open';
import type { TranslateFn } from '@/lib/i18n';
import { hasEscPriorityOverlay } from '@/lib/skills/preview-keys';
import type { AgentSession } from '@/lib/types';
import { cn } from '@/lib/utils';
import { relativeTime } from './project-format';
import { formatSessionRecordText } from '@/lib/session-record-text';
import {
  buildPreviewTimeline,
  classifyExcerptRows,
  excerptTurnsToRecordLines,
  parseApprovalDecisions,
  splitExcerptDocument,
  type ApprovalDecision,
} from './session-excerpt';

type PreviewLayer = 'conversation' | 'convention' | 'approvals';

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
  reviewSessions = [],
  open,
  onClose,
  onContinue,
  onOpenRecord,
  onRecordLoaded,
  busy,
  width,
  className,
  contentRef,
  reloadKey = 0,
}: {
  session: AgentSession;
  reviewSessions?: AgentSession[];
  open: boolean;
  onClose: () => void;
  onContinue: (session: AgentSession) => void;
  onOpenRecord: (session: AgentSession) => void;
  onRecordLoaded?: (sessionId: string, record: { excerpt: string; truncated: boolean } | null) => void;
  busy?: boolean;
  width?: number;
  className?: string;
  contentRef?: RefObject<HTMLDivElement | null>;
  reloadKey?: number;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const titleId = useId();
  const requestSeq = useRef(0);
  const bodyRef = useRef<HTMLDivElement>(null);
  const resolvedBodyRef = contentRef ?? bodyRef;
  const onRecordLoadedRef = useRef(onRecordLoaded);
  onRecordLoadedRef.current = onRecordLoaded;
  const [phase, setPhase] = useState<'loading' | 'ready' | 'empty' | 'error'>('loading');
  const [excerpt, setExcerpt] = useState('');
  const [truncated, setTruncated] = useState(false);
  const [layer, setLayer] = useState<PreviewLayer>('conversation');
  const [approvals, setApprovals] = useState<ApprovalDecision[]>([]);
  const reviewKey = reviewSessions.map((item) => item.id).join('|');

  const applyRows = (sessionId: string, rows: { id: string; excerpt?: string | null; truncated?: boolean | null }[]) => {
    const result = classifyExcerptRows(sessionId, rows);
    if (result.status === 'ready') {
      setExcerpt(result.excerpt);
      setTruncated(result.truncated);
      setPhase('ready');
      onRecordLoadedRef.current?.(sessionId, { excerpt: result.excerpt, truncated: result.truncated });
      return;
    }
    setExcerpt('');
    setTruncated(false);
    setPhase(result.status);
    onRecordLoadedRef.current?.(sessionId, null);
  };

  useEffect(() => {
    setLayer('conversation');
    setApprovals([]);
  }, [session.id]);

  useEffect(() => {
    if (!open) return;
    const seq = ++requestSeq.current;
    const expectedId = session.id;
    setPhase('loading');
    setExcerpt('');
    setTruncated(false);
    setLayer('conversation');
    onRecordLoadedRef.current?.(expectedId, null);
    void getAgentProjectExcerpts([session.id]).then(
      (rows) => {
        if (requestSeq.current !== seq) return;
        if (session.id !== expectedId) return;
        applyRows(expectedId, rows);
      },
      () => {
        if (requestSeq.current !== seq) return;
        setPhase('error');
        onRecordLoadedRef.current?.(expectedId, null);
      },
    );
  }, [open, session.id, reloadKey]);

  useEffect(() => {
    if (!open || !reviewKey) {
      setApprovals([]);
      return;
    }
    const ids = reviewKey.split('|').filter(Boolean);
    const expectedId = session.id;
    void getAgentProjectExcerpts(ids).then(
      (rows) => {
        if (session.id !== expectedId) return;
        const decisions = rows.flatMap((row) =>
          parseApprovalDecisions(splitExcerptDocument(row.excerpt ?? '').turns).map((item) => ({
            ...item,
            at: item.at ?? row.updatedAt,
          })),
        );
        setApprovals(decisions);
      },
      () => {
        if (session.id !== expectedId) return;
        setApprovals([]);
      },
    );
  }, [open, session.id, reloadKey, reviewKey]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      if (hasEscPriorityOverlay()) return;
      e.preventDefault();
      if (layer !== 'conversation') {
        setLayer('conversation');
        return;
      }
      onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose, layer]);

  if (!open) return null;

  const parsed = phase === 'ready' ? splitExcerptDocument(excerpt) : { convention: null, turns: [] };
  const turns = parsed.turns;
  const convention = parsed.convention;
  const timeline =
    phase === 'ready' ? buildPreviewTimeline(convention, turns, approvals) : [];
  const agentMeta = AGENT_MAP[session.agentId];
  const record = normalizeOpenPath(session.path);
  const cwd = session.cwd?.trim() || null;
  const recordText = formatSessionRecordText(
    excerptTurnsToRecordLines(turns, {
      user: t('common.you'),
      assistant: t('projects.preview.roleAssistant', {
        name: agentMeta?.name ?? session.agentId,
      }),
    }),
  );
  const layerTitle =
    layer === 'convention'
      ? t('projects.preview.convention')
      : layer === 'approvals'
        ? t('projects.preview.approvals')
        : session.title || t('projects.preview.titleFallback');
  const copyText =
    layer === 'convention'
      ? convention ?? ''
      : layer === 'approvals'
        ? approvals
            .map((item) => `${approvalOutcomeLabel(item.outcome, t)}\n${item.rationale}`.trim())
            .filter(Boolean)
            .join('\n\n')
        : recordText;
  const copyRecord = () => {
    if (!copyText) {
      toast({ title: t('common.copyRecordEmpty'), variant: 'danger' });
      return;
    }
    void copyTextToClipboard(copyText).then(
      () => toast({ title: t('common.copied'), variant: 'success' }),
      () => toast({ title: t('common.copyFailed'), variant: 'danger' }),
    );
  };

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
        {layer !== 'conversation' ? (
          <Button
            size="icon"
            variant="ghost"
            className="h-7 w-7 shrink-0"
            aria-label={t('projects.preview.backToConversation')}
            title={t('projects.preview.backToConversation')}
            onClick={() => setLayer('conversation')}
          >
            <ChevronLeft className="h-4 w-4" />
          </Button>
        ) : (
          <AgentDot agentId={session.agentId} color={agentMeta?.color} className="shrink-0" />
        )}
        <div className="min-w-0 flex-1 basis-16">
          <div className="flex min-w-0 items-baseline gap-2">
            <h2 id={titleId} className="truncate text-body font-semibold leading-tight text-primary">
              {layerTitle}
            </h2>
            {layer === 'conversation' ? (
              <span className="shrink-0 text-meta text-muted">
                {agentMeta?.name ?? session.agentId}
                {' · '}
                {relativeTime(session.updatedAt, t)}
              </span>
            ) : null}
          </div>
        </div>
        <Button
          size="icon"
          variant="ghost"
          className="h-7 w-7 shrink-0"
          disabled={!copyText}
          aria-label={t('projects.preview.copyRecord')}
          title={t('projects.preview.copyRecord')}
          onClick={copyRecord}
        >
          <Copy className="h-3.5 w-3.5" />
        </Button>
        <Button
          size="icon"
          variant="ghost"
          className="h-7 w-7 shrink-0"
          disabled={busy}
          aria-label={t('projects.tree.continue')}
          title={t('projects.tree.continue')}
          onClick={() => onContinue(session)}
        >
          <MessageSquarePlus className="h-3.5 w-3.5" />
        </Button>
        {record ? (
          <OpenDirButton
            title={t('projects.preview.openRecord')}
            onClick={() => onOpenRecord(session)}
          />
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
                setExcerpt('');
                setTruncated(false);
                onRecordLoadedRef.current?.(expectedId, null);
                void getAgentProjectExcerpts([session.id]).then(
                  (rows) => {
                    if (requestSeq.current !== seq) return;
                    if (session.id !== expectedId) return;
                    applyRows(expectedId, rows);
                  },
                  () => {
                    if (requestSeq.current !== seq) return;
                    setPhase('error');
                    onRecordLoadedRef.current?.(expectedId, null);
                  },
                );
              }}
            >
              {t('projects.preview.retry')}
            </Button>
          </div>
        ) : null}
        {phase === 'ready' && layer === 'convention' ? (
          convention ? (
            <MarkdownView content={convention} variant="document" />
          ) : (
            <p className="py-6 text-body text-muted">{t('projects.preview.conventionEmpty')}</p>
          )
        ) : null}
        {phase === 'ready' && layer === 'approvals' ? (
          approvals.length > 0 ? (
            <ol className="space-y-2">
              {approvals.map((item, index) => (
                <li
                  key={`${index}:${item.outcome}:${item.rationale.slice(0, 24)}`}
                  className="rounded-card border border-border bg-subtle px-3 py-2"
                >
                  <p
                    className={cn(
                      'text-meta font-medium',
                      item.outcome === 'deny' ? 'text-danger' : 'text-accent',
                    )}
                  >
                    {approvalOutcomeLabel(item.outcome, t)}
                  </p>
                  {item.rationale ? (
                    <p className="mt-1 text-body text-primary">{item.rationale}</p>
                  ) : null}
                  <details className="mt-2">
                    <summary className="cursor-pointer text-meta text-muted">
                      {t('projects.preview.approvalRaw')}
                    </summary>
                    <pre className="mt-1 overflow-x-auto whitespace-pre-wrap break-words text-meta text-secondary">
                      {item.raw}
                    </pre>
                  </details>
                </li>
              ))}
            </ol>
          ) : (
            <p className="py-6 text-body text-muted">{t('projects.preview.approvalsEmpty')}</p>
          )
        ) : null}
        {phase === 'ready' && layer === 'conversation' ? (
          <div className="space-y-2">
            {truncated ? (
              <Notice tone="warning" className="text-meta">
                {t('projects.preview.truncated')}
              </Notice>
            ) : null}
            {turns.length > 0 ? (
              <p className="text-meta text-muted">{t('projects.preview.turns', { n: turns.length })}</p>
            ) : timeline.length === 0 ? (
              <p className="py-6 text-body text-muted">{t('projects.preview.empty')}</p>
            ) : null}
            <ol className="space-y-3">
              {timeline.map((item, index) => {
                if (item.kind === 'convention') {
                  return (
                    <li key={`convention:${index}`} className="flex justify-start">
                      <button
                        type="button"
                        className="rounded-btn border border-border bg-subtle px-3 py-1.5 text-left text-meta text-accent hover:underline"
                        onClick={() => setLayer('convention')}
                      >
                        {t('projects.preview.convention')}
                      </button>
                    </li>
                  );
                }
                if (item.kind === 'approval') {
                  const decision = item.decision;
                  return (
                    <li key={`approval:${index}:${decision.outcome}`} className="flex justify-start">
                      <button
                        type="button"
                        className="max-w-[92%] rounded-btn border border-border bg-subtle px-3 py-1.5 text-left"
                        onClick={() => setLayer('approvals')}
                      >
                        <span
                          className={cn(
                            'text-meta font-medium',
                            decision.outcome === 'deny' ? 'text-danger' : 'text-accent',
                          )}
                        >
                          {t('projects.preview.approvals')}
                          {' · '}
                          {approvalOutcomeLabel(decision.outcome, t)}
                        </span>
                        {decision.rationale ? (
                          <span className="mt-0.5 block text-meta text-secondary line-clamp-2">
                            {decision.rationale}
                          </span>
                        ) : null}
                      </button>
                    </li>
                  );
                }
                const turn = item.turn;
                const userish = turn.role === 'user';
                return (
                  <li
                    key={`${index}:${turn.role}:${turn.text.slice(0, 24)}`}
                    className={cn('flex gap-2', userish ? 'justify-end' : 'justify-start')}
                  >
                    {userish ? (
                      <div
                        className="group relative min-w-0 max-w-[92%] rounded-composer bg-subtle px-3 py-2 text-body text-primary"
                        aria-label={t('projects.preview.roleUser')}
                      >
                        <MarkdownView content={turn.text} variant="chat" />
                        <CopyTextButton text={turn.text} label={t('projects.preview.copyTurn')} />
                      </div>
                    ) : (
                      <>
                        <AgentDot
                          agentId={session.agentId}
                          color={agentMeta?.color}
                          className="mt-1.5 shrink-0"
                        />
                        <div className="min-w-0 max-w-[92%] flex-1">
                          <p className="mb-1 text-meta text-muted">
                            {t('projects.preview.roleAssistant', {
                              name: agentMeta?.name ?? session.agentId,
                            })}
                          </p>
                          <div className="group relative rounded-composer bg-hover/60 px-3 py-2 text-body leading-relaxed text-primary">
                            <MarkdownView content={turn.text} variant="chat" />
                            <CopyTextButton text={turn.text} label={t('projects.preview.copyTurn')} />
                          </div>
                        </div>
                      </>
                    )}
                  </li>
                );
              })}
            </ol>
          </div>
        ) : null}
      </div>

      <footer className="flex shrink-0 items-center gap-2 border-t border-border px-3 py-1.5">
        <CopyableFileName
          path={cwd || record || session.path}
          className="min-w-0 flex-1"
        />
      </footer>
    </aside>
  );
}

function approvalOutcomeLabel(outcome: string, t: TranslateFn): string {
  if (outcome === 'allow') return t('projects.preview.approvalAllow');
  if (outcome === 'deny') return t('projects.preview.approvalDeny');
  return outcome;
}
