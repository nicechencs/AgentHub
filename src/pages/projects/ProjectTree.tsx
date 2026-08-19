import type { MouseEvent as ReactMouseEvent } from 'react';
import {
  ChevronDown,
  ChevronRight,
  Copy,
  EyeOff,
  Terminal,
  Loader2,
  MessageSquarePlus,
  Trash2,
} from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Hint, Tip } from '@/components/ui/tooltip';
import type { AgentMeta } from '@/config/agents';
import { normalizeOpenPath, verifiedProjectWorkspacePath } from '@/lib/path-open';
import type { AgentId, AgentProject, AgentSession } from '@/lib/types';
import { cn } from '@/lib/utils';
import { pageRhythm } from '@/components/layout/page-rhythm';
import {
  displayTitle,
  fmtBytes,
  nativeResumeCommand,
  nativeSessionId,
  projectDisplayPath,
  titleHoverLabel,
  relativeTime,
  shortPath,
} from './project-format';

export type ProjectTreeProps = {
  agentId: AgentId;
  agentMeta: AgentMeta | undefined;
  projects: AgentProject[];
  expanded: Set<string>;
  loadingProjectIds: Set<string>;
  selected: Set<string>;
  busy: boolean;
  showDelete: boolean;
  visibleSessions: (projectId: string) => AgentSession[];
  onToggleExpand: (project: AgentProject) => void;
  onOpenProjectWorkspace: (p: AgentProject, e: ReactMouseEvent) => void;
  onToggleHideProject: (p: AgentProject, e: ReactMouseEvent) => void;
  onToggleOne: (id: string) => void;
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
  visibleSessions,
  onToggleExpand,
  onOpenProjectWorkspace,
  onToggleHideProject,
  onToggleOne,
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
                className={cn(
                  'overflow-hidden transition-colors',
                  p.hidden && 'opacity-70',
                )}
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
                  <AgentDot
                    agentId={agentId}
                    color={agentMeta?.color}
                    className="shrink-0"
                  />
                  <div className="flex min-w-0 flex-1 items-center gap-2">
                    <Tip
                      label={titleHoverLabel(title, p.preview)}
                      className="min-w-0 max-w-[18rem] shrink"
                    >
                      <span className="block truncate text-sm font-medium text-primary">
                        {title}
                      </span>
                    </Tip>
                    {p.alias?.trim() && (
                      <span className="shrink-0 text-xs text-muted">({p.title})</span>
                    )}
                    {p.hidden && <span className="shrink-0 text-xs text-muted">{t('projects.tree.hidden')}</span>}
                    <span className="shrink-0 text-xs text-muted tabular-nums">
                      {t('projects.tree.sessionMeta', {
                        time: relativeTime(p.updatedAt, t),
                        count: p.sessionCount,
                        size: fmtBytes(p.sizeBytes),
                      })}
                    </span>
                    {workspace ? (
                      <PathLink
                        path={workspace}
                        disabled={busy}
                        ariaLabel={t('projects.tree.openFolder', { path: workspace })}
                        onOpen={(e) => onOpenProjectWorkspace(p, e)}
                      />
                    ) : (
                      <Tip
                        label={path}
                        className="min-w-0 flex-1 truncate font-mono text-meta text-muted"
                      >
                        {shortPath(path, 40)}
                      </Tip>
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
                      <ul className="divide-y divide-border/60">
                        {kids.map((s) => {
                          const isSel = selected.has(s.id);
                          const record = normalizeOpenPath(s.path);
                          return (
                            <li
                              key={s.id}
                              className="flex items-center gap-2 px-3 py-2 pl-10"
                            >
                              {showDelete && (
                                <input
                                  type="checkbox"
                                  className="h-3.5 w-3.5 shrink-0 accent-[var(--accent)]"
                                  checked={isSel}
                                  onChange={() => onToggleOne(s.id)}
                                  aria-label={t('projects.tree.selectSession', { title: s.title })}
                                />
                              )}
                              <div className="flex min-w-0 flex-1 items-center gap-2">
                                <Tip
                                  label={titleHoverLabel(s.title, s.preview)}
                                  className="min-w-0 max-w-[22rem] shrink"
                                >
                                  <span className="block truncate text-sm text-primary">
                                    {s.title}
                                  </span>
                                </Tip>
                                <span className="shrink-0 text-xs text-muted tabular-nums">
                                  {relativeTime(s.updatedAt, t)} · {fmtBytes(s.sizeBytes)}
                                  {s.messageCount != null && s.messageCount > 0
                                    ? t('projects.tree.lines', { n: s.messageCount })
                                    : ''}
                                </span>
                                {record ? (
                                  <PathLink
                                    path={record}
                                    disabled={busy}
                                    ariaLabel={t('projects.tree.locateRecord', { path: record })}
                                    onOpen={(e) => onOpenSessionRecord(s, e)}
                                  />
                                ) : null}
                              </div>
                              <div className="flex shrink-0 gap-1">
                                {(() => {
                                  const sid = nativeSessionId(s);
                                  const resume = nativeResumeCommand(s);
                                  return (
                                    <>
                                      {sid ? (
                                        <Button
                                          size="icon"
                                          variant="ghost"
                                          disabled={busy}
                                          aria-label={t('projects.tree.copySessionId', { id: sid })}
                                          title={t('projects.tree.copySessionId', { id: sid })}
                                          onClick={(e) => onCopySessionId(s, e)}
                                        >
                                          <Copy className="h-3.5 w-3.5" />
                                        </Button>
                                      ) : null}
                                      {resume ? (
                                        <Button
                                          size="icon"
                                          variant="ghost"
                                          disabled={busy}
                                          aria-label={t('projects.tree.copyResumeCommand', {
                                            command: resume,
                                          })}
                                          title={t('projects.tree.copyResumeCommand', {
                                            command: resume,
                                          })}
                                          onClick={(e) => onCopyResumeCommand(s, e)}
                                        >
                                          <Terminal className="h-3.5 w-3.5" />
                                        </Button>
                                      ) : null}
                                    </>
                                  );
                                })()}
                                <Button
                                  size="sm"
                                  variant="outline"
                                  disabled={busy}
                                  onClick={() => onGoContinue(s)}
                                >
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
                                    onClick={() => onRequestDelete(s)}
                                  >
                                    <Trash2 className="h-3.5 w-3.5" />
                                  </Button>
                                )}
                              </div>
                            </li>
                          );
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

function PathLink({
  path,
  disabled,
  ariaLabel,
  onOpen,
}: {
  path: string;
  disabled?: boolean;
  ariaLabel: string;
  onOpen: (e: ReactMouseEvent) => void;
}) {
  return (
    <span className="min-w-0 flex-1">
      <Hint label={path}>
        <button
          type="button"
          className="max-w-full truncate text-left font-mono text-meta text-accent underline-offset-2 hover:underline disabled:cursor-not-allowed disabled:text-muted disabled:no-underline"
          disabled={disabled}
          aria-label={ariaLabel}
          onClick={(e) => {
            e.stopPropagation();
            onOpen(e);
          }}
        >
          {shortPath(path, 40)}
        </button>
      </Hint>
    </span>
  );
}
