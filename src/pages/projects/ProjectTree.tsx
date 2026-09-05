import { memo, useEffect, useState, type MouseEvent as ReactMouseEvent } from 'react';
import { ChevronDown, ChevronRight, EyeOff, Loader2 } from 'lucide-react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Tip } from '@/components/ui/tooltip';
import { verifiedProjectWorkspacePath } from '@/lib/path-open';
import type { AgentKey, AgentSession } from '@/lib/types';
import { cn } from '@/lib/utils';
import { groupCanExpand, type ProjectGroup } from './project-groups';
import {
  displayTitle,
  fmtBytes,
  projectDisplayPath,
  relativeTime,
  titleHoverLabel,
} from './project-format';
import { ProjectPathLink } from './ProjectPathLink';
import { ProjectSessionRow } from './ProjectSessionRow';
import {
  clampSessionPage,
  sessionPageCount,
  sliceSessionPage,
} from './projects-list-model';
import { nestSessions } from './session-nest';

export type ProjectTreeProps = {
  groups: ProjectGroup[];
  showSessionAgent: boolean;
  expanded: Set<string>;
  loadingProjectIds: Set<string>;
  selected: Set<string>;
  busy: boolean;
  showDelete: boolean;
  deleteHintFor?: (agentId: AgentKey) => string | null;
  previewSessionId: string | null;
  nestedOpen: Set<string>;
  visibleSessions: (groupId: string) => AgentSession[];
  onToggleExpand: (group: ProjectGroup) => void;
  onToggleNested: (id: string) => void;
  onOpenProjectWorkspace: (group: ProjectGroup, e: ReactMouseEvent) => void;
  onToggleHideProject: (group: ProjectGroup, e: ReactMouseEvent) => void;
  onToggleOne: (id: string) => void;
  onPreviewSession: (session: AgentSession) => void;
  onCopySessionId: (s: AgentSession, e?: ReactMouseEvent) => void;
  onCopyResumeCommand: (s: AgentSession, e?: ReactMouseEvent) => void;
  onOpenSessionRecord: (s: AgentSession, e: ReactMouseEvent) => void;
  onGoContinue: (s: AgentSession) => void;
  onRequestDelete: (s: AgentSession) => void;
  queryKey?: string;
};

export function ProjectTree(props: ProjectTreeProps) {
  return (
    <div className={pageRhythm.stackDense}>
      {props.groups.map((group) => (
        <ProjectGroupCard
          key={group.id}
          group={group}
          open={props.expanded.has(group.id)}
          loadingKids={group.members.some((member) => props.loadingProjectIds.has(member.id))}
          {...props}
        />
      ))}
    </div>
  );
}

type GroupCardProps = ProjectTreeProps & {
  group: ProjectGroup;
  open: boolean;
  loadingKids: boolean;
};

const ProjectGroupCard = memo(function ProjectGroupCard({
  group,
  open,
  loadingKids,
  showSessionAgent,
  selected,
  busy,
  showDelete,
  deleteHintFor,
  previewSessionId,
  nestedOpen,
  visibleSessions,
  onToggleExpand,
  onToggleNested,
  onOpenProjectWorkspace,
  onToggleHideProject,
  onToggleOne,
  onPreviewSession,
  onCopySessionId,
  onCopyResumeCommand,
  onOpenSessionRecord,
  onGoContinue,
  onRequestDelete,
  queryKey = '',
}: GroupCardProps) {
  const { t } = useI18n();
  const [page, setPage] = useState(0);
  const [heldOpen, setHeldOpen] = useState(false);
  const keepPane = open || heldOpen;

  useEffect(() => {
    if (open) setHeldOpen(true);
  }, [open]);

  useEffect(() => {
    setPage(0);
  }, [queryKey, group.id]);

  const kids = keepPane ? visibleSessions(group.id) : [];
  const nested = keepPane ? nestSessions(kids) : [];
  const pages = sessionPageCount(nested.length);
  const currentPage = clampSessionPage(page, nested.length);
  const pageRows = keepPane ? sliceSessionPage(nested, currentPage) : [];

  const canExpand = groupCanExpand(group);
  const p = group.primary;
  const title = displayTitle(p);
  const path = projectDisplayPath(p);
  const workspace = verifiedProjectWorkspacePath(p);

  return (
    <Card
      className={cn('overflow-hidden transition-colors', group.hidden && 'opacity-70')}
    >
      <div
        className={cn(
          'flex items-center gap-2 px-3 py-2',
          canExpand && 'cursor-pointer hover:bg-hover/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
        )}
        onClick={() => canExpand && onToggleExpand(group)}
        onKeyDown={(event) => {
          if (!canExpand) return;
          if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault();
            onToggleExpand(group);
          }
        }}
        role={canExpand ? 'button' : undefined}
        tabIndex={canExpand ? 0 : undefined}
        aria-expanded={canExpand ? open : undefined}
      >
        <span className="flex h-5 w-5 shrink-0 items-center justify-center text-muted">
          {!canExpand ? (
            <span className="w-3.5" />
          ) : loadingKids ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : open ? (
            <ChevronDown className="h-3.5 w-3.5" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5" />
          )}
        </span>
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <Tip
            label={titleHoverLabel(title, p.preview)}
            className="min-w-0 max-w-[18rem] shrink"
          >
            <span className="block truncate text-sm font-medium text-primary">{title}</span>
          </Tip>
          {p.alias?.trim() && (
            <span className="shrink-0 text-xs text-muted">({p.title})</span>
          )}
          <span className="flex shrink-0 items-center gap-0.5">
            {group.agentIds.map((id) => (
              <AgentLogo key={id} agentId={id} size="sm" />
            ))}
          </span>
          {group.hidden && (
            <span className="shrink-0 text-xs text-muted">{t('projects.tree.hidden')}</span>
          )}
          <span className="shrink-0 text-xs text-muted tabular-nums">
            {t('projects.tree.sessionMeta', {
              time: relativeTime(group.updatedAt, t),
              count: group.sessionCount,
              size: fmtBytes(group.sizeBytes),
            })}
          </span>
          {workspace ? (
            <ProjectPathLink
              path={workspace}
              disabled={busy}
              ariaLabel={t('projects.tree.openFolder', { path: workspace })}
              onOpen={(e) => onOpenProjectWorkspace(group, e)}
            />
          ) : (
            <ProjectPathLink path={path} />
          )}
        </div>
        <div className="flex shrink-0 gap-1" onClick={(e) => e.stopPropagation()}>
          <Button
            size="icon"
            variant="ghost"
            disabled={busy}
            aria-label={group.hidden ? t('projects.tree.unhide') : t('projects.tree.hide')}
            title={group.hidden ? t('projects.tree.unhide') : t('projects.tree.hide')}
            onClick={(e) => onToggleHideProject(group, e)}
          >
            <EyeOff className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>

      {keepPane && (
        <div
          hidden={!open}
          className={cn('border-t border-border bg-subtle/40', !open && 'hidden')}
        >
          {loadingKids && kids.length === 0 ? (
            <div className="px-3 py-3 text-xs text-muted">{t('projects.tree.loadingSessions')}</div>
          ) : kids.length === 0 ? (
            <div className="px-3 py-3 text-xs text-muted">
              {group.sessionCount === 0 ? t('projects.tree.noSessionFiles') : t('projects.tree.noMatch')}
            </div>
          ) : (
            <>
              <ul>
                {pageRows.flatMap(({ session: s, children }) =>
                  sessionRows({
                    session: s,
                    children,
                    nestedOpen,
                    selected,
                    busy,
                    showDelete,
                    deleteHintFor,
                    showSessionAgent,
                    previewSessionId,
                    nestedLabel: t('projects.tree.subSession'),
                    onToggleNested,
                    onToggleOne,
                    onPreviewSession,
                    onCopySessionId,
                    onCopyResumeCommand,
                    onOpenSessionRecord,
                    onGoContinue,
                    onRequestDelete,
                  }),
                )}
              </ul>
              {pages > 1 && (
                <div
                  className="flex items-center justify-end gap-2 border-t border-border px-3 py-2"
                  role="navigation"
                  aria-label={t('projects.tree.pageAria')}
                >
                  <span className="text-xs text-muted tabular-nums">
                    {t('projects.tree.pageStatus', {
                      page: currentPage + 1,
                      pages,
                    })}
                  </span>
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={currentPage <= 0}
                    onClick={() => setPage((prev) => Math.max(0, prev - 1))}
                  >
                    {t('projects.tree.pagePrev')}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={currentPage >= pages - 1}
                    onClick={() =>
                      setPage((prev) => clampSessionPage(prev + 1, nested.length))
                    }
                  >
                    {t('projects.tree.pageNext')}
                  </Button>
                </div>
              )}
            </>
          )}
        </div>
      )}
    </Card>
  );
}, function groupCardPropsEqual(prev, next) {
  return (
    prev.open === next.open &&
    prev.loadingKids === next.loadingKids &&
    prev.group === next.group &&
    prev.selected === next.selected &&
    prev.nestedOpen === next.nestedOpen &&
    prev.previewSessionId === next.previewSessionId &&
    prev.busy === next.busy &&
    prev.showDelete === next.showDelete &&
    prev.showSessionAgent === next.showSessionAgent &&
    prev.queryKey === next.queryKey &&
    prev.visibleSessions === next.visibleSessions
  );
});

function sessionRows({
  session: s,
  children,
  nestedOpen,
  selected,
  busy,
  showDelete,
  deleteHintFor,
  showSessionAgent,
  previewSessionId,
  nestedLabel,
  onToggleNested,
  onToggleOne,
  onPreviewSession,
  onCopySessionId,
  onCopyResumeCommand,
  onOpenSessionRecord,
  onGoContinue,
  onRequestDelete,
}: {
  session: AgentSession;
  children: AgentSession[];
  nestedOpen: Set<string>;
  selected: Set<string>;
  busy: boolean;
  showDelete: boolean;
  deleteHintFor?: (agentId: AgentKey) => string | null;
  showSessionAgent: boolean;
  previewSessionId: string | null;
  nestedLabel: string;
  onToggleNested: (id: string) => void;
  onToggleOne: (id: string) => void;
  onPreviewSession: (session: AgentSession) => void;
  onCopySessionId: (s: AgentSession, e?: ReactMouseEvent) => void;
  onCopyResumeCommand: (s: AgentSession, e?: ReactMouseEvent) => void;
  onOpenSessionRecord: (s: AgentSession, e: ReactMouseEvent) => void;
  onGoContinue: (s: AgentSession) => void;
  onRequestDelete: (s: AgentSession) => void;
}) {
  const openNested = nestedOpen.has(s.id);
  return [
    <ProjectSessionRow
      key={s.id}
      session={s}
      selected={selected.has(s.id)}
      busy={busy}
      showDelete={showDelete}
      deleteHint={deleteHintFor?.(s.agentId) ?? null}
      showAgent={showSessionAgent}
      nested={false}
      childCount={children.length}
      nestedOpen={openNested}
      onToggleNested={onToggleNested}
      previewOpen={previewSessionId === s.id}
      onToggleOne={onToggleOne}
      onPreviewSession={onPreviewSession}
      onCopySessionId={onCopySessionId}
      onCopyResumeCommand={onCopyResumeCommand}
      onOpenSessionRecord={onOpenSessionRecord}
      onGoContinue={onGoContinue}
      onRequestDelete={onRequestDelete}
    />,
    ...(openNested
      ? children.map((child) => (
          <ProjectSessionRow
            key={child.id}
            session={child}
            selected={selected.has(child.id)}
            busy={busy}
            showDelete={showDelete}
            deleteHint={deleteHintFor?.(child.agentId) ?? null}
            showAgent={showSessionAgent}
            nested
            nestedLabel={nestedLabel}
            previewOpen={previewSessionId === child.id}
            onToggleOne={onToggleOne}
            onPreviewSession={onPreviewSession}
            onCopySessionId={onCopySessionId}
            onCopyResumeCommand={onCopyResumeCommand}
            onOpenSessionRecord={onOpenSessionRecord}
            onGoContinue={onGoContinue}
            onRequestDelete={onRequestDelete}
          />
        ))
      : []),
  ];
}
