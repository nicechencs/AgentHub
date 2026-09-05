import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
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
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import { AgentTabStrip, type AgentTabId } from '@/components/layout/AgentTabStrip';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { PageRefreshButton } from '@/components/shared/PageRefreshButton';
import { SearchField } from '@/components/shared/SearchField';
import {
  closeConfirmationOnOpenChange,
  preventBusyConfirmationDismissal,
} from '@/components/shared/busy-confirmation';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { ListSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName } from '@/config/agents';
import {
  deleteAgentSession,
  deleteAgentSessions,
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
  readCachedProjectList,
  rememberProjectAgent,
  rememberedProjectAgent,
  shouldShowProjectListSkeleton,
  useAgentProjectList,
  useProjectShowHidden,
} from '@/lib/hooks/useProjects';
import { normalizeOpenPath, verifiedProjectWorkspacePath } from '@/lib/path-open';
import { loadString, saveString, StorageKey } from '@/lib/ui-preferences';
import type { AgentKey, AgentProject, AgentSession } from '@/lib/types';
import { cn } from '@/lib/utils';
import { nativeResumeCommand, nativeSessionId, shortSessionId } from './project-format';
import { buildContinuePrompt, buildSummaryPrompt } from './project-prompts';
import {
  resolveInitialProjectAgentId,
  resolveProjectFetchAgentId,
  resolveProjectTabAgents,
} from './project-tab-agents';
import {
  groupProjectsByPath,
  parseProjectSortKey,
  sortProjectGroups,
  sortSessions,
  type ProjectGroup,
  type ProjectSortKey,
} from './project-groups';
import { ProjectConversationPreviewPanel } from './ProjectConversationPreviewPanel';
import { ProjectTree } from './ProjectTree';
import { nestSessions } from './session-nest';
import {
  allVisibleSessionsSelected,
  collectSelectableSessions,
  filterVisibleProjects,
  nextSelectedForToggleAllVisible,
  toggleSelectedSession,
  visibleSessionsForProject,
} from './projects-list-model';


const PROJECTS_PREVIEW_WIDTH_KEY = StorageKey.projectsPreviewWidth;

export default function ProjectsPage() {
  const { t } = useI18n();
  const { toast } = useToast();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { installedAgents, hiddenIds, loading: agentsLoading } = useInstalledAgents();
  const { showHidden, ready: hiddenReady, setShowHidden } = useProjectShowHidden();

  const agentFromUrl = searchParams.get('agent') as AgentTabId | null;
  const tabAgents = useMemo(
    () => resolveProjectTabAgents(installedAgents, hiddenIds),
    [installedAgents, hiddenIds],
  );
  const tabAgentIds = useMemo(() => tabAgents.map((agent) => agent.id), [tabAgents]);

  const [agentId, setAgentId] = useState<AgentTabId>(() =>
    resolveInitialProjectAgentId(agentFromUrl, tabAgents, rememberedProjectAgent()),
  );
  const [sortKey, setSortKey] = useState<ProjectSortKey>(() =>
    parseProjectSortKey(loadString(StorageKey.projectsListSort, 'time')),
  );

  /** Lazy-loaded sessions keyed by project id */
  const [sessionsByProject, setSessionsByProject] = useState<Record<string, AgentSession[]>>({});
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [nestedOpen, setNestedOpen] = useState<Set<string>>(new Set());
  const [loadingProjectIds, setLoadingProjectIds] = useState<Set<string>>(new Set());
  const [search, setSearch] = useState('');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<AgentSession | null>(null);
  const [batchDeleteOpen, setBatchDeleteOpen] = useState(false);
  const preview = useSideSplit<AgentSession>({ storageKey: PROJECTS_PREVIEW_WIDTH_KEY });
  const previewBodyRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (agentId) rememberProjectAgent(agentId);
  }, [agentId]);

  const canDeleteFor = useCallback(
    (id: AgentKey) => {
      const caps = installedAgents.find((agent) => agent.id === id)?.capabilities;
      return isCapabilityUsable(caps?.projectDelete);
    },
    [installedAgents],
  );
  const showDelete = tabAgents.some((agent) => canDeleteFor(agent.id));
  const deleteHintFor = useCallback(
    (id: AgentKey) => {
      if (canDeleteFor(id)) return null;
      if (id === 'zcode' || showDelete) {
        return t('projects.tree.deleteInAgent', { name: agentDisplayName(id) });
      }
      return null;
    },
    [canDeleteFor, showDelete, t],
  );

  useEffect(() => {
    if (!agentFromUrl || agentFromUrl === agentId) return;
    if (agentFromUrl === 'all' || tabAgents.some((a) => a.id === agentFromUrl)) {
      rememberProjectAgent(agentFromUrl);
      setAgentId(agentFromUrl);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only react to URL
  }, [agentFromUrl]);

  useEffect(() => {
    if (agentsLoading || tabAgents.length === 0) return;
    if (agentId === 'all' || tabAgents.some((a) => a.id === agentId)) return;
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
    // eslint-disable-next-line react-hooks/exhaustive-deps -- URL write is a one-shot fallback
  }, [agentsLoading, tabAgents, agentId]);

  const fetchScope = resolveProjectFetchAgentId(tabAgents, agentId);
  const listEnabled =
    hiddenReady && Boolean(fetchScope) && (agentsLoading || tabAgentIds.length > 0);
  const {
    data,
    error,
    loading: listLoading,
    reload,
    replaceProjectListFromMutation,
  } = useAgentProjectList(fetchScope, showHidden, listEnabled, tabAgentIds);
  const projects = data ?? [];
  const projectCounts = useMemo(() => {
    const next: Partial<Record<AgentTabId, number | undefined>> = {};
    for (const id of tabAgentIds) {
      const rows = readCachedProjectList(id, showHidden);
      if (rows) next[id] = rows.length;
    }
    if (fetchScope && fetchScope !== 'all' && data) next[fetchScope] = data.length;
    const allRows =
      fetchScope === 'all' && data
        ? data
        : tabAgentIds.every((id) => next[id] != null)
          ? tabAgentIds.flatMap((id) => readCachedProjectList(id, showHidden) ?? [])
          : null;
    if (allRows) next.all = groupProjectsByPath(allRows, true).length;
    return next;
  }, [data, fetchScope, showHidden, tabAgentIds]);
  const showListSkeleton = shouldShowProjectListSkeleton({
    listLoading,
    data,
    error,
    agentsLoading,
    hiddenReady,
  });

  const resetTree = useCallback(() => {
    setExpanded(new Set());
    setNestedOpen(new Set());
    setSessionsByProject({});
    setSelected(new Set());
    setLoadingProjectIds(new Set());
    preview.reset();
  }, [preview.reset]);

  const setAgent = (id: AgentTabId) => {
    rememberProjectAgent(id);
    setAgentId(id);
    resetTree();
    setSearch('');
    const next = new URLSearchParams(searchParams);
    next.set('agent', id);
    setSearchParams(next, { replace: true });
  };

  const changeSort = (value: string) => {
    const next = parseProjectSortKey(value);
    setSortKey(next);
    saveString(StorageKey.projectsListSort, next);
  };

  const reloadProjects = () => {
    if (!fetchScope) return Promise.resolve();
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

  async function toggleExpand(group: ProjectGroup) {
    const expandable = group.members.filter(
      (member) => member.sessionCount > 0 || member.agentId !== 'cursor',
    );
    if (expandable.length === 0) {
      toast({
        title: t('projects.toast.cursorNoTranscript'),
        variant: 'danger',
      });
      return;
    }
    const isOpen = expanded.has(group.id);
    if (isOpen) {
      setExpanded((prev) => {
        const next = new Set(prev);
        next.delete(group.id);
        return next;
      });
      const kids = group.members.flatMap((member) => sessionsByProject[member.id] ?? []);
      if (kids.length > 0) {
        setSelected((prev) => {
          const next = new Set(prev);
          for (const s of kids) next.delete(s.id);
          return next;
        });
      }
      if (preview.target && group.members.some((member) => member.id === preview.target?.projectId)) {
        preview.close();
      }
      return;
    }
    setExpanded((prev) => new Set(prev).add(group.id));
    await Promise.all(
      expandable
        .filter((member) => !(member.id in sessionsByProject))
        .map((member) => loadSessionsFor(member)),
    );
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

  async function toggleHideProject(group: ProjectGroup, e: React.MouseEvent) {
    e.stopPropagation();
    setBusy(true);
    const nextHidden = !group.hidden;
    try {
      await Promise.all(
        group.members.map((member) => upsertProjectMeta(member.id, { hidden: nextHidden })),
      );
      toast({
        title: nextHidden ? t('projects.toast.hidden') : t('projects.toast.unhidden'),
        variant: 'success',
      });
      await reloadProjects();
    } catch (err) {
      toast({ title: err instanceof Error ? err.message : String(err), variant: 'danger' });
    } finally {
      setBusy(false);
    }
  }

  async function openProjectWorkspace(group: ProjectGroup, e: React.MouseEvent) {
    e.stopPropagation();
    const target = verifiedProjectWorkspacePath(group.primary);
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
  const mergeByPath = agentId === 'all';

  const visibleProjects = useMemo(
    () => filterVisibleProjects(projects, q, sessionsByProject),
    [projects, q, sessionsByProject],
  );

  const visibleGroups = useMemo(
    () => sortProjectGroups(groupProjectsByPath(visibleProjects, mergeByPath), sortKey),
    [visibleProjects, mergeByPath, sortKey],
  );

  const visibleSessions = useCallback(
    (groupId: string) => {
      const group = visibleGroups.find((item) => item.id === groupId);
      const rows = group
        ? group.members.flatMap((member) =>
            visibleSessionsForProject(member.id, projects, q, sessionsByProject),
          )
        : visibleSessionsForProject(groupId, projects, q, sessionsByProject);
      return sortSessions(rows, sortKey);
    },
    [visibleGroups, sessionsByProject, q, projects, sortKey],
  );

  const selectableSessions = useMemo(
    () => collectSelectableSessions(visibleGroups, expanded, visibleSessions, nestedOpen),
    [visibleGroups, expanded, visibleSessions, nestedOpen],
  );

  function toggleNested(id: string) {
    setNestedOpen((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  useEffect(() => {
    if (!q) return;
    setNestedOpen((prev) => {
      const next = new Set(prev);
      let changed = false;
      for (const group of visibleGroups) {
        if (!expanded.has(group.id)) continue;
        for (const { session, children } of nestSessions(visibleSessions(group.id))) {
          if (children.length === 0 || next.has(session.id)) continue;
          next.add(session.id);
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [q, visibleGroups, expanded, visibleSessions]);

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
      await deleteAgentSession(deleteTarget.id);
      const pid = deleteTarget.projectId;
      setSessionsByProject((prev) => {
        const kids = (prev[pid] ?? []).filter((s) => s.id !== deleteTarget.id);
        return { ...prev, [pid]: kids };
      });
      replaceProjectListFromMutation((prev) =>
        prev
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
      if (preview.target?.id === deleteTarget.id) preview.close();
      toast({ title: t('projects.toast.deleted'), variant: 'success' });
      setDeleteTarget(null);
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    } finally {
      setBusy(false);
    }
  }

  async function handleBatchDelete() {
    const ids = [...selected].filter((id) => {
      for (const kids of Object.values(sessionsByProject)) {
        const hit = kids.find((session) => session.id === id);
        if (hit) return canDeleteFor(hit.agentId);
      }
      return true;
    });
    if (ids.length === 0) return;
    setBusy(true);
    try {
      const n = await deleteAgentSessions(ids);
      const idSet = new Set(ids);
      setSessionsByProject((prev) => {
        const next: Record<string, AgentSession[]> = {};
        for (const [pid, kids] of Object.entries(prev)) {
          next[pid] = kids.filter((s) => !idSet.has(s.id));
        }
        return next;
      });
      replaceProjectListFromMutation((prev) =>
        prev
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
      if (preview.target?.id && ids.includes(preview.target.id)) preview.close();
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
    const ok = setChatBootstrap({
      agentIds: [p.agentId],
      cwd: p.cwd ?? null,
      title: p.title,
      prompt: buildContinuePrompt(p),
    });
    if (!ok) {
      toast({ title: t('projects.toast.handoffFailed'), variant: 'danger' });
      return;
    }
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
      const selectedSessions = selectableSessions.filter((item) => selected.has(item.id));
      const agentIds = [...new Set(selectedSessions.map((item) => item.agentId))];
      const name =
        agentIds.length === 1 ? agentDisplayName(agentIds[0]) : t('kind.all');
      const ok = setChatBootstrap({
        agentIds: agentIds.length > 0 ? agentIds : agentId === 'all' ? tabAgentIds : [agentId],
        cwd,
        title: t('projects.toast.summarizeTitle', { n: excerpts.length }),
        prompt: buildSummaryPrompt(name, excerpts),
      });
      if (!ok) {
        toast({ title: t('projects.toast.handoffFailed'), variant: 'danger' });
        return;
      }
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
            showAll
            allLabel={t('kind.all')}
            value={agentId}
            onChange={setAgent}
            agents={tabAgents}
            emptyLabel={t('projects.page.noAgents')}
            counts={projectCounts}
            countMode="defined"
            countTitle={(_id, n) => t('projects.page.projectCount', { n })}
            aria-label={t('projects.page.filterAria')}
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
        <div className={pageRhythm.chromeActions}>
          {selected.size > 0 && (
            <>
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
              {showDelete && (
                <Button
                  size="sm"
                  variant="dangerOutline"
                  disabled={busy}
                  onClick={() => setBatchDeleteOpen(true)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  {t('projects.page.delete', { n: selected.size })}
                </Button>
              )}
            </>
          )}
          <PageRefreshButton
            disabled={showListSkeleton || busy || tabAgents.length === 0}
            onClick={() => void reloadProjects()}
            label={t('projects.page.refresh')}
          />
        </div>
      </div>

      <div className={pageRhythm.chromeRow}>
        <SearchField
          className="min-w-[200px] max-w-sm flex-1"
          placeholder={t('projects.page.searchPlaceholder')}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <Select value={sortKey} onValueChange={changeSort}>
          <SelectTrigger className="w-[8.75rem]" aria-label={t('projects.page.sortAria')}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="time">{t('projects.page.sortTime')}</SelectItem>
            <SelectItem value="agent">{t('projects.page.sortAgent')}</SelectItem>
            <SelectItem value="name">{t('projects.page.sortName')}</SelectItem>
          </SelectContent>
        </Select>
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
      ) : visibleGroups.length === 0 ? (
        <EmptyState
          icon={FolderKanban}
          title={projects.length === 0 ? t('projects.empty.noProjects') : t('projects.empty.noMatch')}
          description={
            projects.length === 0
              ? agentId === 'all'
                ? t('projects.empty.noProjectsDescAll')
                : t('projects.empty.noProjectsDesc', { name: agentDisplayName(agentId) })
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
          groups={visibleGroups}
          showSessionAgent={mergeByPath}
          expanded={expanded}
          loadingProjectIds={loadingProjectIds}
          selected={selected}
          busy={busy}
          showDelete={showDelete}
          deleteHintFor={deleteHintFor}
          previewSessionId={preview.target?.id ?? null}
          nestedOpen={nestedOpen}
          visibleSessions={visibleSessions}
          onToggleExpand={(group) => void toggleExpand(group)}
          onToggleNested={toggleNested}
          onOpenProjectWorkspace={(group, e) => void openProjectWorkspace(group, e)}
          onToggleHideProject={(group, e) => void toggleHideProject(group, e)}
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

  const previewPanel = preview.target ? (
    <ProjectConversationPreviewPanel
      session={preview.target}
      open
      width={preview.paneWidth}
      onClose={preview.close}
      onContinue={goContinue}
      busy={busy}
      onOpenRecord={(s) => void openSessionRecord(s)}
      contentRef={previewBodyRef}
      className="h-full min-w-0 shrink-0"
    />
  ) : null;

  return (
    <>
    <WorkbenchSplitPage
      split={preview}
      resizeAria={t('projects.preview.resizeAria')}
      panel={previewPanel}
      listOverflowX="hidden"
    >
            <PageHeader
              title={t('projects.page.title')}
              description={t('projects.page.description')}
              descriptionTip={t('projects.page.descriptionTip')}
            />
      {listPane}
    </WorkbenchSplitPage>

      <Dialog
        open={!!deleteTarget}
        onOpenChange={(open) => closeConfirmationOnOpenChange(open, busy, () => setDeleteTarget(null))}
      >
        <DialogContent
          hideClose={busy}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(busy, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(busy, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(busy, event)}
        >
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

      <Dialog
        open={batchDeleteOpen}
        onOpenChange={(open) => closeConfirmationOnOpenChange(open, busy, () => setBatchDeleteOpen(false))}
      >
        <DialogContent
          hideClose={busy}
          onEscapeKeyDown={(event) => preventBusyConfirmationDismissal(busy, event)}
          onPointerDownOutside={(event) => preventBusyConfirmationDismissal(busy, event)}
          onInteractOutside={(event) => preventBusyConfirmationDismissal(busy, event)}
        >
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
    </>
  );
}
