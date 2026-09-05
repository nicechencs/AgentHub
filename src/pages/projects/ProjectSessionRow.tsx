import type { MouseEvent as ReactMouseEvent } from 'react';
import { ChevronDown, ChevronRight, Copy, MessageSquarePlus, Terminal, Trash2 } from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { ListNameButton } from '@/components/shared/ListNameButton';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { shouldOpenTableRowFromClick } from '@/components/ui/table-row-model';
import { Tip } from '@/components/ui/tooltip';
import { normalizeOpenPath } from '@/lib/path-open';
import type { AgentSession } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  fmtBytes,
  nativeResumeCommand,
  nativeSessionId,
  relativeTime,
  sessionFileName,
  titleHoverLabel,
} from './project-format';

/**
 * Packed tracks plus a shrinking spacer. The action cluster is `auto` so it
 * never compresses into the file name while the splitter is dragged.
 */
export function projectSessionRowGrid(showDelete: boolean, showAgent = false): string {
  return cn(
    'grid min-w-0 items-center gap-x-2 overflow-hidden',
    showDelete && showAgent
      ? 'grid-cols-[1.25rem_1.5rem_minmax(0,22rem)_minmax(0,10rem)_minmax(0,6.5rem)_minmax(0,4.75rem)_minmax(0,1fr)_auto]'
      : showDelete
        ? 'grid-cols-[1.25rem_minmax(0,22rem)_minmax(0,10rem)_minmax(0,6.5rem)_minmax(0,4.75rem)_minmax(0,1fr)_auto]'
        : showAgent
          ? 'grid-cols-[1.5rem_minmax(0,22rem)_minmax(0,10rem)_minmax(0,6.5rem)_minmax(0,4.75rem)_minmax(0,1fr)_auto]'
          : 'grid-cols-[minmax(0,22rem)_minmax(0,10rem)_minmax(0,6.5rem)_minmax(0,4.75rem)_minmax(0,1fr)_auto]',
  );
}

export function ProjectSessionRow({
  session,
  selected,
  busy,
  showDelete,
  deleteHint,
  nested = false,
  nestedLabel,
  childCount = 0,
  nestedOpen = false,
  onToggleNested,
  previewOpen,
  followPreview = false,
  onToggleOne,
  onPreviewSession,
  onCopySessionId,
  onCopyResumeCommand,
  onOpenSessionRecord,
  onGoContinue,
  onRequestDelete,
  showAgent = false,
}: {
  session: AgentSession;
  selected: boolean;
  busy: boolean;
  showDelete: boolean;
  /** When set, the delete control is visible but disabled (read-only agents). */
  deleteHint?: string | null;
  nested?: boolean;
  nestedLabel?: string;
  childCount?: number;
  nestedOpen?: boolean;
  onToggleNested?: (id: string) => void;
  previewOpen: boolean;
  followPreview?: boolean;
  onToggleOne: (id: string) => void;
  onPreviewSession: (session: AgentSession) => void;
  onCopySessionId: (s: AgentSession, e?: ReactMouseEvent) => void;
  onCopyResumeCommand: (s: AgentSession, e?: ReactMouseEvent) => void;
  onOpenSessionRecord: (s: AgentSession, e: ReactMouseEvent) => void;
  onGoContinue: (s: AgentSession) => void;
  onRequestDelete: (s: AgentSession) => void;
  /** 全部 / 合并路径时在会话行标出 Agent。 */
  showAgent?: boolean;
}) {
  const { t } = useI18n();
  const record = normalizeOpenPath(session.path);
  const fileName = sessionFileName(session);
  const sid = nativeSessionId(session);
  const resume = nativeResumeCommand(session);
  const showDeleteAction = showDelete || Boolean(deleteHint);

  return (
    <li
      className={cn(
        projectSessionRowGrid(showDelete, showAgent),
        'px-3 py-2',
        nested ? 'pl-16' : 'pl-10',
        previewOpen && 'bg-active',
        followPreview && 'cursor-pointer',
      )}
      onClick={(event) => {
        if (!followPreview || !shouldOpenTableRowFromClick(event)) return;
        onPreviewSession(session);
      }}
    >
      {showDelete && (
        <input
          type="checkbox"
          className="h-3.5 w-3.5 shrink-0 accent-[var(--accent)]"
          checked={selected}
          onChange={() => onToggleOne(session.id)}
          aria-label={t('projects.tree.selectSession', { title: session.title })}
        />
      )}
      {showAgent ? (
        <span className="flex justify-center">
          <AgentLogo agentId={session.agentId} size="sm" />
        </span>
      ) : null}
      <div className="flex min-w-0 w-full items-center gap-1">
        {childCount > 0 ? (
          <button
            type="button"
            className="flex h-5 w-5 shrink-0 items-center justify-center text-muted hover:text-primary"
            aria-expanded={nestedOpen}
            aria-label={
              nestedOpen
                ? t('projects.tree.collapseSubSessions')
                : t('projects.tree.expandSubSessions')
            }
            onClick={(e) => {
              e.stopPropagation();
              onToggleNested?.(session.id);
            }}
          >
            {nestedOpen ? (
              <ChevronDown className="h-3.5 w-3.5" />
            ) : (
              <ChevronRight className="h-3.5 w-3.5" />
            )}
          </button>
        ) : nested ? null : (
          <span className="h-5 w-5 shrink-0" />
        )}
        <div className="min-w-0 flex-1">
          <ListNameButton
            hint={titleHoverLabel(session.title, session.preview)}
            className="w-full"
            aria-current={previewOpen ? 'true' : undefined}
            data-help="list-row"
            aria-label={t('projects.tree.previewAria', { title: session.title })}
            onClick={() => onPreviewSession(session)}
          >
            {nested && nestedLabel ? (
              <span className="mr-1.5 text-meta text-muted">{nestedLabel}</span>
            ) : null}
            {session.title}
          </ListNameButton>
        </div>
        {childCount > 0 && !nestedOpen ? (
          <span className="shrink-0 text-xs text-muted tabular-nums">
            {t('projects.tree.subSessionCount', { n: childCount })}
          </span>
        ) : null}
      </div>
      {fileName ? (
        <Tip label={record ?? fileName} className="min-w-0 w-full">
          {record ? (
            <button
              type="button"
              className="block w-full min-w-0 truncate text-left font-mono text-meta text-muted hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
              disabled={busy}
              aria-label={t('projects.tree.openRecordFolder', { path: record })}
              onClick={(e) => onOpenSessionRecord(session, e)}
            >
              {fileName}
            </button>
          ) : (
            <span className="block min-w-0 truncate font-mono text-meta text-muted">{fileName}</span>
          )}
        </Tip>
      ) : (
        <span />
      )}
      <span className="min-w-0 truncate text-xs text-muted tabular-nums">
        {relativeTime(session.updatedAt, t)}
      </span>
      <span className="min-w-0 truncate text-xs text-muted tabular-nums">
        {fmtBytes(session.sizeBytes)}
      </span>
      <span className="min-w-0" aria-hidden />
      <div className="flex shrink-0 gap-1">
        {sid ? (
          <Button
            size="icon"
            variant="ghost"
            disabled={busy}
            aria-label={t('projects.tree.copySessionId', { id: sid })}
            title={t('projects.tree.copySessionId', { id: sid })}
            onClick={(e) => onCopySessionId(session, e)}
          >
            <Copy className="h-3.5 w-3.5" />
          </Button>
        ) : null}
        {resume ? (
          <Button
            size="icon"
            variant="ghost"
            disabled={busy}
            aria-label={t('projects.tree.copyResumeCommand', { command: resume })}
            title={t('projects.tree.copyResumeCommand', { command: resume })}
            onClick={(e) => onCopyResumeCommand(session, e)}
          >
            <Terminal className="h-3.5 w-3.5" />
          </Button>
        ) : null}
        <Button
          size="icon"
          variant="ghost"
          disabled={busy}
          aria-label={t('projects.tree.continue')}
          title={t('projects.tree.continue')}
          onClick={() => onGoContinue(session)}
        >
          <MessageSquarePlus className="h-3.5 w-3.5" />
        </Button>
        {showDeleteAction && (
          <Button
            size="icon"
            variant="ghost"
            disabled={busy || Boolean(deleteHint)}
            className="text-danger hover:text-danger"
            aria-label={deleteHint || t('projects.tree.deleteSession')}
            title={deleteHint || t('projects.tree.deleteSession')}
            onClick={() => {
              if (deleteHint) return;
              onRequestDelete(session);
            }}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        )}
      </div>
    </li>
  );
}
