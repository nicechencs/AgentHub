import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  ChevronDown,
  ChevronRight,
  Copy,
  EyeOff,
  FolderKanban,
  FolderOpen,
  Loader2,
  MessageSquarePlus,
  Pencil,
  Sparkles,
  Trash2,
} from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { AgentTabStrip } from '@/components/layout/AgentTabStrip';
import { AgentDot } from '@/components/shared/AgentDot';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { SearchField } from '@/components/shared/SearchField';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { ListSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { Tip } from '@/components/ui/tooltip';
import { AGENTS, AGENT_MAP } from '@/config/agents';
import {
  deleteAgentProject,
  deleteAgentProjects,
  getAgentProjectExcerpts,
  getProjectMetadata,
  listAgentProjects,
  listAgentProjectSessions,
  setShowHiddenProjects,
  upsertProjectMeta,
} from '@/lib/api/project';
import { openPathInFileManager } from '@/lib/api/skill';
import { setChatBootstrap } from '@/lib/chat-bootstrap';
import { isCapabilityUsable } from '@/lib/capability';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { normalizeOpenPath, projectOpenCandidates } from '@/lib/path-open';
import type { AgentId, AgentProject, AgentSession } from '@/lib/types';
import { cn } from '@/lib/utils';

function displayTitle(p: AgentProject): string {
  const a = p.alias?.trim();
  return a || p.title;
}

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

function relativeTime(iso: string): string {
  const t = Date.parse(iso.includes('T') ? iso : iso.replace(' ', 'T') + 'Z');
  if (Number.isNaN(t)) return '';
  const diff = Date.now() - t;
  const m = Math.floor(diff / 60000);
  if (m < 1) return '刚刚';
  if (m < 60) return `${m} 分钟前`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} 小时前`;
  const d = Math.floor(h / 24);
  if (d < 30) return `${d} 天前`;
  return new Date(t).toLocaleDateString();
}

function shortPath(p: string, max = 48): string {
  if (p.length <= max) return p;
  return `…${p.slice(-(max - 1))}`;
}

function buildSummaryPrompt(
  agentName: string,
  excerpts: { title: string; cwd?: string | null; updatedAt: string; excerpt: string }[],
): string {
  const blocks = excerpts.map((e, i) => {
    const head = [
      `### 记录 ${i + 1}: ${e.title}`,
      e.cwd ? `工作目录: ${e.cwd}` : null,
      `更新时间: ${e.updatedAt}`,
      '',
      e.excerpt || '（无正文摘录）',
    ]
      .filter(Boolean)
      .join('\n');
    return head;
  });
  return [
    `请根据以下 ${excerpts.length} 条 ${agentName} 历史会话摘录，写一份结构化总结。`,
    '',
    '要求：',
    '1. 每条记录的核心目标与结论',
    '2. 跨记录的共同主题或重复问题',
    '3. 未完成事项与建议下一步',
    '4. 若信息不足请明确标出，不要编造',
    '',
    '---',
    '',
    blocks.join('\n\n---\n\n'),
  ].join('\n');
}

function buildContinuePrompt(p: AgentSession): string {
  const bits = [
    '我想基于这条历史会话继续工作。',
    p.cwd ? `工作目录：${p.cwd}` : null,
    p.preview ? `上次话题预览：${p.preview}` : `标题：${p.title}`,
    '',
    '请先简要回顾你认为的上下文（若不确定请说明），然后问我下一步要做什么。',
  ];
  return bits.filter(Boolean).join('\n');
}

function sessionMatches(s: AgentSession, q: string): boolean {
  if (!q) return true;
  const hay = [
    s.sessionId ?? '',
    s.id,
    s.title,
    s.preview ?? '',
    s.cwd ?? '',
    s.path,
    s.relativePath,
  ]
    .join('\n')
    .toLowerCase();
  return hay.includes(q);
}

/** 原生 CLI session id（无则 null） */
function nativeSessionId(s: AgentSession): string | null {
  const sid = s.sessionId?.trim();
  return sid ? sid : null;
}

/** 展示用短 id */
function shortSessionId(id: string, max = 36): string {
  if (id.length <= max) return id;
  return `${id.slice(0, max - 1)}…`;
}

function projectMatches(p: AgentProject, q: string): boolean {
  if (!q) return true;
  const hay = [
    p.title,
    p.alias ?? '',
    p.preview ?? '',
    p.actualPath ?? '',
    p.storagePath,
    p.relativePath,
  ]
    .join('\n')
    .toLowerCase();
  return hay.includes(q);
}

export default function ProjectsPage() {
  const { toast } = useToast();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const { installedAgents, installedIds, hiddenIds, loading: agentsLoading } = useInstalledAgents();

  const agentFromUrl = searchParams.get('agent') as AgentId | null;
  const tabAgents =
    installedAgents.length > 0
      ? installedAgents
      : AGENTS.filter((a) => !hiddenIds.includes(a.id));
  /** 稳定 key，避免 installedAgents 每渲染新建数组导致计数重复拉取 */
  const tabAgentIdsKey = agentsLoading
    ? ''
    : installedIds.length > 0
      ? installedIds.join(',')
      : tabAgents.map((a) => a.id).join(',');

  const [agentId, setAgentId] = useState<AgentId>(() => {
    if (agentFromUrl && tabAgents.some((a) => a.id === agentFromUrl)) return agentFromUrl;
    return tabAgents[0]?.id ?? 'claude';
  });

  const [projects, setProjects] = useState<AgentProject[]>([]);
  /** Lazy-loaded sessions keyed by project id */
  const [sessionsByProject, setSessionsByProject] = useState<Record<string, AgentSession[]>>({});
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [loadingProjectIds, setLoadingProjectIds] = useState<Set<string>>(new Set());
  const [phase, setPhase] = useState<'loading' | 'error' | 'ready'>('loading');
  const [error, setError] = useState<unknown>(null);
  const [search, setSearch] = useState('');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<AgentSession | null>(null);
  const [batchDeleteOpen, setBatchDeleteOpen] = useState(false);
  const [showHidden, setShowHidden] = useState(false);
  const [aliasTarget, setAliasTarget] = useState<AgentProject | null>(null);
  const [aliasDraft, setAliasDraft] = useState('');
  /** 各 agent 项目数量（Tab 角标） */
  const [projectCounts, setProjectCounts] = useState<Partial<Record<AgentId, number>>>({});

  const requestIdRef = useRef(0);

  const agentCaps = installedAgents.find((a) => a.id === agentId)?.capabilities;
  const canDelete = isCapabilityUsable(agentCaps?.projectDelete);
  const showSummarize = agentId !== 'cursor';
  const showDelete = canDelete;
  const agentMeta = AGENT_MAP[agentId];

  useEffect(() => {
    if (agentFromUrl && agentFromUrl !== agentId && tabAgents.some((a) => a.id === agentFromUrl)) {
      setAgentId(agentFromUrl);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only react to URL
  }, [agentFromUrl]);

  useEffect(() => {
    if (agentsLoading || tabAgents.length === 0) return;
    if (!tabAgents.some((a) => a.id === agentId)) {
      const nextId = tabAgents[0].id;
      setAgentId(nextId);
      const next = new URLSearchParams(searchParams);
      next.set('agent', nextId);
      setSearchParams(next, { replace: true });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- URL write is a one-shot fallback
  }, [agentsLoading, tabAgents, agentId]);

  const agentVisible = tabAgents.some((a) => a.id === agentId);

  const resetTree = useCallback(() => {
    setExpanded(new Set());
    setSessionsByProject({});
    setSelected(new Set());
    setLoadingProjectIds(new Set());
  }, []);

  const setAgent = (id: AgentId) => {
    setAgentId(id);
    resetTree();
    setSearch('');
    const next = new URLSearchParams(searchParams);
    next.set('agent', id);
    setSearchParams(next, { replace: true });
  };

  const loadProjects = useCallback(
    async (id: AgentId, includeHidden: boolean) => {
      const req = ++requestIdRef.current;
      setPhase('loading');
      setError(null);
      resetTree();
      try {
        const rows = await listAgentProjects(id, includeHidden);
        if (req !== requestIdRef.current) return;
        setProjects(rows);
        setProjectCounts((prev) => ({ ...prev, [id]: rows.length }));
        setPhase('ready');
      } catch (e) {
        if (req !== requestIdRef.current) return;
        setError(e);
        setPhase('error');
      }
    },
    [resetTree],
  );

  useEffect(() => {
    void getProjectMetadata()
      .then((m) => setShowHidden(!!m.showHiddenProjects))
      .catch(() => {
        /* ignore */
      });
  }, []);

  useEffect(() => {
    if (agentsLoading) return;
    if (!agentVisible) {
      setProjects([]);
      setPhase(tabAgents.length === 0 ? 'ready' : 'loading');
      return;
    }
    void loadProjects(agentId, showHidden);
  }, [agentId, showHidden, loadProjects, agentsLoading, agentVisible, tabAgents.length]);

  /** 拉取全部 agent 项目数，角标与 Skills 工具条一致 */
  useEffect(() => {
    if (!tabAgentIdsKey) return;
    const ids = tabAgentIdsKey.split(',') as AgentId[];
    let cancelled = false;
    void listAgentProjects(null, showHidden)
      .then((rows) => {
        if (cancelled) return;
        const next: Partial<Record<AgentId, number>> = {};
        for (const id of ids) next[id] = 0;
        for (const p of rows) {
          if (ids.includes(p.agentId)) {
            next[p.agentId] = (next[p.agentId] ?? 0) + 1;
          }
        }
        setProjectCounts((prev) => ({ ...prev, ...next }));
      })
      .catch(() => {
        /* 角标失败不阻塞主列表 */
      });
    return () => {
      cancelled = true;
    };
  }, [tabAgentIdsKey, showHidden]);

  /** 本地删会话等导致列表变短时，同步当前 tab 角标 */
  useEffect(() => {
    if (phase !== 'ready') return;
    setProjectCounts((prev) => {
      if (prev[agentId] === projects.length) return prev;
      return { ...prev, [agentId]: projects.length };
    });
  }, [phase, agentId, projects.length]);

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
        title: 'Cursor 仅提供工作区目录列表，无会话 transcript',
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
        title: p.hidden ? '已取消隐藏' : '已隐藏项目',
        variant: 'success',
      });
      await loadProjects(agentId, showHidden);
    } catch (err) {
      toast({ title: err instanceof Error ? err.message : String(err), variant: 'danger' });
    } finally {
      setBusy(false);
    }
  }

  function openAliasDialog(p: AgentProject, e: React.MouseEvent) {
    e.stopPropagation();
    setAliasTarget(p);
    setAliasDraft(p.alias ?? '');
  }

  async function saveAlias() {
    if (!aliasTarget) return;
    setBusy(true);
    try {
      await upsertProjectMeta(aliasTarget.id, { alias: aliasDraft });
      toast({ title: '已保存别名', variant: 'success' });
      setAliasTarget(null);
      await loadProjects(agentId, showHidden);
    } catch (err) {
      toast({ title: err instanceof Error ? err.message : String(err), variant: 'danger' });
    } finally {
      setBusy(false);
    }
  }

  /**
   * Open project folder: try workspace (actualPath) then storagePath.
   * Paths are normalized for the new `cwd/D:/…` / forward-slash formats.
   */
  async function openProjectDir(p: AgentProject, e: React.MouseEvent) {
    e.stopPropagation();
    const candidates = projectOpenCandidates({
      actualPath: p.actualPath,
      storagePath: p.storagePath,
    });
    if (candidates.length === 0) {
      toast({ title: '没有可打开的路径', variant: 'danger' });
      return;
    }
    let lastErr: unknown;
    for (const target of candidates) {
      try {
        await openPathInFileManager(target);
        return;
      } catch (err) {
        lastErr = err;
      }
    }
    toast({
      title: lastErr instanceof Error ? lastErr.message : String(lastErr),
      variant: 'danger',
    });
  }

  async function openSessionCwd(s: AgentSession, e: React.MouseEvent) {
    e.stopPropagation();
    const target = normalizeOpenPath(s.cwd);
    if (!target) {
      toast({ title: '该会话没有可打开的工作目录', variant: 'danger' });
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
      toast({ title: '该会话没有原生 Session ID', variant: 'danger' });
      return;
    }
    try {
      await navigator.clipboard.writeText(sid);
      toast({ title: 'Session ID 已复制', description: shortSessionId(sid, 48) });
    } catch {
      toast({ title: '复制失败', variant: 'danger' });
    }
  }

  const q = search.trim().toLowerCase();

  const visibleProjects = useMemo(() => {
    return projects.filter((p) => {
      if (projectMatches(p, q)) return true;
      // Keep parent if any loaded child matches search
      if (!q) return true;
      const kids = sessionsByProject[p.id];
      if (!kids) return false;
      return kids.some((s) => sessionMatches(s, q));
    });
  }, [projects, q, sessionsByProject]);

  const visibleSessions = useCallback(
    (projectId: string) => {
      const kids = sessionsByProject[projectId] ?? [];
      if (!q) return kids;
      // If project itself matched, show all kids; else only matching kids
      const proj = projects.find((p) => p.id === projectId);
      if (proj && projectMatches(proj, q)) return kids;
      return kids.filter((s) => sessionMatches(s, q));
    },
    [sessionsByProject, q, projects],
  );

  const selectableSessions = useMemo(() => {
    const out: AgentSession[] = [];
    for (const p of visibleProjects) {
      if (!expanded.has(p.id)) continue;
      out.push(...visibleSessions(p.id));
    }
    return out;
  }, [visibleProjects, expanded, visibleSessions]);

  const allVisibleSelected =
    selectableSessions.length > 0 && selectableSessions.every((s) => selected.has(s.id));

  function toggleOne(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function toggleAllVisible() {
    if (allVisibleSelected) {
      setSelected((prev) => {
        const next = new Set(prev);
        for (const s of selectableSessions) next.delete(s.id);
        return next;
      });
      return;
    }
    setSelected((prev) => {
      const next = new Set(prev);
      for (const s of selectableSessions) next.add(s.id);
      return next;
    });
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
      setProjects((prev) =>
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
      toast({ title: '已删除记录', variant: 'success' });
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
      setProjects((prev) =>
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
      await loadProjects(agentId, showHidden);
      setSelected(new Set());
      setBatchDeleteOpen(false);
      toast({
        title: n === ids.length ? `已删除 ${n} 条记录` : `已删除 ${n}/${ids.length} 条记录`,
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
      toast({ title: '请先勾选要总结的会话', variant: 'danger' });
      return;
    }
    setBusy(true);
    try {
      const excerpts = await getAgentProjectExcerpts(ids);
      if (excerpts.length === 0) {
        toast({ title: '未能读取会话摘录', variant: 'danger' });
        return;
      }
      const cwds = excerpts.map((e) => e.cwd).filter(Boolean) as string[];
      const cwd = cwds.length > 0 && cwds.every((c) => c === cwds[0]) ? cwds[0] : null;
      const name = agentMeta?.name ?? agentId;
      setChatBootstrap({
        agentIds: [agentId],
        cwd,
        title: `总结 ${excerpts.length} 条记录`,
        prompt: buildSummaryPrompt(name, excerpts),
      });
      navigate('/chat?from=projects');
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <PageHeader
        title="项目"
        description="本机会话与工作区"
        descriptionTip="按 Agent 浏览本地项目与会话；可打开目录、继续 Chat、删除或批量总结。不调用各 CLI 原生 --resume。"
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
                    总结 ({selected.size})
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
                    删除 ({selected.size})
                  </Button>
                )}
              </>
            )}
            <Button
              size="sm"
              variant="outline"
              disabled={phase === 'loading' || busy}
              onClick={() => void loadProjects(agentId, showHidden)}
            >
              刷新
            </Button>
          </div>
        }
      />

      <div className={cn(pageRhythm.chromeRow, 'gap-3')}>
        {agentsLoading ? (
          <div className="h-9 w-64 animate-pulse rounded-card bg-hover" />
        ) : (
          <AgentTabStrip
            value={agentId}
            onChange={setAgent}
            agents={tabAgents}
            emptyLabel="尚未安装任何 Agent"
            counts={projectCounts}
            countMode="defined"
            countTitle={(_id, n) => `${n} 个项目`}
          />
        )}
        {selected.size > 0 && (
          <span className="text-xs text-muted">已选 {selected.size}</span>
        )}
        <Button
          size="sm"
          variant={showHidden ? 'outline' : 'ghost'}
          onClick={() => void toggleShowHidden()}
        >
          <EyeOff className="h-3.5 w-3.5" />
          {showHidden ? '隐藏项' : '显示隐藏'}
        </Button>
      </div>

      <div className={pageRhythm.chromeRow}>
        <SearchField
          className="min-w-[200px] max-w-sm flex-1"
          placeholder="搜索项目名、路径；已展开时可搜会话…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        {selectableSessions.length > 0 && showDelete && (
          <Button size="sm" variant="ghost" onClick={toggleAllVisible}>
            {allVisibleSelected ? '取消全选' : '全选已展开会话'}
          </Button>
        )}
      </div>

      {phase === 'loading' ? (
        <ListSkeleton rows={5} />
      ) : phase === 'error' ? (
        <ErrorState error={error} onRetry={() => void loadProjects(agentId, showHidden)} />
      ) : visibleProjects.length === 0 ? (
        <EmptyState
          icon={FolderKanban}
          title={projects.length === 0 ? '暂无项目' : '没有匹配的项目'}
          description={
            projects.length === 0
              ? `在 ${agentMeta?.name ?? agentId} 中对话后会出现在此`
              : '换关键词或清空搜索'
          }
          actionLabel={projects.length === 0 ? '刷新' : '清空搜索'}
          onAction={
            projects.length === 0
              ? () => void loadProjects(agentId, showHidden)
              : () => setSearch('')
          }
        />
      ) : (
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
                  onClick={() => canExpand && void toggleExpand(p)}
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
                          onClick={(e) => void openProjectDir(p, e)}
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
                      onClick={(e) => openAliasDialog(p, e)}
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </Button>
                    <Button
                      size="icon"
                      variant="ghost"
                      disabled={busy}
                      aria-label={p.hidden ? '取消隐藏' : '隐藏'}
                      title={p.hidden ? '取消隐藏' : '隐藏'}
                      onClick={(e) => void toggleHideProject(p, e)}
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
                                  onChange={() => toggleOne(s.id)}
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
                                          onClick={(e) => void copySessionId(s, e)}
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
                                      onClick={(e) => void openSessionCwd(s, e)}
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
                                      onClick={(e) => void copySessionId(s, e)}
                                    >
                                      <Copy className="h-3.5 w-3.5" />
                                    </Button>
                                  );
                                })()}
                                <Button
                                  size="sm"
                                  variant="outline"
                                  disabled={busy}
                                  onClick={() => goContinue(s)}
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
                                    onClick={() => setDeleteTarget(s)}
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
      )}

      <Dialog open={!!deleteTarget} onOpenChange={(o) => !o && setDeleteTarget(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>删除会话？</DialogTitle>
            <DialogDescription>
              {deleteTarget?.agentId === 'grok'
                ? '删除会话目录（含 sidecar），不可恢复；不改 Agent 配置。'
                : '删除会话日志，不可恢复；不改 Agent 配置。'}
            </DialogDescription>
          </DialogHeader>
          {deleteTarget && (
            <div className="rounded-btn bg-subtle px-3 py-2 text-sm">
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
            <Button variant="outline" disabled={busy} onClick={() => setDeleteTarget(null)}>
              取消
            </Button>
            <Button variant="danger" disabled={busy} onClick={() => void handleDeleteOne()}>
              {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
              确认删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={batchDeleteOpen} onOpenChange={setBatchDeleteOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>删除 {selected.size} 条会话？</DialogTitle>
            <DialogDescription>批量删除日志，不可恢复。</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={busy} onClick={() => setBatchDeleteOpen(false)}>
              取消
            </Button>
            <Button variant="danger" disabled={busy} onClick={() => void handleBatchDelete()}>
              {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
              确认删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={!!aliasTarget} onOpenChange={(o) => !o && setAliasTarget(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>项目别名</DialogTitle>
            <DialogDescription>
              仅存于 AgentHub，不改原生日志。留空清除。
            </DialogDescription>
          </DialogHeader>
          {aliasTarget && (
            <div className="space-y-2">
              <p className="text-xs text-muted">原标题：{aliasTarget.title}</p>
              <Input
                value={aliasDraft}
                onChange={(e) => setAliasDraft(e.target.value)}
                placeholder="显示别名"
                autoFocus
              />
            </div>
          )}
          <DialogFooter>
            <Button variant="outline" disabled={busy} onClick={() => setAliasTarget(null)}>
              取消
            </Button>
            <Button disabled={busy} onClick={() => void saveAlias()}>
              {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
              保存
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
