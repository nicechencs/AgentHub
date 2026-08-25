import type { MouseEvent as ReactMouseEvent } from 'react';
import { Copy, MessageSquarePlus, Terminal, Trash2 } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
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

const previewTextClass =
  'block w-full min-w-0 truncate text-left hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60';

/**
 * Packed tracks plus a shrinking spacer. The action cluster is `auto` so it
 * never compresses into the file name while the splitter is dragged.
 */
export function projectSessionRowGrid(showDelete: boolean): string {
  return cn(
    'grid min-w-0 items-center gap-x-2 overflow-hidden',
    showDelete
      ? 'grid-cols-[1.25rem_minmax(0,22rem)_minmax(0,10rem)_minmax(0,6.5rem)_minmax(0,4.75rem)_minmax(0,1fr)_auto]'
      : 'grid-cols-[minmax(0,22rem)_minmax(0,10rem)_minmax(0,6.5rem)_minmax(0,4.75rem)_minmax(0,1fr)_auto]',
  );
}

export function ProjectSessionRow({
  session,
  selected,
  busy,
  showDelete,
  previewOpen,
  onToggleOne,
  onPreviewSession,
  onCopySessionId,
  onCopyResumeCommand,
  onOpenSessionRecord,
  onGoContinue,
  onRequestDelete,
}: {
  session: AgentSession;
  selected: boolean;
  busy: boolean;
  showDelete: boolean;
  previewOpen: boolean;
  onToggleOne: (id: string) => void;
  onPreviewSession: (session: AgentSession) => void;
  onCopySessionId: (s: AgentSession, e?: ReactMouseEvent) => void;
  onCopyResumeCommand: (s: AgentSession, e?: ReactMouseEvent) => void;
  onOpenSessionRecord: (s: AgentSession, e: ReactMouseEvent) => void;
  onGoContinue: (s: AgentSession) => void;
  onRequestDelete: (s: AgentSession) => void;
}) {
  const { t } = useI18n();
  const record = normalizeOpenPath(session.path);
  const fileName = sessionFileName(session);
  const sid = nativeSessionId(session);
  const resume = nativeResumeCommand(session);

  return (
    <li
      className={cn(
        projectSessionRowGrid(showDelete),
        'px-3 py-2 pl-10',
        previewOpen && 'bg-active',
      )}
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
      <Tip label={titleHoverLabel(session.title, session.preview)} className="min-w-0 w-full">
        <button
          type="button"
          className={cn(previewTextClass, 'text-sm text-primary')}
          aria-current={previewOpen ? 'true' : undefined}
          aria-label={t('projects.tree.previewAria', { title: session.title })}
          onClick={() => onPreviewSession(session)}
        >
          {session.title}
        </button>
      </Tip>
      {fileName ? (
        <Tip label={record ?? fileName} className="min-w-0 w-full">
          {record ? (
            <button
              type="button"
              className={cn(previewTextClass, 'font-mono text-meta text-muted')}
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
        {showDelete && (
          <Button
            size="icon"
            variant="ghost"
            disabled={busy}
            className="text-danger hover:text-danger"
            aria-label={t('projects.tree.deleteSession')}
            title={t('projects.tree.deleteSession')}
            onClick={() => onRequestDelete(session)}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        )}
      </div>
    </li>
  );
}
