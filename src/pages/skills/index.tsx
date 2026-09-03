import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useSearchParams } from 'react-router-dom';
import { Plus, Store } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import {
  SkillMarkdownPreviewPanel,
  type SkillPreviewTarget,
} from './SkillMarkdownPreviewPanel';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { segmentedCountClass } from '@/components/ui/segmented-styles';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { useToast } from '@/components/ui/toast';
import { Tip } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { AGENTS, agentDisplayName } from '@/config/agents';
import {
  checkConflict,
  isMappedState,
  mapCoreSkill,
  openPathInFileManager,
  toggleSkillSync,
  type InstalledSkillDto,
} from '@/lib/api/skill';
import { useInstalledAgents, type AgentColumn } from '@/lib/hooks/useInstalledAgents';
import { normalizeOpenPath } from '@/lib/path-open';
import { shouldIgnoreListKeyboard } from '@/lib/skills/preview-keys';
import {
  runImportPrivateSkill,
  runInstallMarketSkill,
  runInstallProjectSkill,
  runInstallSkill,
  runUninstallProjectSkill,
  runUninstallSkill,
  useSkillCatalog,
  useSkillMarket,
  useSkillsCacheVersion,
} from '@/lib/hooks/useSkills';
import { useProjectSkills } from '@/lib/hooks/useProjectSkills';
import {
  fetchAgentProjectsShared,
  useProjectShowHidden,
} from '@/lib/hooks/useProjects';
import { loadString, saveString, StorageKey } from '@/lib/ui-preferences';
import { getSettings } from '@/lib/api/settings';
import { FEATURE_NOT_WIRED } from '@/lib/platform';
import type { AgentKey, AgentProject, Skill, SkillMarketSource } from '@/lib/types';
import { pageRhythm } from '@/components/layout/page-rhythm';
import {
  adoptFailedToast,
  adoptOkToast,
  batchEnableToast,
  conflictPromptToast,
  deleteSharedFailedToast,
  deleteSharedOkToast,
  disableFailedToast,
  disableOkToast,
  enableFailedToast,
  enableOkToast,
  installFailedToast,
  installNeedSourceToast,
  installOkToast,
  marketExistsToast,
  marketInstallOkToast,
  noAgentsToast,
  openPathFailedToast,
  openPathMissingToast,
  overwriteOkToast,
  removeFailedToast,
  removeOkToast,
} from './copy';
import {
  catalogRowKey,
  isPrivateSourceRow,
  isSharedCatalogRow,
  previewTargetFromCatalogRow,
  visibleCatalogRows,
} from './SkillMatrix';
import {
  allFilteredSharedSelected,
  countLibraryFilters,
  filterLibraryRows,
  filteredSharedRows,
  nextSelectedForToggleAll,
  toggleSelectedSkill,
} from './skills-library-model';
import {
  previewAfterHiddenAgent,
  previewAfterRemoveFromTool,
  previewTargetsEqual,
  resyncPreviewTarget,
} from './skills-preview-resync';
import { applyCatalogCellState, cellKey } from './skills-catalog-model';
import {
  parseSkillTab,
  type LocalFilter,
  type SkillTab,
} from './skills-preview-model';
import { SkillsLibraryPanel } from './SkillsLibraryPanel';
import { SkillsMarketPanel } from './SkillsMarketPanel';
import { SkillsProjectPanel } from './SkillsProjectPanel';
import {
  filterProjectSkillRows,
  matchProjectSkillOption,
  projectSkillOptions,
  projectSkillRowKey,
} from './skills-project-model';

const SKILLS_PREVIEW_WIDTH_KEY = StorageKey.skillsPreviewWidth;

export default function SkillsPage() {
  const { toast } = useToast();
  const { t } = useI18n();
  const [searchParams, setSearchParams] = useSearchParams();
  const tab = parseSkillTab(searchParams.get('tab'));
  const { installedAgents, hiddenIds, loading: agentsLoading } = useInstalledAgents();
  // 最优列集：仅已安装 Agent（含 Kimi 等无 skills 能力），用后端 mapStatus 解释灰色格
  // doctor 未完成时先用全量列，避免矩阵空列等待；detect 完成后未安装不占列
  const matrixAgents: AgentColumn[] = useMemo(
    () =>
      installedAgents.length > 0 || !agentsLoading
        ? installedAgents
        : AGENTS.filter((a) => !hiddenIds.includes(a.id)),
    [installedAgents, agentsLoading, hiddenIds],
  );
  const installedAgentIds = useMemo(
    () => matrixAgents.map((a) => a.id),
    [matrixAgents],
  );
  const visibleAgentIdSet = useMemo(
    () => new Set<string>(installedAgentIds),
    [installedAgentIds],
  );
  const [marketQuery, setMarketQuery] = useState('');
  const [installingMarketId, setInstallingMarketId] = useState<string | null>(null);
  const [skillMarketSource, setSkillMarketSource] = useState<SkillMarketSource>('auto');

  /** 进程内 SWR：再进 Skills 页可立刻用旧 catalog，后台 revalidate */
  const {
    data: catalog,
    error,
    loading,
    reload: load,
    setData: setCatalog,
  } = useSkillCatalog();

  const [search, setSearch] = useState('');
  const [filter, setFilter] = useState<LocalFilter>('all');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [batchSyncing, setBatchSyncing] = useState(false);
  const [pendingCells, setPendingCells] = useState<Set<string>>(new Set());
  const [installSource, setInstallSource] = useState('');
  const [installOpen, setInstallOpen] = useState(false);
  const [installTarget, setInstallTarget] = useState<'user' | 'project'>('user');
  const [projectSearch, setProjectSearch] = useState('');
  const [projectRows, setProjectRows] = useState<AgentProject[] | null>(null);
  const [projectListError, setProjectListError] = useState<unknown>(null);
  const [projectListLoading, setProjectListLoading] = useState(false);
  const [workspacePath, setWorkspacePath] = useState<string | null>(() => {
    const fromUrl = searchParams.get('workspace');
    if (fromUrl?.trim()) return fromUrl;
    const stored = loadString(StorageKey.skillsProjectWorkspace, '');
    return stored.trim() ? stored : null;
  });
  const [removeProject, setRemoveProject] = useState<{
    skillId: string;
    name: string;
    origin: string;
  } | null>(null);
  const { showHidden, ready: hiddenReady } = useProjectShowHidden();
  const [importingIds, setImportingIds] = useState<Set<string>>(new Set());
  const [importConflict, setImportConflict] = useState<{
    skillId: string;
    agentId: AgentKey;
    name: string;
  } | null>(null);
  /** 从某工具目录删除技能（不删共享库） */
  const [removeFromTool, setRemoveFromTool] = useState<{
    skillId: string;
    agentId: AgentKey;
    name: string;
    inLibrary: boolean;
  } | null>(null);
  const [removeShared, setRemoveShared] = useState<{
    skillId: string;
    name: string;
  } | null>(null);
  const [dangerBusy, setDangerBusy] = useState(false);
  const preview = useSideSplit<SkillPreviewTarget>({ storageKey: SKILLS_PREVIEW_WIDTH_KEY });
  const previewTarget = preview.target;
  const previewBodyRef = useRef<HTMLDivElement>(null);

  const market = useSkillMarket(marketQuery, { enabled: tab === 'market' });
  /** 设置页切换市场源后 invalidate → version 变化，这里同步刷新源标签 */
  const marketCacheVersion = useSkillsCacheVersion('market');

  useEffect(() => {
    if (tab !== 'market') return;
    let cancelled = false;
    void getSettings()
      .then((s) => {
        if (!cancelled) setSkillMarketSource(s.skillMarketSource ?? 'auto');
      })
      .catch(() => {
        /* keep previous */
      });
    return () => {
      cancelled = true;
    };
  }, [tab, marketCacheVersion]);

  const activeMarketProvider = market.data?.[0]?.providerId;

  const projectOptions = useMemo(
    () => projectSkillOptions(projectRows ?? []),
    [projectRows],
  );
  const selectedProject = matchProjectSkillOption(projectOptions, workspacePath);
  const projectSkills = useProjectSkills(selectedProject?.workspacePath ?? null, tab === 'project');

  const loadProjectList = useCallback(async () => {
    setProjectListLoading(true);
    try {
      const rows = await fetchAgentProjectsShared(null, showHidden);
      setProjectRows(rows);
      setProjectListError(null);
    } catch (err) {
      setProjectListError(err);
    } finally {
      setProjectListLoading(false);
    }
  }, [showHidden]);

  useEffect(() => {
    if (tab !== 'project' || !hiddenReady) return;
    void loadProjectList();
  }, [tab, hiddenReady, loadProjectList]);

  useEffect(() => {
    if (!previewTarget?.workspacePath) return;
    if (previewTarget.workspacePath === selectedProject?.workspacePath) return;
    preview.close();
  }, [previewTarget, selectedProject, preview.close]);

  const selectWorkspace = useCallback(
    (path: string) => {
      setWorkspacePath(path);
      saveString(StorageKey.skillsProjectWorkspace, path);
      const p = new URLSearchParams(searchParams);
      p.set('tab', 'project');
      p.set('workspace', path);
      setSearchParams(p, { replace: true });
    },
    [searchParams, setSearchParams],
  );

  const setTab = (next: SkillTab) => {
    const p = new URLSearchParams(searchParams);
    if (next === 'library') p.delete('tab');
    else p.set('tab', next);
    if (next === 'project' && workspacePath) p.set('workspace', workspacePath);
    else if (next !== 'project') p.delete('workspace');
    setSearchParams(p, { replace: true });
  };

  const localRows = useMemo(
    () => (catalog ? visibleCatalogRows(catalog, visibleAgentIdSet) : []),
    [catalog, visibleAgentIdSet],
  );

  const sharedCount = localRows.filter(isSharedCatalogRow).length;
  /** 仅本地（可收编）数量，不含「已在真源」的投影/副本 */
  const privateOnlyCount = localRows.filter(isPrivateSourceRow).length;
  /** Tab「用户技能」角标：列表可见行（共享库 + 只在本工具） */
  const localCount = localRows.length;

  /** 筛选角标：全量计数，不受搜索影响 */
  const filterCounts = useMemo(() => countLibraryFilters(localRows), [localRows]);

  const filtered = useMemo(
    () => filterLibraryRows(localRows, search, filter),
    [localRows, search, filter],
  );

  const filteredShared = useMemo(
    () => filteredSharedRows(filtered),
    [filtered],
  );

  const allSelected = allFilteredSharedSelected(filteredShared, selected);

  const handleToggleSelect = (skillId: string) => {
    setSelected((prev) => toggleSelectedSkill(prev, skillId));
  };

  const handleToggleSelectAll = () => {
    setSelected(nextSelectedForToggleAll(filteredShared, allSelected));
  };

  const doToggle = async (
    skillId: string,
    agentId: AgentKey,
    force = false,
    meta?: { name?: string; wasMapped?: boolean; mode?: 'link' | 'copy' },
  ) => {
    const key = cellKey(skillId, agentId);
    const agentName = agentDisplayName(agentId);
    const skillName =
      meta?.name ??
      catalog?.find((s) => s.origin === 'shared' && s.id === skillId)?.name ??
      skillId;
    setPendingCells((prev) => new Set(prev).add(key));
    try {
      const result = await toggleSkillSync(skillId, agentId, {
        force,
        mode: meta?.mode,
      });
      if (result.conflict && !force) {
        toast({
          ...conflictPromptToast(t, agentName, skillName),
          duration: 12_000,
          onAction: () => {
            void doToggle(skillId, agentId, true, {
              name: skillName,
              wasMapped: false,
              mode: meta?.mode,
            });
          },
        });
        return;
      }
      setCatalog((prev) =>
        prev
          ? applyCatalogCellState(prev, skillId, agentId, result.state)
          : prev,
      );
      if (force) {
        toast({
          ...overwriteOkToast(t, agentName, skillName),
          variant: 'success',
        });
      } else if (meta?.wasMapped) {
        toast({
          ...disableOkToast(t, agentName, skillName),
          variant: 'success',
        });
      } else if (isMappedState(result.state)) {
        toast({
          ...enableOkToast(t, agentName, skillName),
          variant: 'success',
        });
      }
    } catch (e) {
      toast({
        ...(meta?.wasMapped
          ? disableFailedToast(t, e instanceof Error ? e.message : String(e))
          : enableFailedToast(t, e instanceof Error ? e.message : String(e))),
        variant: 'danger',
      });
      // 写失败后从服务端拉齐，避免本地假状态
      await load();
    } finally {
      setPendingCells((prev) => {
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
    }
  };

  const handleCellClick = async (skill: Skill, agentId: AgentKey) => {
    const state = skill.sync[agentId];
    if (state === 'unsupported') return;
    const agentName = agentDisplayName(agentId);
    // 已同步 → 直接取消，结果由 doToggle 统一提示
    if (isMappedState(state)) {
      await doToggle(skill.id, agentId, false, { name: skill.name, wasMapped: true });
      return;
    }
    // foreign / conflict → 通知确认覆盖
    if (state === 'foreign' || state === 'conflict' || skill.conflicts.includes(agentId)) {
      try {
        const conflict =
          skill.conflicts.includes(agentId) || (await checkConflict(skill.id, agentId));
        if (conflict) {
          toast({
            ...conflictPromptToast(t, agentName, skill.name),
            duration: 12_000,
            onAction: () => {
              void doToggle(skill.id, agentId, true, { name: skill.name, wasMapped: false });
            },
          });
          return;
        }
      } catch {
        // 查询失败时直接尝试同步，由后端返回 conflict
      }
    }
    void doToggle(skill.id, agentId, false, { name: skill.name, wasMapped: false });
  };

  const handleCellProject = (
    skill: Skill,
    agentId: AgentKey,
    mode: 'link' | 'copy' | 'disable',
  ) => {
    const state = skill.sync[agentId];
    if (state === 'unsupported') return;
    if (mode === 'disable') {
      if (!isMappedState(state)) return;
      void doToggle(skill.id, agentId, false, { name: skill.name, wasMapped: true });
      return;
    }
    if (state === 'foreign' || state === 'conflict' || skill.conflicts.includes(agentId)) {
      const agentName = agentDisplayName(agentId);
      toast({
        ...conflictPromptToast(t, agentName, skill.name),
        duration: 12_000,
        onAction: () => {
          void doToggle(skill.id, agentId, true, {
            name: skill.name,
            wasMapped: false,
            mode,
          });
        },
      });
      return;
    }
    void doToggle(skill.id, agentId, false, {
      name: skill.name,
      wasMapped: false,
      mode,
    });
  };

  const handleInstall = async () => {
    if (!installSource.trim()) {
      toast({ ...installNeedSourceToast(t), variant: 'danger' });
      return;
    }
    try {
      if (installTarget === 'project') {
        if (!selectedProject) {
          toast({
            title: t('skills.toast.projectNeedWorkspace'),
            variant: 'danger',
          });
          return;
        }
        const skill = await runInstallProjectSkill(
          selectedProject.workspacePath,
          installSource.trim(),
          false,
        );
        toast({
          title: t('skills.toast.projectInstallOk'),
          description: t('skills.toast.projectInstallOkDesc', { name: skill.name }),
          variant: 'success',
          duration: 8000,
        });
        setInstallOpen(false);
        setInstallSource('');
        await projectSkills.reload();
        return;
      }
      await runInstallSkill(installSource.trim(), false);
      toast({
        ...installOkToast(t),
        variant: 'success',
        duration: 8000,
      });
      setInstallOpen(false);
      setInstallSource('');
      await load();
    } catch (e) {
      toast({
        ...installFailedToast(t, e instanceof Error ? e.message : String(e)),
        variant: 'danger',
      });
    }
  };

  const handleOpenDir = async (path: string) => {
    const target = normalizeOpenPath(path) ?? path.trim();
    if (!target) {
      toast({ ...openPathMissingToast(t), variant: 'danger' });
      return;
    }
    try {
      await openPathInFileManager(target);
    } catch (e) {
      toast({
        ...openPathFailedToast(t, e instanceof Error ? e.message : FEATURE_NOT_WIRED),
        variant: 'danger',
      });
    }
  };

  /** 勾选行：仅对所选共享技能启用到已装工具（跳过已启用/冲突/不支持，不 force） */
  const handleBatchEnable = async () => {
    if (!catalog || selected.size === 0) return;
    if (installedAgents.length === 0) {
      toast({ ...noAgentsToast(t), variant: 'danger' });
      return;
    }
    setBatchSyncing(true);
    let enabled = 0;
    let conflictSkipped = 0;
    let blockedSkipped = 0;
    let failed = 0;
    const targets = installedAgents.map((a) => a.id);
    try {
      for (const row of catalog) {
        if (!isSharedCatalogRow(row) || !selected.has(row.id)) continue;
        const skill = mapCoreSkill(row);
        for (const agentId of targets) {
          const state = skill.sync[agentId] ?? 'unsupported';
          const proj = skill.projections?.find((p) => p.agent === agentId);
          const mapStatus = proj?.mapStatus;
          if (isMappedState(state)) continue;
          if (state === 'unsupported') {
            blockedSkipped++;
            continue;
          }
          if (
            mapStatus === 'agent_unsupported' ||
            mapStatus === 'target_unavailable' ||
            mapStatus === 'private_source' ||
            mapStatus === 'agent_not_installed'
          ) {
            blockedSkipped++;
            continue;
          }
          if (
            state === 'foreign' ||
            state === 'conflict' ||
            skill.conflicts.includes(agentId) ||
            mapStatus === 'conflict'
          ) {
            conflictSkipped++;
            continue;
          }
          try {
            const result = await toggleSkillSync(skill.id, agentId, { force: false });
            if (result.conflict) {
              conflictSkipped++;
              setCatalog((prev) =>
                prev ? applyCatalogCellState(prev, skill.id, agentId, result.state) : prev,
              );
              continue;
            }
            enabled++;
            setCatalog((prev) =>
              prev ? applyCatalogCellState(prev, skill.id, agentId, result.state) : prev,
            );
          } catch {
            failed++;
          }
        }
      }
      const skipParts: string[] = [];
      if (conflictSkipped > 0) skipParts.push(t('skills.toast.skipConflict', { n: conflictSkipped }));
      if (blockedSkipped > 0) skipParts.push(t('skills.toast.skipBlocked', { n: blockedSkipped }));
      if (failed > 0) skipParts.push(t('skills.toast.skipFailed', { n: failed }));
      toast({
        ...batchEnableToast(t, enabled, failed, skipParts),
        variant: failed > 0 ? 'danger' : 'success',
        duration: 8000,
      });
      setSelected(new Set());
      await load();
    } finally {
      setBatchSyncing(false);
    }
  };

  const goLibraryAndHighlight = useCallback(() => {
    const p = new URLSearchParams(searchParams);
    p.delete('tab'); // library 为默认 tab
    setSearchParams(p, { replace: true });
  }, [searchParams, setSearchParams]);

  const handleImportPrivate = async (
    skillId: string,
    agentId: AgentKey,
    name: string,
    overwrite = false,
  ): Promise<boolean> => {
    const key = `${agentId}:${skillId}`;
    setImportingIds((prev) => new Set(prev).add(key));
    try {
      await runImportPrivateSkill(skillId, agentId, overwrite);
      const toastCopy = adoptOkToast(t, overwrite);
      toast({
        title: toastCopy.title,
        description: toastCopy.description,
        variant: 'success',
        duration: 8000,
      });
      setImportConflict(null);
      await load();
      return true;
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (!overwrite && (msg.includes('skill.conflict') || msg.toLowerCase().includes('already exists'))) {
        setImportConflict({ skillId, agentId, name });
        return false;
      }
      toast({
        ...adoptFailedToast(t, msg),
        variant: 'danger',
      });
      return false;
    } finally {
      setImportingIds((prev) => {
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
    }
  };

  const handleUninstallPrivate = (
    skillId: string,
    agentId: AgentKey,
    name?: string,
    inLibrary?: boolean,
  ) => {
    setRemoveFromTool({
      skillId,
      agentId,
      name: name ?? skillId,
      inLibrary: inLibrary ?? false,
    });
  };

  const handleDeleteFromTool = (skillId: string, agentId: AgentKey, name: string) => {
    const inLibrary = Boolean(
      catalog?.some((row) => isSharedCatalogRow(row) && row.id === skillId),
    );
    handleUninstallPrivate(skillId, agentId, name, inLibrary);
  };

  const handleDeleteShared = (row: { id: string; name?: string }) => {
    setRemoveShared({ skillId: row.id, name: row.name ?? row.id });
  };

  const confirmDeleteShared = async () => {
    if (!removeShared) return;
    const { skillId, name } = removeShared;
    setDangerBusy(true);
    try {
      await runUninstallSkill(skillId);
      toast({
        ...deleteSharedOkToast(t, name),
        variant: 'success',
      });
      setRemoveShared(null);
      if (previewTarget?.skillId === skillId) {
        preview.close();
      }
      await load();
    } catch (e) {
      toast({
        ...deleteSharedFailedToast(t, e instanceof Error ? e.message : String(e)),
        variant: 'danger',
      });
    } finally {
      setDangerBusy(false);
    }
  };

  const confirmDeleteProjectSkill = async () => {
    if (!removeProject || !selectedProject) return;
    const { skillId, name, origin } = removeProject;
    setDangerBusy(true);
    try {
      await runUninstallProjectSkill(selectedProject.workspacePath, skillId, origin);
      toast({
        title: t('skills.toast.projectDeleteOk'),
        description: t('skills.toast.projectDeleteOkDesc', { skillName: name }),
        variant: 'success',
      });
      setRemoveProject(null);
      if (previewTarget?.skillId === skillId && previewTarget.workspacePath) {
        preview.close();
      }
      await projectSkills.reload();
    } catch (e) {
      toast({
        ...removeFailedToast(t, e instanceof Error ? e.message : String(e)),
        variant: 'danger',
      });
    } finally {
      setDangerBusy(false);
    }
  };

  const confirmRemoveFromTool = async () => {
    if (!removeFromTool) return;
    const { skillId, agentId, name } = removeFromTool;
    setDangerBusy(true);
    try {
      await runUninstallSkill(skillId, agentId);
      toast({
        ...removeOkToast(t, agentDisplayName(agentId), name),
        variant: 'success',
      });
      setRemoveFromTool(null);
      if (previewTarget?.skillId === skillId) {
        const next = previewAfterRemoveFromTool(previewTarget, agentId);
        if (next === 'close') preview.close();
        else preview.open(next);
      }
      await load();
    } catch (e) {
      toast({
        ...removeFailedToast(t, e instanceof Error ? e.message : String(e)),
        variant: 'danger',
      });
    } finally {
      setDangerBusy(false);
    }
  };

  const activeKey = previewTarget?.rowKey ?? null;

  const openCatalogPreview = useCallback(
    (row: InstalledSkillDto, agentId?: AgentKey) => {
      preview.open(previewTargetFromCatalogRow(row, agentId));
    },
    [preview.open],
  );

  const openProjectPreview = useCallback(
    (row: InstalledSkillDto) => {
      if (!selectedProject) return;
      preview.open({
        skillId: row.id,
        name: row.name,
        sourceDir: row.sourceDir,
        privateAgent: null,
        includeShared: false,
        workspacePath: selectedProject.workspacePath,
        originRoot: row.origin,
        rowKey: projectSkillRowKey(row),
      });
    },
    [preview.open, selectedProject],
  );

  const selectPreviewCopy = useCallback((agentId: AgentKey | null) => {
    const current = preview.target;
    if (!current) return;
    if (agentId == null) {
      if (current.privateAgent == null) return;
      preview.open({
        ...current,
        privateAgent: null,
        sourceDir: current.libraryDir ?? current.sourceDir,
      });
      return;
    }
    const loc = (current.copies ?? []).find((copy) => copy.agentId === agentId);
    if (!loc || current.privateAgent === agentId) return;
    preview.open({ ...current, privateAgent: agentId, sourceDir: loc.sourceDir });
  }, [preview.open, preview.target]);

  useEffect(() => {
    if (!previewTarget) return;
    const next = previewAfterHiddenAgent(previewTarget, visibleAgentIdSet);
    if (next === 'keep') return;
    if (next === 'close') {
      preview.close();
      return;
    }
    preview.open(next);
  }, [previewTarget, visibleAgentIdSet, preview.close, preview.open]);

  useEffect(() => {
    if (!previewTarget) return;
    const ignoreAgentId =
      dangerBusy && removeFromTool?.skillId === previewTarget.skillId
        ? removeFromTool.agentId
        : null;
    const result = resyncPreviewTarget(previewTarget, localRows, {
      catalogReady: catalog != null,
      ignoreAgentId,
    });
    if (result === 'keep') return;
    if (result === 'close') {
      preview.close();
      return;
    }
    if (previewTargetsEqual(result, previewTarget)) return;
    preview.open(result);
  }, [
    catalog,
    dangerBusy,
    localRows,
    previewTarget,
    removeFromTool,
    preview.close,
    preview.open,
  ]);

  /** ↑/↓ when preview open: move among currently filtered local rows (shared + private). */
  useEffect(() => {
    if (!preview.expanded || !previewTarget) return;

    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
      if (shouldIgnoreListKeyboard(e.target)) return;
      if (tab === 'library') {
        if (filtered.length === 0) return;
        e.preventDefault();
        const keys = filtered.map((row) => catalogRowKey(row));
        const cur = activeKey;
        let idx = cur ? keys.indexOf(cur) : -1;
        if (idx < 0) idx = e.key === 'ArrowDown' ? -1 : 0;
        const nextIdx =
          e.key === 'ArrowDown'
            ? Math.min(keys.length - 1, idx + 1)
            : Math.max(0, idx <= 0 ? 0 : idx - 1);
        const row = filtered[nextIdx];
        if (row) openCatalogPreview(row);
        return;
      }
      if (tab !== 'project') return;
      const projectFiltered = filterProjectSkillRows(projectSkills.data ?? [], projectSearch);
      if (projectFiltered.length === 0) return;
      e.preventDefault();
      const keys = projectFiltered.map((row) => projectSkillRowKey(row));
      const cur = activeKey;
      let idx = cur ? keys.indexOf(cur) : -1;
      if (idx < 0) idx = e.key === 'ArrowDown' ? -1 : 0;
      const nextIdx =
        e.key === 'ArrowDown'
          ? Math.min(keys.length - 1, idx + 1)
          : Math.max(0, idx <= 0 ? 0 : idx - 1);
      const row = projectFiltered[nextIdx];
      if (row) openProjectPreview(row);
    };

    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [
    preview.expanded,
    previewTarget,
    tab,
    filtered,
    activeKey,
    openCatalogPreview,
    openProjectPreview,
    projectSkills.data,
    projectSearch,
  ]);

  /** Enter on focused name already handled in row; Enter while preview open → focus document. */
  useEffect(() => {
    if (!preview.expanded) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Enter') return;
      if (shouldIgnoreListKeyboard(e.target)) return;
      // Name buttons handle their own Enter; if focus is elsewhere in the list area, focus body.
      const t = e.target;
      if (t instanceof HTMLElement && t.closest('button, a, [role="menuitem"]')) return;
      e.preventDefault();
      previewBodyRef.current?.focus();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [preview.expanded]);

  const previewPanel = previewTarget ? (
    <SkillMarkdownPreviewPanel
      target={previewTarget}
      open
      width={preview.paneWidth}
      onClose={preview.close}
      onOpenDir={(path) => void handleOpenDir(path)}
      onSelectCopy={selectPreviewCopy}
      onRemoveCopy={
        previewTarget.workspacePath
          ? () =>
              setRemoveProject({
                skillId: previewTarget.skillId,
                name: previewTarget.name ?? previewTarget.skillId,
                origin: previewTarget.originRoot ?? '.agents/skills',
              })
          : previewTarget.includeShared && !previewTarget.privateAgent
            ? () =>
                handleDeleteShared({
                  id: previewTarget.skillId,
                  name: previewTarget.name,
                })
            : previewTarget.privateAgent && !previewTarget.includeShared
              ? () =>
                  handleUninstallPrivate(
                    previewTarget.skillId,
                    previewTarget.privateAgent!,
                    previewTarget.name,
                    false,
                  )
              : undefined
      }
      removeCopyLabel={
        previewTarget.workspacePath
          ? t('skills.workspace.delete')
          : previewTarget.includeShared && !previewTarget.privateAgent
            ? t('skills.preview.removeShared')
            : undefined
      }
      contentRef={previewBodyRef}
      className="h-full min-w-0 shrink-0"
    />
  ) : null;

  return (
    <>
    <WorkbenchSplitPage
      split={preview}
      resizeAria={t('skills.preview.resizeAria')}
      panel={previewPanel}
    >
            <PageHeader
              title={t('skills.page.title')}
              description={
                tab === 'project'
                  ? selectedProject
                    ? t('skills.page.projectMeta', {
                        project: selectedProject.label,
                        n: projectSkills.data?.length ?? 0,
                      })
                    : t('skills.page.projectMetaIdle')
                  : t('skills.page.meta', {
                      shared: catalog == null ? '…' : sharedCount,
                      privateOnly: privateOnlyCount,
                    })
              }
              descriptionTip={
                tab === 'project'
                  ? t('skills.page.projectDescriptionTip')
                  : t('skills.page.descriptionTip')
              }
            />
          <Tabs value={tab} onValueChange={(v) => setTab(parseSkillTab(v))}>
            <div className={pageRhythm.chromeRow}>
              <TabsList>
                <TabsTrigger value="library" className="gap-1.5">
                  {t('skills.tabs.library')}
                  {catalog != null ? (
                    <Tip className={segmentedCountClass} label={t('skills.tabs.libraryBadge', { n: localCount })}>
                      {localCount}
                    </Tip>
                  ) : null}
                </TabsTrigger>
                <TabsTrigger value="project" className="gap-1.5">
                  {t('skills.tabs.project')}
                  {tab === 'project' && projectSkills.data != null ? (
                    <Tip
                      className={segmentedCountClass}
                      label={t('skills.tabs.projectBadge', { n: projectSkills.data.length })}
                    >
                      {projectSkills.data.length}
                    </Tip>
                  ) : null}
                </TabsTrigger>
                <TabsTrigger value="market" className="gap-1.5">
                  <Store className="h-3.5 w-3.5" />
                  {t('skills.tabs.market')}
                </TabsTrigger>
              </TabsList>
              <div className={pageRhythm.chromeActions}>
                <Button
                  size="sm"
                  onClick={() => {
                    setInstallTarget(tab === 'project' ? 'project' : 'user');
                    setInstallOpen(true);
                  }}
                  disabled={tab === 'project' && !selectedProject}
                  title={
                    tab === 'project' && !selectedProject
                      ? t('skills.dialog.pickProjectFirst')
                      : undefined
                  }
                >
                  <Plus className="h-3.5 w-3.5" /> {t('skills.page.installCta')}
                </Button>
              </div>
            </div>
            <TabsContent value="library" className="space-y-3">
              <SkillsLibraryPanel
                error={error}
                loading={loading}
                onRetry={load}
                search={search}
                onSearchChange={setSearch}
                filter={filter}
                onFilterChange={setFilter}
                filterCounts={filterCounts}
                selected={selected}
                onClearSelected={() => setSelected(new Set())}
                batchSyncing={batchSyncing}
                onBatchEnable={() => void handleBatchEnable()}
                filtered={filtered}
                allSelected={allSelected}
                pendingCells={pendingCells}
                importingIds={importingIds}
                onToggleSelect={handleToggleSelect}
                onToggleSelectAll={handleToggleSelectAll}
                onCellClick={handleCellClick}
                onCellProject={handleCellProject}
                onPreview={openCatalogPreview}
                activeKey={activeKey}
                onAdopt={(skillId, agentId, name) => {
                  void handleImportPrivate(skillId, agentId, name, false);
                }}
                onOpenDir={(path) => void handleOpenDir(path)}
                onDeleteShared={(row) => handleDeleteShared(row)}
                onDeleteFromTool={handleDeleteFromTool}
                agents={matrixAgents}
                installedAgentIds={installedAgentIds}
              />
            </TabsContent>
            <TabsContent value="project" className="space-y-3">
              <SkillsProjectPanel
                options={projectOptions}
                workspacePath={selectedProject?.workspacePath ?? null}
                onWorkspaceChange={selectWorkspace}
                projectsLoading={projectListLoading}
                projectsError={projectListError}
                onRetryProjects={() => void loadProjectList()}
                search={projectSearch}
                onSearchChange={setProjectSearch}
                rows={projectSkills.data}
                loading={projectSkills.loading}
                error={projectSkills.error}
                onRetry={() => void projectSkills.reload()}
                activeKey={previewTarget?.workspacePath ? previewTarget.rowKey ?? null : null}
                onPreview={openProjectPreview}
                onDelete={(row) =>
                  setRemoveProject({
                    skillId: row.id,
                    name: row.name,
                    origin: row.origin,
                  })
                }
              />
            </TabsContent>
            <TabsContent value="market">
              <SkillsMarketPanel
                marketQuery={marketQuery}
                onMarketQueryChange={setMarketQuery}
                skillMarketSource={skillMarketSource}
                activeMarketProvider={activeMarketProvider}
                loading={market.loading}
                error={market.error}
                onRetry={market.reload}
                items={market.data}
                installingId={installingMarketId}
                onInstall={(item) => {
                  void (async () => {
                    setInstallingMarketId(item.id);
                    try {
                      const skill = await runInstallMarketSkill(item.id, false);
                      const toastCopy = marketInstallOkToast(t, skill.name);
                      toast({
                        title: toastCopy.title,
                        description: toastCopy.description,
                        variant: 'success',
                        actionLabel: toastCopy.actionLabel,
                        onAction: goLibraryAndHighlight,
                        duration: 8000,
                      });
                      void market.reload();
                      await load();
                    } catch (e) {
                      const msg = e instanceof Error ? e.message : String(e);
                      if (msg.toLowerCase().includes('already exists')) {
                        toast({
                          ...marketExistsToast(t, msg),
                          variant: 'danger',
                        });
                      } else {
                        toast({
                          ...installFailedToast(t, msg),
                          variant: 'danger',
                        });
                      }
                    } finally {
                      setInstallingMarketId(null);
                    }
                  })();
                }}
              />
            </TabsContent>
          </Tabs>
    </WorkbenchSplitPage>

      <Dialog
        open={removeShared !== null}
        onOpenChange={(open) => !open && !dangerBusy && setRemoveShared(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('skills.dialog.deleteSharedTitle')}</DialogTitle>
            <DialogDescription>
              {removeShared
                ? t('skills.dialog.deleteSharedBody', { skillName: removeShared.name })
                : null}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="secondary"
              disabled={dangerBusy}
              onClick={() => setRemoveShared(null)}
            >
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              disabled={dangerBusy}
              onClick={() => void confirmDeleteShared()}
            >
              {dangerBusy ? t('skills.dialog.busy') : t('skills.dialog.deleteSharedConfirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={removeFromTool !== null}
        onOpenChange={(open) => !open && !dangerBusy && setRemoveFromTool(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {removeFromTool?.inLibrary
                ? t('skills.dialog.removeTitle')
                : t('skills.dialog.deleteTitle')}
            </DialogTitle>
            <DialogDescription>
              {removeFromTool &&
                (removeFromTool.inLibrary
                  ? t('skills.dialog.removeBody', {
                      agentName: agentDisplayName(removeFromTool.agentId),
                      skillName: removeFromTool.name,
                    })
                  : t('skills.dialog.deleteBody', {
                      agentName: agentDisplayName(removeFromTool.agentId),
                      skillName: removeFromTool.name,
                    }))}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="secondary"
              disabled={dangerBusy}
              onClick={() => setRemoveFromTool(null)}
            >
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              disabled={dangerBusy}
              onClick={() => void confirmRemoveFromTool()}
            >
              {dangerBusy
                ? t('skills.dialog.busy')
                : removeFromTool?.inLibrary
                  ? t('skills.dialog.removeConfirm')
                  : t('skills.dialog.deleteConfirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={removeProject !== null}
        onOpenChange={(open) => !open && !dangerBusy && setRemoveProject(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('skills.dialog.deleteProjectTitle')}</DialogTitle>
            <DialogDescription>
              {removeProject
                ? t('skills.dialog.deleteProjectBody', {
                    skillName: removeProject.name,
                    origin: removeProject.origin,
                  })
                : null}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="secondary"
              disabled={dangerBusy}
              onClick={() => setRemoveProject(null)}
            >
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              disabled={dangerBusy}
              onClick={() => void confirmDeleteProjectSkill()}
            >
              {dangerBusy ? t('skills.dialog.busy') : t('skills.dialog.deleteProjectConfirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={installOpen} onOpenChange={setInstallOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {installTarget === 'project'
                ? t('skills.dialog.installProjectTitle')
                : t('skills.dialog.installTitle')}
            </DialogTitle>
            <DialogDescription>
              {installTarget === 'project'
                ? t('skills.dialog.installProjectBody')
                : t('skills.dialog.installBody')}
            </DialogDescription>
          </DialogHeader>
          <Input
            value={installSource}
            onChange={(e) => setInstallSource(e.target.value)}
            placeholder={t('skills.dialog.installPlaceholder')}
          />
          <DialogFooter>
            <Button variant="secondary" onClick={() => setInstallOpen(false)}>
              {t('skills.dialog.conflictCancel')}
            </Button>
            <Button onClick={() => void handleInstall()}>
              {t('skills.dialog.installConfirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={importConflict !== null}
        onOpenChange={(open) => !open && setImportConflict(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('skills.dialog.importConflictTitle')}</DialogTitle>
            <DialogDescription>
              {importConflict &&
                t('skills.dialog.importConflictBody', { name: importConflict.name })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setImportConflict(null)}>
              {t('skills.dialog.conflictCancel')}
            </Button>
            <Button
              onClick={() => {
                if (!importConflict) return;
                void handleImportPrivate(
                  importConflict.skillId,
                  importConflict.agentId,
                  importConflict.name,
                  true,
                );
              }}
            >
              {t('skills.dialog.importConflictConfirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
