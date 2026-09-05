import type { MouseEvent as ReactMouseEvent } from 'react';
import { ChevronDown, ChevronRight, EyeOff, Loader2 } from 'lucide-react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { AgentDot } from '@/components/shared/AgentDot';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Tip } from '@/components/ui/tooltip';
import { agentDisplayName } from '@/config/agents';
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
};

export function ProjectTree({
  groups,
  showSessionAgent,
  expanded,
  loadingProjectIds,
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
}: ProjectTreeProps) {
  const { t } = useI18n();
  return (
    <div className={pageRhythm.stackDense}>
      {groups.map((group) => {
        const p = group.primary;
        const open = expanded.has(group.id);
        const loadingKids = group.members.some((member) => loadingProjectIds.has(member.id));
        const kids = open ? visibleSessions(group.id) : [];
        const canExpand = groupCanExpand(group);
        const title = displayTitle(p);
        const path = projectDisplayPath(p);
        const workspace = verifiedProjectWorkspacePath(p);
        return (
          <Card
            key={group.id}
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
              <span className="flex shrink-0 items-center gap-0.5">
                {group.agentIds.map((id) => (
                  <AgentDot
                    key={id}
                    agentId={id}
                    title={agentDisplayName(id)}
                  />
                ))}
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
                {group.agentIds.length > 1 && (
                  <span className="shrink-0 text-xs text-muted">
                    {group.agentIds.map((id) => agentDisplayName(id)).join(' · ')}
                  </span>
                )}
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

            {open && (
              <div className="border-t border-border bg-subtle/40">
                {loadingKids ? (
                  <div className="px-3 py-3 text-xs text-muted">{t('projects.tree.loadingSessions')}</div>
                ) : kids.length === 0 ? (
                  <div className="px-3 py-3 text-xs text-muted">
                    {group.sessionCount === 0 ? t('projects.tree.noSessionFiles') : t('projects.tree.noMatch')}
                  </div>
                ) : (
                  <ul>
                    {nestSessions(kids).flatMap(({ session: s, children }) => {
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
                                nestedLabel={t('projects.tree.subSession')}
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
                    })}
                  </ul>
                )}
              </div>
            )}
          </Card>
        );
      })}
    </div>
  );
}
