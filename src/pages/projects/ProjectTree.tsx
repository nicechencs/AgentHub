import type { MouseEvent as ReactMouseEvent } from 'react';
import {
  ChevronDown,
  ChevronRight,
  Copy,
  EyeOff,
  FolderOpen,
  Loader2,
  MessageSquarePlus,
  Pencil,
  Trash2,
} from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Tip } from '@/components/ui/tooltip';
import type { AgentMeta } from '@/config/agents';
import { normalizeOpenPath, projectOpenCandidates } from '@/lib/path-open';
import type { AgentId, AgentProject, AgentSession } from '@/lib/types';
import { cn } from '@/lib/utils';
import { pageRhythm } from '@/components/layout/page-rhythm';
import {
  displayTitle,
  fmtBytes,
  nativeSessionId,
  relativeTime,
  shortPath,
  shortSessionId,
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
  onOpenProjectDir: (p: AgentProject, e: ReactMouseEvent) => void;
  onOpenAliasDialog: (p: AgentProject, e: ReactMouseEvent) => void;
  onToggleHideProject: (p: AgentProject, e: ReactMouseEvent) => void;
  onToggleOne: (id: string) => void;
  onCopySessionId: (s: AgentSession, e?: ReactMouseEvent) => void;
  onOpenSessionCwd: (s: AgentSession, e: ReactMouseEvent) => void;
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
  onOpenProjectDir,
  onOpenAliasDialog,
  onToggleHideProject,
  onToggleOne,
  onCopySessionId,
  onOpenSessionCwd,
  onGoContinue,
  onRequestDelete,
}: ProjectTreeProps) {
  return (
        <div className={pageRhythm.stackDense}>
          {visibleProjects.map((p) => {
            const open = expanded.has(p.id);
            const loadingKids = loadingProjectIds.has(p.id);
            const kids = open ? visibleSessions(p.id) : [];
            const canExpand = p.sessionCount > 0 || p.agentId !== 'cursor';
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
                    'flex items-start gap-2 px-3 py-3',
                    canExpand && 'cursor-pointer hover:bg-hover/40',
                  )}
                  onClick={() => canExpand && onToggleExpand(p)}
                  role={canExpand ? 'button' : undefined}
                  aria-expanded={canExpand ? open : undefined}
                >
                  <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center text-muted">
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
                    size="lg"
                    className="mt-1.5"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                      <span className="text-sm font-medium text-primary">{displayTitle(p)}</span>
                      {p.alias?.trim() && (
                        <span className="text-xs text-muted">({p.title})</span>
                      )}
                      {p.hidden && <span className="text-xs text-muted">已隐藏</span>}
                      <span className="text-xs text-muted tabular-nums">
                        {relativeTime(p.updatedAt)}
                      </span>
                      <span className="text-xs text-muted">·</span>
                      <span className="text-xs text-muted tabular-nums">
                        {p.sessionCount} 会话
                      </span>
                      <span className="text-xs text-muted">·</span>
                      <span className="text-xs text-muted tabular-nums">
                        {fmtBytes(p.sizeBytes)}
                      </span>
                    </div>
                    {p.preview && (
                      <p className="mt-1 line-clamp-2 text-xs text-secondary">{p.preview}</p>
                    )}
                    <div className="mt-1 flex flex-wrap gap-x-3 gap-y-0.5 font-mono text-2xs text-muted">
                      {p.actualPath && (
                        <Tip label={p.actualPath}>{shortPath(p.actualPath, 48)}</Tip>
                      )}
                      <Tip label={p.storagePath}>
                        {shortPath(p.relativePath || p.storagePath, 40)}
                      </Tip>
                    </div>
                  </div>
                  <div className="flex shrink-0 gap-1" onClick={(e) => e.stopPropagation()}>
                    {(() => {
                      const openTargets = projectOpenCandidates({
                        actualPath: p.actualPath,
                        storagePath: p.storagePath,
                      });
                      // 路径格式修复后仍无法得到绝对路径 → 隐藏打开图标
                      if (openTargets.length === 0) return null;
                      const primary = openTargets[0];
                      const isWorkspace =
                        !!normalizeOpenPath(p.actualPath) &&
                        normalizeOpenPath(p.actualPath) === primary;
                      return (
                        <Button
                          size="icon"
                          variant="ghost"
                          disabled={busy}
                          aria-label={isWorkspace ? '打开工作区' : '打开存储目录'}
                          title={
                            isWorkspace
                              ? `打开工作区：${primary}`
                              : `打开存储目录：${primary}`
                          }
                          onClick={(e) => onOpenProjectDir(p, e)}
                        >
                          <FolderOpen className="h-3.5 w-3.5" />
                        </Button>
                      );
                    })()}
                    <Button
                      size="icon"
                      variant="ghost"
                      disabled={busy}
                      aria-label="设置别名"
                      title="设置别名"
                      onClick={(e) => onOpenAliasDialog(p, e)}
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </Button>
                    <Button
                      size="icon"
                      variant="ghost"
                      disabled={busy}
                      aria-label={p.hidden ? '取消隐藏' : '隐藏'}
                      title={p.hidden ? '取消隐藏' : '隐藏'}
                      onClick={(e) => onToggleHideProject(p, e)}
                    >
                      <EyeOff className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                </div>

                {open && (
                  <div className="border-t border-border bg-subtle/40">
                    {loadingKids ? (
                      <div className="px-3 py-3 text-xs text-muted">加载会话…</div>
                    ) : kids.length === 0 ? (
                      <div className="px-3 py-3 text-xs text-muted">
                        {p.sessionCount === 0 ? '该项目下没有会话文件' : '没有匹配的会话'}
                      </div>
                    ) : (
                      <ul className="divide-y divide-border/60">
                        {kids.map((s) => {
                          const isSel = selected.has(s.id);
                          return (
                            <li
                              key={s.id}
                              className={cn(
                                'flex items-start gap-2 px-3 py-2.5 pl-10',
                                isSel && 'bg-accent/5',
                              )}
                            >
                              {showDelete && (
                                <input
                                  type="checkbox"
                                  className="mt-1 h-3.5 w-3.5 shrink-0 accent-[var(--accent)]"
                                  checked={isSel}
                                  onChange={() => onToggleOne(s.id)}
                                  aria-label={`选择 ${s.title}`}
                                />
                              )}
                              <div className="min-w-0 flex-1">
                                <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                                  <span className="text-sm text-primary">{s.title}</span>
                                  <span className="text-xs text-muted tabular-nums">
                                    {relativeTime(s.updatedAt)}
                                  </span>
                                  <span className="text-xs text-muted">·</span>
                                  <span className="text-xs text-muted tabular-nums">
                                    {fmtBytes(s.sizeBytes)}
                                  </span>
                                  {s.messageCount != null && s.messageCount > 0 && (
                                    <>
                                      <span className="text-xs text-muted">·</span>
                                      <span className="text-xs text-muted tabular-nums">
                                        ~{s.messageCount} 行
                                      </span>
                                    </>
                                  )}
                                </div>
                                {s.preview && (
                                  <p className="mt-0.5 line-clamp-2 text-xs text-secondary">
                                    {s.preview}
                                  </p>
                                )}
                                <div className="mt-0.5 flex flex-wrap items-center gap-x-3 gap-y-0.5 font-mono text-2xs text-muted">
                                  {(() => {
                                    const sid = nativeSessionId(s);
                                    if (!sid) return null;
                                    return (
                                      <Tip label={`原生 Session ID：${sid}`}>
                                        <button
                                          type="button"
                                          className="inline-flex max-w-full items-center gap-1 rounded-sm text-left hover:text-secondary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                                          aria-label={`复制 Session ID ${sid}`}
                                          title="点击复制原生 Session ID"
                                          onClick={(e) => onCopySessionId(s, e)}
                                        >
                                          <span className="truncate">
                                            id: {shortSessionId(sid)}
                                          </span>
                                          <Copy className="h-3 w-3 shrink-0 opacity-70" />
                                        </button>
                                      </Tip>
                                    );
                                  })()}
                                  {s.cwd && (
                                    <Tip label={s.cwd}>
                                      cwd: {shortPath(s.cwd, 36)}
                                    </Tip>
                                  )}
                                  <Tip label={s.path}>
                                    {shortPath(s.relativePath || s.path, 48)}
                                  </Tip>
                                </div>
                              </div>
                              <div className="flex shrink-0 gap-1">
                                {(() => {
                                  const cwdOpen = normalizeOpenPath(s.cwd);
                                  if (!cwdOpen) return null;
                                  return (
                                    <Button
                                      size="icon"
                                      variant="ghost"
                                      disabled={busy}
                                      aria-label="打开工作目录"
                                      title={`打开工作目录：${cwdOpen}`}
                                      onClick={(e) => onOpenSessionCwd(s, e)}
                                    >
                                      <FolderOpen className="h-3.5 w-3.5" />
                                    </Button>
                                  );
                                })()}
                                {(() => {
                                  const sid = nativeSessionId(s);
                                  if (!sid) return null;
                                  return (
                                    <Button
                                      size="icon"
                                      variant="ghost"
                                      disabled={busy}
                                      aria-label="复制 Session ID"
                                      title={`复制 Session ID：${sid}`}
                                      onClick={(e) => onCopySessionId(s, e)}
                                    >
                                      <Copy className="h-3.5 w-3.5" />
                                    </Button>
                                  );
                                })()}
                                <Button
                                  size="sm"
                                  variant="outline"
                                  disabled={busy}
                                  onClick={() => onGoContinue(s)}
                                >
                                  <MessageSquarePlus className="h-3.5 w-3.5" />
                                  继续
                                </Button>
                                {showDelete && (
                                  <Button
                                    size="icon"
                                    variant="ghost"
                                    disabled={busy}
                                    className="text-danger hover:text-danger"
                                    aria-label="删除会话"
                                    title="删除会话"
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
