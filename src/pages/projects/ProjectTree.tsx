import type { MouseEvent as ReactMouseEvent } from 'react';
import { ChevronDown, ChevronRight, EyeOff, Loader2 } from 'lucide-react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { AgentDot } from '@/components/shared/AgentDot';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Tip } from '@/components/ui/tooltip';
import type { AgentMeta } from '@/config/agents';
import { verifiedProjectWorkspacePath } from '@/lib/path-open';
import type { AgentId, AgentProject, AgentSession } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  displayTitle,
  fmtBytes,
  projectDisplayPath,
  relativeTime,
  titleHoverLabel,
} from './project-format';
import { ProjectPathLink } from './ProjectPathLink';
import { ProjectSessionRow } from './ProjectSessionRow';

export type ProjectTreeProps = {
  agentId: AgentId;
  agentMeta: AgentMeta | undefined;
  projects: AgentProject[];
  expanded: Set<string>;
  loadingProjectIds: Set<string>;
  selected: Set<string>;
  busy: boolean;
  showDelete: boolean;
  previewSessionId: string | null;
  visibleSessions: (projectId: string) => AgentSession[];
  onToggleExpand: (project: AgentProject) => void;
  onOpenProjectWorkspace: (p: AgentProject, e: ReactMouseEvent) => void;
  onToggleHideProject: (p: AgentProject, e: ReactMouseEvent) => void;
  onToggleOne: (id: string) => void;
  onPreviewSession: (session: AgentSession) => void;
  onCopySessionId: (s: AgentSession, e?: ReactMouseEvent) => void;
  onCopyResumeCommand: (s: AgentSession, e?: ReactMouseEvent) => void;
  onOpenSessionRecord: (s: AgentSession, e: ReactMouseEvent) => void;
  onGoContinue: (s: AgentSession) => void;
  onRequestDelete: (s: AgentSession) => void;
};

export function ProjectTree({
  agentId,
  agentMeta,
  projects: visibleProjects,
  expanded,
  loadingProjectIds,
  selected,
  busy,
  showDelete,
  previewSessionId,
  visibleSessions,
  onToggleExpand,
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
      {visibleProjects.map((p) => {
        const open = expanded.has(p.id);
        const loadingKids = loadingProjectIds.has(p.id);
        const kids = open ? visibleSessions(p.id) : [];
        const canExpand = p.sessionCount > 0 || p.agentId !== 'cursor';
        const title = displayTitle(p);
        const path = projectDisplayPath(p);
        const workspace = verifiedProjectWorkspacePath(p);
        return (
          <Card
            key={p.id}
            className={cn('overflow-hidden transition-colors', p.hidden && 'opacity-70')}
          >
            <div
              className={cn(
                'flex items-center gap-2 px-3 py-2',
                canExpand && 'cursor-pointer hover:bg-hover/40',
              )}
              onClick={() => canExpand && onToggleExpand(p)}
              role={canExpand ? 'button' : undefined}
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
              <AgentDot agentId={agentId} color={agentMeta?.color} className="shrink-0" />
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
                {p.hidden && (
                  <span className="shrink-0 text-xs text-muted">{t('projects.tree.hidden')}</span>
                )}
                <span className="shrink-0 text-xs text-muted tabular-nums">
                  {t('projects.tree.sessionMeta', {
                    time: relativeTime(p.updatedAt, t),
                    count: p.sessionCount,
                    size: fmtBytes(p.sizeBytes),
                  })}
                </span>
                {workspace ? (
                  <ProjectPathLink
                    path={workspace}
                    disabled={busy}
                    ariaLabel={t('projects.tree.openFolder', { path: workspace })}
                    onOpen={(e) => onOpenProjectWorkspace(p, e)}
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
                  aria-label={p.hidden ? t('projects.tree.unhide') : t('projects.tree.hide')}
                  title={p.hidden ? t('projects.tree.unhide') : t('projects.tree.hide')}
                  onClick={(e) => onToggleHideProject(p, e)}
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
                    {p.sessionCount === 0 ? t('projects.tree.noSessionFiles') : t('projects.tree.noMatch')}
                  </div>
                ) : (
                  <ul>
                    {kids.map((s) => (
                      <ProjectSessionRow
                        key={s.id}
                        session={s}
                        selected={selected.has(s.id)}
                        busy={busy}
                        showDelete={showDelete}
                        previewOpen={previewSessionId === s.id}
                        onToggleOne={onToggleOne}
                        onPreviewSession={onPreviewSession}
                        onCopySessionId={onCopySessionId}
                        onCopyResumeCommand={onCopyResumeCommand}
                        onOpenSessionRecord={onOpenSessionRecord}
                        onGoContinue={onGoContinue}
                        onRequestDelete={onRequestDelete}
                      />
                    ))}
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
