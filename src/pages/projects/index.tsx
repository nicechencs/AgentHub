import { useCallback, useEffect, useMemo, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  EyeOff,
  FolderKanban,
  Loader2,
  Sparkles,
  Trash2,
} from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { AgentTabStrip } from '@/components/layout/AgentTabStrip';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { SearchField } from '@/components/shared/SearchField';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { ListSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { AGENT_MAP } from '@/config/agents';
import {
  deleteAgentProject,
  deleteAgentProjects,
  getAgentProjectExcerpts,
  listAgentProjectSessions,
  setShowHiddenProjects,
  upsertProjectMeta,
} from '@/lib/api/project';
import { openPathInFileManager } from '@/lib/api/skill';
import { setChatBootstrap } from '@/lib/chat-bootstrap';
import { isCapabilityUsable } from '@/lib/capability';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import {
  invalidateProjects,
  rememberProjectAgent,
  rememberedProjectAgent,
  shouldShowProjectListSkeleton,
  useAgentProjectList,
  useProjectShowHidden,
} from '@/lib/hooks/useProjects';
import { normalizeOpenPath, verifiedProjectWorkspacePath } from '@/lib/path-open';
import type { AgentId, AgentProject, AgentSession } from '@/lib/types';
import { cn } from '@/lib/utils';
import { nativeResumeCommand, nativeSessionId, shortSessionId } from './project-format';
import { buildContinuePrompt, buildSummaryPrompt } from './project-prompts';
import {
  resolveInitialProjectAgentId,
  resolveProjectFetchAgentId,
  resolveProjectTabAgents,
} from './project-tab-agents';
import { ProjectConversationPreviewPanel } from './ProjectConversationPreviewPanel';
import { ProjectTree } from './ProjectTree';
import {
  allVisibleSessionsSelected,
  collectSelectableSessions,
  filterVisibleProjects,
  nextSelectedForToggleAllVisible,
  toggleSelectedSession,
  visibleSessionsForProject,
} from './projects-list-model';
import {
  PREVIEW_FRAME_PAD_RIGHT,
  PREVIEW_FRAME_PAD_Y,
} from './projects-preview-model';
import { useProjectPreview } from './use-project-preview';

export default function ProjectsPage() {
  const { t } = useI18n();
  const { toast } = useToast();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { installedAgents, hiddenIds, loading: agentsLoading } = useInstalledAgents();
  const { showHidden, ready: hiddenReady, setShowHidden } = useProjectShowHidden();

  const agentFromUrl = searchParams.get('agent') as AgentId | null;
  const tabAgents = resolveProjectTabAgents(installedAgents, hiddenIds);

  const [agentId, setAgentId] = useState<AgentId>(() =>
    resolveInitialProjectAgentId(agentFromUrl, tabAgents, rememberedProjectAgent()),
  );

  /** Lazy-loaded sessions keyed by project id */
  const [sessionsByProject, setSessionsByProject] = useState<Record<string, AgentSession[]>>({});
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [loadingProjectIds, setLoadingProjectIds] = useState<Set<string>>(new Set());
  const [search, setSearch] = useState('');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<AgentSession | null>(null);
  const [batchDeleteOpen, setBatchDeleteOpen] = useState(false);
  const preview = useProjectPreview();

  useEffect(() => {
    if (agentId) rememberProjectAgent(agentId);
  }, [agentId]);

  const agentCaps = installedAgents.find((a) => a.id === agentId)?.capabilities;
  const canDelete = isCapabilityUsable(agentCaps?.projectDelete);
  const showSummarize = agentId !== 'cursor';
  const showDelete = canDelete;
  const agentMeta = AGENT_MAP[agentId];

  useEffect(() => {
    if (agentFromUrl && agentFromUrl !== agentId && tabAgents.some((a) => a.id === agentFromUrl)) {
      rememberProjectAgent(agentFromUrl);
      setAgentId(agentFromUrl);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only react to URL
  }, [agentFromUrl]);

  useEffect(() => {
    if (agentsLoading || tabAgents.length === 0) return;
    if (!tabAgents.some((a) => a.id === agentId)) {
      const nextId = resolveInitialProjectAgentId(
        agentFromUrl,
        tabAgents,
        rememberedProjectAgent(),
      );
      rememberProjectAgent(nextId);
      setAgentId(nextId);
      const next = new URLSearchParams(searchParams);
      next.set('agent', nextId);
      setSearchParams(next, { replace: true });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- URL write is a one-shot fallback
  }, [agentsLoading, tabAgents, agentId]);

  const fetchAgentId = resolveProjectFetchAgentId(tabAgents, agentId);
  const listEnabled = hiddenReady && !agentsLoading && !!fetchAgentId;
  const {
    data,
    error,
    loading: listLoading,
    reload,
    setData,
  } = useAgentProjectList(fetchAgentId, showHidden, listEnabled);
  const projects = data ?? [];
  const projectCounts = useMemo(() => {
    const next: Partial<Record<AgentId, number>> = {};
    if (fetchAgentId && data) next[fetchAgentId] = data.length;
    return next;
  }, [fetchAgentId, data]);
  const showListSkeleton = shouldShowProjectListSkeleton({
    listLoading,
    data,
    error,
    agentsLoading,
    hiddenReady,
  });

  const resetTree = useCallback(() => {
    setExpanded(new Set());
    setSessionsByProject({});
    setSelected(new Set());
    setLoadingProjectIds(new Set());
    preview.reset();
  }, [preview.reset]);

  const setAgent = (id: AgentId) => {
    rememberProjectAgent(id);
    setAgentId(id);
    resetTree();
    setSearch('');
    const next = new URLSearchParams(searchParams);
    next.set('agent', id);
    setSearchParams(next, { replace: true });
  };

  const reloadProjects = () => {
    if (!fetchAgentId) return Promise.resolve();
    invalidateProjects();
    return reload();
  };

  const loadSessionsFor = useCallback(async (project: AgentProject) => {
    if (project.sessionCount === 0) {
      setSessionsByProject((prev) => ({ ...prev, [project.id]: [] }));
      return;
    }
    setLoadingProjectIds((prev) => new Set(prev).add(project.id));
    try {
      const rows = await listAgentProjectSessions(project.id);
      setSessionsByProject((prev) => ({ ...prev, [project.id]: rows }));
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
      setExpanded((prev) => {
        const next = new Set(prev);
        next.delete(project.id);
        return next;
      });
    } finally {
      setLoadingProjectIds((prev) => {
        const next = new Set(prev);
        next.delete(project.id);
        return next;
      });
    }
  }, [toast]);

  async function toggleExpand(project: AgentProject) {
    if (project.agentId === 'cursor' && project.sessionCount === 0) {
      toast({
        title: t('projects.toast.cursorNoTranscript'),
        variant: 'danger',
      });
      return;
    }
    const isOpen = expanded.has(project.id);
    if (isOpen) {
      setExpanded((prev) => {
        const next = new Set(prev);
        next.delete(project.id);
        return next;
      });
      // Drop selection under this project
      const kids = sessionsByProject[project.id] ?? [];
      if (kids.length > 0) {
        setSelected((prev) => {
          const next = new Set(prev);
          for (const s of kids) next.delete(s.id);
          return next;
        });
      }
      if (preview.session?.projectId === project.id) preview.close();
      return;
    }
    setExpanded((prev) => new Set(prev).add(project.id));
    if (!(project.id in sessionsByProject)) {
      await loadSessionsFor(project);
    }
  }

  async function toggleShowHidden() {
    const next = !showHidden;
    setShowHidden(next);
    try {
      await setShowHiddenProjects(next);
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    }
  }

  async function toggleHideProject(p: AgentProject, e: React.MouseEvent) {
    e.stopPropagation();
    setBusy(true);
    try {
      await upsertProjectMeta(p.id, { hidden: !p.hidden });
      toast({
        title: p.hidden ? t('projects.toast.unhidden') : t('projects.toast.hidden'),
        variant: 'success',
      });
      await reloadProjects();
    } catch (err) {
      toast({ title: err instanceof Error ? err.message : String(err), variant: 'danger' });
    } finally {
      setBusy(false);
    }
  }

  async function openProjectWorkspace(p: AgentProject, e: React.MouseEvent) {
    e.stopPropagation();
    const target = verifiedProjectWorkspacePath(p);
    if (!target) {
      toast({ title: t('projects.toast.pathInvalid'), variant: 'danger' });
      return;
    }
    try {
      await openPathInFileManager(target);
    } catch (err) {
      toast({
        title: err instanceof Error ? err.message : String(err),
        variant: 'danger',
      });
    }
  }

  async function openSessionRecord(s: AgentSession, e?: React.MouseEvent) {
    e?.stopPropagation();
    const target = normalizeOpenPath(s.path);
    if (!target) {
      toast({ title: t('projects.toast.noRecord'), variant: 'danger' });
      return;
    }
    try {
      await openPathInFileManager(target);
    } catch (err) {
      toast({
        title: err instanceof Error ? err.message : String(err),
        variant: 'danger',
      });
    }
  }

  async function copySessionId(s: AgentSession, e?: React.MouseEvent) {
    e?.stopPropagation();
    const sid = nativeSessionId(s);
    if (!sid) {
      toast({ title: t('projects.toast.noSessionId'), variant: 'danger' });
      return;
    }
    try {
      await navigator.clipboard.writeText(sid);
      toast({ title: t('projects.toast.sessionIdCopied'), description: shortSessionId(sid, 48) });
    } catch {
      toast({ title: t('projects.toast.copyFailed'), variant: 'danger' });
    }
  }

  async function copyResumeCommand(s: AgentSession, e?: React.MouseEvent) {
    e?.stopPropagation();
    const command = nativeResumeCommand(s);
    if (!command) {
      toast({ title: t('projects.toast.noResumeCommand'), variant: 'danger' });
      return;
    }
    try {
      await navigator.clipboard.writeText(command);
      toast({
        title: t('projects.toast.resumeCommandCopied'),
        description: command,
      });
    } catch {
      toast({ title: t('projects.toast.copyFailed'), variant: 'danger' });
    }
  }

  const q = search.trim().toLowerCase();

  const visibleProjects = useMemo(
    () => filterVisibleProjects(projects, q, sessionsByProject),
    [projects, q, sessionsByProject],
  );

  const visibleSessions = useCallback(
    (projectId: string) =>
      visibleSessionsForProject(projectId, projects, q, sessionsByProject),
    [sessionsByProject, q, projects],
  );

  const selectableSessions = useMemo(
    () => collectSelectableSessions(visibleProjects, expanded, visibleSessions),
    [visibleProjects, expanded, visibleSessions],
  );

  const allVisibleSelected = allVisibleSessionsSelected(selectableSessions, selected);

  function toggleOne(id: string) {
    setSelected((prev) => toggleSelectedSession(prev, id));
  }

  function toggleAllVisible() {
    setSelected((prev) =>
      nextSelectedForToggleAllVisible(prev, selectableSessions, allVisibleSelected),
    );
  }

  async function handleDeleteOne() {
    if (!deleteTarget) return;
    setBusy(true);
    try {
      await deleteAgentProject(deleteTarget.id);
      const pid = deleteTarget.projectId;
      setSessionsByProject((prev) => {
        const kids = (prev[pid] ?? []).filter((s) => s.id !== deleteTarget.id);
        return { ...prev, [pid]: kids };
      });
      setData((prev) =>
        (prev ?? [])
          .map((p) =>
            p.id === pid
              ? {
                  ...p,
                  sessionCount: Math.max(0, p.sessionCount - 1),
                  sizeBytes: Math.max(0, p.sizeBytes - deleteTarget.sizeBytes),
                }
              : p,
          )
          .filter((p) => p.agentId === 'cursor' || p.sessionCount > 0),
      );
      setSelected((prev) => {
        const next = new Set(prev);
        next.delete(deleteTarget.id);
        return next;
      });
      if (preview.sessionId === deleteTarget.id) preview.close();
      toast({ title: t('projects.toast.deleted'), variant: 'success' });
      setDeleteTarget(null);
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    } finally {
      setBusy(false);
    }
  }

  async function handleBatchDelete() {
    const ids = [...selected];
    if (ids.length === 0) return;
    setBusy(true);
    try {
      const n = await deleteAgentProjects(ids);
      const idSet = new Set(ids);
      setSessionsByProject((prev) => {
        const next: Record<string, AgentSession[]> = {};
        for (const [pid, kids] of Object.entries(prev)) {
          next[pid] = kids.filter((s) => !idSet.has(s.id));
        }
        return next;
      });
      setData((prev) =>
        (prev ?? [])
          .map((p) => {
            const removed = (sessionsByProject[p.id] ?? []).filter((s) => idSet.has(s.id));
            if (removed.length === 0) return p;
            const size = removed.reduce((a, s) => a + s.sizeBytes, 0);
            return {
              ...p,
              sessionCount: Math.max(0, p.sessionCount - removed.length),
              sizeBytes: Math.max(0, p.sizeBytes - size),
            };
          })
          .filter((p) => p.agentId === 'cursor' || p.sessionCount > 0),
      );
      await reloadProjects();
      setSelected(new Set());
      if (preview.sessionId && ids.includes(preview.sessionId)) preview.close();
      setBatchDeleteOpen(false);
      toast({
        title: n === ids.length
          ? t('projects.toast.deletedAll', { n })
          : t('projects.toast.deletedPartial', { n, total: ids.length }),
        variant: n === ids.length ? 'success' : 'danger',
      });
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    } finally {
      setBusy(false);
    }
  }

  function goContinue(p: AgentSession) {
    setChatBootstrap({
      agentIds: [p.agentId],
      cwd: p.cwd ?? null,
      title: p.title,
      prompt: buildContinuePrompt(p),
    });
    navigate('/chat?from=projects');
  }

  async function handleSummarize() {
    const ids = selectableSessions.filter((p) => selected.has(p.id)).map((p) => p.id);
    if (ids.length === 0) {
      toast({ title: t('projects.toast.selectToSummarize'), variant: 'danger' });
      return;
    }
    setBusy(true);
    try {
      const excerpts = await getAgentProjectExcerpts(ids);
      if (excerpts.length === 0) {
        toast({ title: t('projects.toast.excerptFailed'), variant: 'danger' });
        return;
      }
      const cwds = excerpts.map((e) => e.cwd).filter(Boolean) as string[];
      const cwd = cwds.length > 0 && cwds.every((c) => c === cwds[0]) ? cwds[0] : null;
      const name = agentMeta?.name ?? agentId;
      setChatBootstrap({
        agentIds: [agentId],
        cwd,
        title: t('projects.toast.summarizeTitle', { n: excerpts.length }),
        prompt: buildSummaryPrompt(name, excerpts),
      });
      navigate('/chat?from=projects');
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    } finally {
      setBusy(false);
    }
  }

  const listPane = (
    <>
      <div className={cn(pageRhythm.chromeRow, 'gap-3')}>
        {agentsLoading ? (
          <div className="h-9 w-64 animate-pulse rounded-card bg-hover" />
        ) : (
          <AgentTabStrip
            value={agentId}
            onChange={setAgent}
            agents={tabAgents}
            emptyLabel={t('projects.page.noAgents')}
            counts={projectCounts}
            countMode="defined"
            countTitle={(_id, n) => t('projects.page.projectCount', { n })}
          />
        )}
        {selected.size > 0 && (
          <span className="text-meta text-muted">{t('projects.page.selected', { n: selected.size })}</span>
        )}
        <Button
          size="sm"
          variant={showHidden ? 'outline' : 'ghost'}
          onClick={() => void toggleShowHidden()}
        >
          <EyeOff className="h-3.5 w-3.5" />
          {showHidden ? t('projects.page.hideItems') : t('projects.page.showHidden')}
        </Button>
      </div>

      <div className={pageRhythm.chromeRow}>
        <SearchField
          className="min-w-[200px] max-w-sm flex-1"
          placeholder={t('projects.page.searchPlaceholder')}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        {selectableSessions.length > 0 && showDelete && (
          <Button size="sm" variant="ghost" onClick={toggleAllVisible}>
            {allVisibleSelected ? t('projects.page.deselectAll') : t('projects.page.selectAllExpanded')}
          </Button>
        )}
      </div>

      {showListSkeleton ? (
        <ListSkeleton rows={5} />
      ) : error && data == null ? (
        <ErrorState error={error} onRetry={() => void reloadProjects()} />
      ) : tabAgents.length === 0 ? (
        <EmptyState
          icon={FolderKanban}
          title={t('projects.empty.noAgentsTitle')}
          description={t('projects.empty.noAgentsDesc')}
          actionLabel={t('projects.empty.goAgents')}
          onAction={() => navigate('/agents')}
        />
      ) : visibleProjects.length === 0 ? (
        <EmptyState
          icon={FolderKanban}
          title={projects.length === 0 ? t('projects.empty.noProjects') : t('projects.empty.noMatch')}
          description={
            projects.length === 0
              ? t('projects.empty.noProjectsDesc', { name: agentMeta?.name ?? agentId })
              : t('projects.empty.noMatchDesc')
          }
          actionLabel={projects.length === 0 ? t('projects.empty.refresh') : t('projects.empty.clearSearch')}
          onAction={
            projects.length === 0
              ? () => void reloadProjects()
              : () => setSearch('')
          }
        />
      ) : (
        <ProjectTree
          agentId={agentId}
          agentMeta={agentMeta}
          projects={visibleProjects}
          expanded={expanded}
          loadingProjectIds={loadingProjectIds}
          selected={selected}
          busy={busy}
          showDelete={showDelete}
          previewSessionId={preview.sessionId}
          visibleSessions={visibleSessions}
          onToggleExpand={(p) => void toggleExpand(p)}
          onOpenProjectWorkspace={(p, e) => void openProjectWorkspace(p, e)}
          onToggleHideProject={(p, e) => void toggleHideProject(p, e)}
          onToggleOne={toggleOne}
          onPreviewSession={preview.open}
          onCopySessionId={(s, e) => void copySessionId(s, e)}
          onCopyResumeCommand={(s, e) => void copyResumeCommand(s, e)}
          onOpenSessionRecord={(s, e) => void openSessionRecord(s, e)}
          onGoContinue={goContinue}
          onRequestDelete={setDeleteTarget}
        />
      )}
    </>
  );

  return (
    <div className="flex h-full min-h-0 flex-col bg-canvas">
      <div ref={preview.splitRef} className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          <div className={pageRhythm.workbenchHeader}>
            <PageHeader
              size="compact"
              title={t('projects.page.title')}
              description={t('projects.page.description')}
              descriptionTip={t('projects.page.descriptionTip')}
              actions={
                <div className="flex items-center gap-2">
                  {selected.size > 0 && (
                    <>
                      {showSummarize && (
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={busy}
                          onClick={() => void handleSummarize()}
                        >
                          {busy ? (
                            <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          ) : (
                            <Sparkles className="h-3.5 w-3.5" />
                          )}
                          {t('projects.page.summarize', { n: selected.size })}
                        </Button>
                      )}
                      {showDelete && (
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={busy}
                          className="text-danger hover:text-danger"
                          onClick={() => setBatchDeleteOpen(true)}
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                          {t('projects.page.delete', { n: selected.size })}
                        </Button>
                      )}
                    </>
                  )}
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={showListSkeleton || busy || tabAgents.length === 0}
                    onClick={() => void reloadProjects()}
                  >
                    {t('projects.page.refresh')}
                  </Button>
                </div>
              }
            />
          </div>
          <div
            className={cn(
              'min-h-0 min-w-0 flex-1 overflow-y-auto bg-canvas',
              preview.previewShellMounted
                ? cn(pageRhythm.workbenchXSplit, 'overflow-x-hidden')
                : cn(pageRhythm.workbenchX, 'overflow-x-auto'),
              pageRhythm.workbenchY,
            )}
          >
            {listPane}
          </div>
        </div>

        {preview.previewShellMounted && preview.session ? (
          <>
            <div
              role="separator"
              aria-orientation="vertical"
              aria-label={t('projects.preview.resizeAria')}
              aria-valuenow={preview.previewWidth}
              aria-valuemin={preview.valuemin}
              tabIndex={preview.previewExpanded ? 0 : -1}
              onPointerDown={preview.previewExpanded ? preview.onPreviewResizeStart : undefined}
              onDoubleClick={preview.previewExpanded ? preview.resetPreviewWidth : undefined}
              onKeyDown={preview.previewExpanded ? preview.onPreviewSeparatorKeyDown : undefined}
              className={cn(
                'group relative z-10 w-1.5 shrink-0 cursor-col-resize bg-transparent outline-none',
                'hover:bg-accent/40 focus-visible:bg-accent/40 active:bg-accent/60',
                'before:absolute before:inset-y-0 before:-left-1.5 before:-right-1.5 before:content-[""]',
                !preview.previewExpanded && 'pointer-events-none opacity-0',
              )}
            />
            <div
              className={cn('h-full min-h-0 shrink-0 overflow-hidden', preview.previewWidthTransition)}
              style={{ width: preview.previewShellWidth }}
              onTransitionEnd={preview.onPreviewPaneTransitionEnd}
            >
              <div
                className="box-border flex h-full min-h-0"
                style={{
                  width: preview.previewWidth + PREVIEW_FRAME_PAD_RIGHT,
                  paddingTop: 0,
                  paddingBottom: PREVIEW_FRAME_PAD_Y,
                  paddingRight: PREVIEW_FRAME_PAD_RIGHT,
                }}
              >
                <ProjectConversationPreviewPanel
                  session={preview.session}
                  open
                  width={preview.previewWidth}
                  onClose={preview.close}
                  onContinue={goContinue}
                  busy={busy}
                  onOpenRecord={(s) => void openSessionRecord(s)}
                  contentRef={preview.previewBodyRef}
                  className="h-full min-w-0 shrink-0"
                />
              </div>
            </div>
          </>
        ) : null}
      </div>

      <Dialog open={!!deleteTarget} onOpenChange={(o) => !o && setDeleteTarget(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('projects.dialog.deleteTitle')}</DialogTitle>
            <DialogDescription>
              {deleteTarget?.agentId === 'grok'
                ? t('projects.dialog.deleteGrok')
                : t('projects.dialog.deleteLog')}
            </DialogDescription>
          </DialogHeader>
          {deleteTarget && (
            <div className="rounded-card bg-subtle px-3 py-2 text-sm">
              <p className="font-medium">{deleteTarget.title}</p>
              {nativeSessionId(deleteTarget) && (
                <p className="mt-0.5 break-all font-mono text-xs text-muted">
                  session: {nativeSessionId(deleteTarget)}
                </p>
              )}
              <p className="mt-0.5 break-all font-mono text-xs text-muted">
                {deleteTarget.path}
              </p>
            </div>
          )}
          <DialogFooter>
            <Button variant="secondary" disabled={busy} onClick={() => setDeleteTarget(null)}>
              {t('common.cancel')}
            </Button>
            <Button variant="danger" disabled={busy} onClick={() => void handleDeleteOne()}>
              {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
              {t('projects.dialog.confirmDelete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={batchDeleteOpen} onOpenChange={setBatchDeleteOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('projects.dialog.batchTitle', { n: selected.size })}</DialogTitle>
            <DialogDescription>{t('projects.dialog.batchDesc')}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" disabled={busy} onClick={() => setBatchDeleteOpen(false)}>
              {t('common.cancel')}
            </Button>
            <Button variant="danger" disabled={busy} onClick={() => void handleBatchDelete()}>
              {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
              {t('projects.dialog.confirmDelete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
