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
  titleHoverLabel,
} from './project-format';
import { ProjectPathLink } from './ProjectPathLink';

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
  const sid = nativeSessionId(session);
  const resume = nativeResumeCommand(session);

  return (
    <li className={cn('flex items-center gap-2 px-3 py-2 pl-10', previewOpen && 'bg-active')}>
      {showDelete && (
        <input
          type="checkbox"
          className="h-3.5 w-3.5 shrink-0 accent-[var(--accent)]"
          checked={selected}
          onChange={() => onToggleOne(session.id)}
          aria-label={t('projects.tree.selectSession', { title: session.title })}
        />
      )}
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <Tip
          label={titleHoverLabel(session.title, session.preview)}
          className="min-w-0 max-w-[22rem] shrink"
        >
          <button
            type="button"
            className="block w-full truncate text-left text-sm text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60"
            aria-current={previewOpen ? 'true' : undefined}
            aria-label={t('projects.tree.previewAria', { title: session.title })}
            onClick={() => onPreviewSession(session)}
          >
            {session.title}
          </button>
        </Tip>
        <span className="shrink-0 text-xs text-muted tabular-nums">
          {relativeTime(session.updatedAt, t)} · {fmtBytes(session.sizeBytes)}
          {session.messageCount != null && session.messageCount > 0
            ? t('projects.tree.lines', { n: session.messageCount })
            : ''}
        </span>
        {record ? (
          <ProjectPathLink
            path={record}
            disabled={busy}
            ariaLabel={t('projects.tree.locateRecord', { path: record })}
            onOpen={(e) => onOpenSessionRecord(session, e)}
          />
        ) : null}
      </div>
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
        <Button size="sm" variant="outline" disabled={busy} onClick={() => onGoContinue(session)}>
          <MessageSquarePlus className="h-3.5 w-3.5" />
          {t('projects.tree.continue')}
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
