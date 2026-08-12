import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type TransitionEvent as ReactTransitionEvent,
} from 'react';
import { useSearchParams } from 'react-router-dom';
import { PackageSearch, Plus, Store } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { SearchField } from '@/components/shared/SearchField';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
import {
  SkillMarkdownPreviewPanel,
  type SkillPreviewTarget,
} from '@/components/shared/SkillMarkdownPreviewPanel';
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
import { TableSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { Tip } from '@/components/ui/tooltip';
import { AGENTS, AGENT_MAP } from '@/config/agents';
import {
  checkConflict,
  isMappedState,
  isPrivateInstalledOrigin,
  openPathInFileManager,
  toggleSkillSync,
  type InstalledSkillDto,
} from '@/lib/api/skill';
import { useInstalledAgents, type AgentColumn } from '@/lib/hooks/useInstalledAgents';
import { normalizeOpenPath } from '@/lib/path-open';
import {
  privateSkillActiveKey,
  sharedSkillActiveKey,
  shouldIgnoreListKeyboard,
  skillPreviewActiveKey,
} from '@/lib/skills/preview-keys';
import {
  runImportPrivateSkill,
  runInstallMarketSkill,
  runInstallSkill,
  runUninstallSkill,
  useInstalledSkills,
  useSkillMarket,
  useSkillsCacheVersion,
  useSkillsList,
} from '@/lib/hooks/useSkills';
import { getSettings } from '@/lib/api/settings';
import { usePrefersReducedMotion } from '@/lib/motion';
import { openExternalLink } from '@/lib/open-external';
import { FEATURE_NOT_WIRED } from '@/lib/platform';
import type { AgentId, Skill, SkillMarketSource, SkillSyncState } from '@/lib/types';
import { cn } from '@/lib/utils';
import { AgentWorkspace } from './AgentWorkspace';
import { pageEdgePx, pageRhythm } from '@/components/layout/page-rhythm';
import { skillsCopy } from './copy';
import { SkillMarketTable } from './SkillMarketTable';
import { SkillMatrix } from './SkillMatrix';

const PREVIEW_WIDTH_DEFAULT = 440;
/** 正常拖拽/记忆宽度下限 */
const PREVIEW_WIDTH_MIN = 300;
/** 视口极窄时允许压到的硬底（仍可横滑看文档） */
const PREVIEW_WIDTH_FLOOR = 240;
/** 预览打开时给左侧列表预留的舒适宽度 */
const MAIN_WIDTH_MIN = 380;
/** 极窄时左侧可再让一点，避免预览被裁到消失 */
const MAIN_WIDTH_FLOOR = 280;
/** 预览卡片与窗边：水平 24 与 pageShell/workbenchX 一致 */
const PREVIEW_FRAME_PAD_RIGHT = pageEdgePx.x;
const PREVIEW_FRAME_PAD_Y = pageEdgePx.previewY;
const PREVIEW_SEPARATOR_W = pageEdgePx.separator;
const PREVIEW_WIDTH_STORAGE_KEY = 'agenthub.skills.previewWidth';
const PREVIEW_WIDTH_STEP = 16;
const PREVIEW_WIDTH_STEP_LARGE = 48;

function readStoredPreviewWidth(): number {
  if (typeof window === 'undefined') return PREVIEW_WIDTH_DEFAULT;
  try {
    const raw = window.localStorage.getItem(PREVIEW_WIDTH_STORAGE_KEY);
    const n = raw ? Number(raw) : NaN;
    if (Number.isFinite(n) && n >= PREVIEW_WIDTH_MIN) return Math.round(n);
  } catch {
    // ignore
  }
  return PREVIEW_WIDTH_DEFAULT;
}

function marketSourceLabel(source: SkillMarketSource): string {
  if (source === 'skillhub.cn') return 'skillhub.cn';
  if (source === 'skills.sh') return 'skills.sh';
  return '自动';
}

function marketHomeUrl(activeProvider: string | undefined, source: SkillMarketSource): string {
  if (activeProvider === 'skillhub.cn' || source === 'skillhub.cn') return 'https://skillhub.cn/';
  return 'https://skills.sh/';
}

/** 结果页展示的当前源名：优先实际 provider，否则用设置项 */
function marketResultLabel(
  activeProvider: string | undefined,
  source: SkillMarketSource,
): string {
  if (activeProvider === 'skills.sh' || activeProvider === 'skillhub.cn') {
    return activeProvider;
  }
  return marketSourceLabel(source);
}

/** library=本地共享库 · workspace=各agent技能 · market=市场；installed 兼容旧 URL */
const SKILL_TABS = ['library', 'workspace', 'market'] as const;
type SkillTab = (typeof SKILL_TABS)[number];

function parseSkillTab(raw: string | null): SkillTab {
  if (raw === 'installed') return 'library';
  if (raw && (SKILL_TABS as readonly string[]).includes(raw)) return raw as SkillTab;
  return 'library';
}

type SyncFilter = 'all' | 'mapped' | 'unmapped';

const FILTERS: { id: SyncFilter; label: string }[] = [
  { id: 'all', label: skillsCopy.filters.enableAll },
  { id: 'mapped', label: skillsCopy.filters.enableMapped },
  { id: 'unmapped', label: skillsCopy.filters.enableUnmapped },
];

const cellKey = (skillId: string, agentId: AgentId) => `${skillId}:${agentId}`;

/** 按真实写操作结果更新本地单元格 */
function applyCellState(
  skills: Skill[],
  skillId: string,
  agentId: AgentId,
  state: SkillSyncState,
  conflict: boolean,
): Skill[] {
  return skills.map((s) => {
    if (s.id !== skillId) return s;
    const conflicts = conflict
      ? Array.from(new Set([...s.conflicts.filter((a) => a !== agentId), agentId]))
      : s.conflicts.filter((a) => a !== agentId);
    const projections = (s.projections ?? []).map((p) =>
      p.agent === agentId
        ? {
            ...p,
            state,
            linkKind: state === 'linked' ? p.linkKind : 'none',
          }
        : p,
    );
    return {
      ...s,
      sync: { ...s.sync, [agentId]: state },
      projections,
      conflicts,
    };
  });
}

export default function SkillsPage() {
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();
  const tab = parseSkillTab(searchParams.get('tab'));
  const { installedAgents, loading: agentsLoading } = useInstalledAgents();
  const [marketQuery, setMarketQuery] = useState('');
  const [installingMarketId, setInstallingMarketId] = useState<string | null>(null);
  const [skillMarketSource, setSkillMarketSource] = useState<SkillMarketSource>('auto');

  /** 进程内 SWR：再进 Skills 页可立刻用旧矩阵，后台 revalidate */
  const {
    data: skills,
    error,
    loading,
    reload: load,
    setData: setSkills,
  } = useSkillsList();

  const [search, setSearch] = useState('');
  const [filter, setFilter] = useState<SyncFilter>('all');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [batchSyncing, setBatchSyncing] = useState(false);
  const [pendingCells, setPendingCells] = useState<Set<string>>(new Set());
  const [installSource, setInstallSource] = useState('');
  const [installOpen, setInstallOpen] = useState(false);
  const [importingIds, setImportingIds] = useState<Set<string>>(new Set());
  const [batchAdopting, setBatchAdopting] = useState(false);
  const [importConflict, setImportConflict] = useState<{
    skillId: string;
    agentId: AgentId;
    name: string;
  } | null>(null);
  /** 从某工具目录删除技能（不删共享库） */
  const [removeFromTool, setRemoveFromTool] = useState<{
    skillId: string;
    agentId: AgentId;
    name: string;
    inLibrary: boolean;
  } | null>(null);
  const [dangerBusy, setDangerBusy] = useState(false);
  const [previewTarget, setPreviewTarget] = useState<SkillPreviewTarget | null>(null);
  const [previewWidth, setPreviewWidth] = useState(readStoredPreviewWidth);
  /** 壳层是否挂载（关闭动画结束后再卸） */
  const [previewShellMounted, setPreviewShellMounted] = useState(false);
  /** 视觉展开（宽度 0 → previewWidth） */
  const [previewExpanded, setPreviewExpanded] = useState(false);
  const [previewResizing, setPreviewResizing] = useState(false);
  const reduceMotion = usePrefersReducedMotion();
  const splitRef = useRef<HTMLDivElement>(null);
  const previewBodyRef = useRef<HTMLDivElement>(null);

  const clampPreviewWidth = useCallback((w: number) => {
    const containerW = splitRef.current?.getBoundingClientRect().width ?? window.innerWidth;
    // 分隔条 + 预览右侧画布留白（宽度动画要计入，避免卡片贴死右边框）
    const chrome = PREVIEW_SEPARATOR_W + PREVIEW_FRAME_PAD_RIGHT;
    const usable = Math.max(0, containerW - chrome);
    // 够宽：保证左侧 MAIN_WIDTH_MIN；偏窄：左侧可收到 MAIN_WIDTH_FLOOR
    const mainReserve =
      usable >= MAIN_WIDTH_MIN + PREVIEW_WIDTH_MIN
        ? MAIN_WIDTH_MIN
        : Math.min(MAIN_WIDTH_MIN, Math.max(MAIN_WIDTH_FLOOR, Math.floor(usable * 0.48)));
    const maxW = Math.max(PREVIEW_WIDTH_FLOOR, usable - mainReserve);
    const minW = Math.min(PREVIEW_WIDTH_MIN, maxW);
    return Math.min(maxW, Math.max(minW, Math.round(w)));
  }, []);

  const persistPreviewWidth = useCallback((w: number) => {
    const next = clampPreviewWidth(w);
    setPreviewWidth(next);
    try {
      window.localStorage.setItem(PREVIEW_WIDTH_STORAGE_KEY, String(next));
    } catch {
      // ignore
    }
    return next;
  }, [clampPreviewWidth]);

  /** 窗口/分栏变窄时重夹预览宽，避免固定像素把正文裁死 */
  useEffect(() => {
    if (!previewShellMounted || !previewExpanded) return;
    const el = splitRef.current;
    if (!el || typeof ResizeObserver === 'undefined') {
      const onWin = () => setPreviewWidth((w) => clampPreviewWidth(w));
      window.addEventListener('resize', onWin);
      onWin();
      return () => window.removeEventListener('resize', onWin);
    }
    const ro = new ResizeObserver(() => {
      setPreviewWidth((w) => {
        const next = clampPreviewWidth(w);
        return next === w ? w : next;
      });
    });
    ro.observe(el);
    setPreviewWidth((w) => clampPreviewWidth(w));
    return () => ro.disconnect();
  }, [previewShellMounted, previewExpanded, clampPreviewWidth]);

  const requestOpenPreview = useCallback((target: SkillPreviewTarget) => {
    setPreviewTarget(target);
    setPreviewShellMounted(true);
    if (reduceMotion) {
      setPreviewExpanded(true);
      return;
    }
    // 双 rAF：先挂上 width:0，再扩到目标宽，触发 transition
    requestAnimationFrame(() => {
      requestAnimationFrame(() => setPreviewExpanded(true));
    });
  }, [reduceMotion]);

  const requestClosePreview = useCallback(() => {
    setPreviewExpanded(false);
    if (reduceMotion) {
      setPreviewTarget(null);
      setPreviewShellMounted(false);
    }
  }, [reduceMotion]);

  const onPreviewPaneTransitionEnd = useCallback(
    (e: ReactTransitionEvent<HTMLElement>) => {
      if (e.propertyName !== 'width') return;
      if (previewExpanded) return;
      setPreviewTarget(null);
      setPreviewShellMounted(false);
    },
    [previewExpanded],
  );

  const onPreviewResizeStart = useCallback(
    (e: ReactPointerEvent<HTMLDivElement>) => {
      e.preventDefault();
      const startX = e.clientX;
      const startW = previewWidth;

      const prevCursor = document.body.style.cursor;
      const prevSelect = document.body.style.userSelect;
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      setPreviewResizing(true);

      const onMove = (ev: PointerEvent) => {
        // Dragging the left edge of the preview: mouse left → wider panel
        setPreviewWidth(clampPreviewWidth(startW + (startX - ev.clientX)));
      };
      const onUp = (ev: PointerEvent) => {
        persistPreviewWidth(startW + (startX - ev.clientX));
        document.body.style.cursor = prevCursor;
        document.body.style.userSelect = prevSelect;
        setPreviewResizing(false);
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
      };

      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
    },
    [previewWidth, clampPreviewWidth, persistPreviewWidth],
  );

  const onPreviewSeparatorKeyDown = useCallback(
    (e: ReactKeyboardEvent<HTMLDivElement>) => {
      if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
        e.preventDefault();
        const step = e.shiftKey ? PREVIEW_WIDTH_STEP_LARGE : PREVIEW_WIDTH_STEP;
        // Left arrow widens preview (same as drag left)
        const delta = e.key === 'ArrowLeft' ? step : -step;
        persistPreviewWidth(previewWidth + delta);
      } else if (e.key === 'Home') {
        e.preventDefault();
        persistPreviewWidth(PREVIEW_WIDTH_MIN);
      } else if (e.key === 'End') {
        e.preventDefault();
        const containerW = splitRef.current?.getBoundingClientRect().width ?? window.innerWidth;
        persistPreviewWidth(containerW - MAIN_WIDTH_MIN);
      }
    },
    [previewWidth, persistPreviewWidth],
  );

  const resetPreviewWidth = useCallback(() => {
    persistPreviewWidth(PREVIEW_WIDTH_DEFAULT);
  }, [persistPreviewWidth]);

  /**
   * installed / market 按需加载：
   * - workspace Tab：立刻拉 installed
   * - library：共享库矩阵出数后再延后拉（badge，不挡首屏）
   * - market：仅进入市场 Tab 才请求 skills.sh
   */
  const [installedEnabled, setInstalledEnabled] = useState(tab === 'workspace');
  useEffect(() => {
    if (tab === 'workspace') {
      setInstalledEnabled(true);
      return;
    }
    if (skills !== null) {
      const id = window.setTimeout(() => setInstalledEnabled(true), 80);
      return () => window.clearTimeout(id);
    }
  }, [tab, skills]);

  const installedQuery = useInstalledSkills({ enabled: installedEnabled });
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

  const setTab = (next: SkillTab) => {
    const p = new URLSearchParams(searchParams);
    if (next === 'library') p.delete('tab');
    else p.set('tab', next);
    setSearchParams(p, { replace: true });
  };

  /** 仅本地（可收编）数量，不含「已在真源」的投影/副本 */
  const privateOnlyCount = useMemo(
    () =>
      installedQuery.data?.filter(
        (s) =>
          isPrivateInstalledOrigin(s.origin) &&
          (s.mapStatus === 'private_source' || s.mapStatus == null),
      ).length ?? 0,
    [installedQuery.data],
  );

  /** 启用状态分段角标（与各Agent技能筛选一致：全量计数，不受搜索影响） */
  const filterCounts = useMemo(() => {
    if (!skills) return { all: 0, mapped: 0, unmapped: 0 };
    let mapped = 0;
    let unmapped = 0;
    for (const s of skills) {
      if (Object.values(s.sync).some((st) => isMappedState(st))) mapped++;
      else unmapped++;
    }
    return { all: skills.length, mapped, unmapped };
  }, [skills]);

  const filtered = useMemo(() => {
    if (!skills) return [];
    const keyword = search.trim().toLowerCase();
    return skills.filter((s) => {
      if (keyword && !s.name.toLowerCase().includes(keyword)) return false;
      const states = Object.values(s.sync);
      const hasMapped = states.some((st) => isMappedState(st));
      if (filter === 'mapped') return hasMapped;
      if (filter === 'unmapped') return !hasMapped;
      return true;
    });
  }, [skills, search, filter]);

  const allSelected = filtered.length > 0 && filtered.every((s) => selected.has(s.id));

  const handleToggleSelect = (skillId: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(skillId)) next.delete(skillId);
      else next.add(skillId);
      return next;
    });
  };

  const handleToggleSelectAll = () => {
    setSelected(allSelected ? new Set() : new Set(filtered.map((s) => s.id)));
  };

  const doToggle = async (
    skillId: string,
    agentId: AgentId,
    force = false,
    meta?: { name?: string; wasMapped?: boolean },
  ) => {
    const key = cellKey(skillId, agentId);
    const agentName = AGENT_MAP[agentId]?.name ?? agentId;
    const skillName = meta?.name ?? skills?.find((s) => s.id === skillId)?.name ?? skillId;
    setPendingCells((prev) => new Set(prev).add(key));
    try {
      const result = await toggleSkillSync(skillId, agentId, { force });
      if (result.conflict && !force) {
        toast({
          ...skillsCopy.toast.conflictPrompt(agentName, skillName),
          duration: 12_000,
          onAction: () => {
            void doToggle(skillId, agentId, true, { name: skillName, wasMapped: false });
          },
        });
        return;
      }
      setSkills((prev) =>
        prev ? applyCellState(prev, skillId, agentId, result.state, result.conflict) : prev,
      );
      if (force) {
        toast({
          ...skillsCopy.toast.overwriteOk(agentName, skillName),
          variant: 'success',
        });
      } else if (meta?.wasMapped) {
        toast({
          ...skillsCopy.toast.disableOk(agentName, skillName),
          variant: 'success',
        });
      } else if (isMappedState(result.state)) {
        toast({
          ...skillsCopy.toast.enableOk(agentName, skillName),
          variant: 'success',
        });
      }
    } catch (e) {
      toast({
        ...(meta?.wasMapped ? skillsCopy.toast.disableFailed : skillsCopy.toast.enableFailed)(
          e instanceof Error ? e.message : String(e),
        ),
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

  const handleCellClick = async (skill: Skill, agentId: AgentId) => {
    const state = skill.sync[agentId];
    if (state === 'unsupported') return;
    const agentName = AGENT_MAP[agentId]?.name ?? agentId;
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
            ...skillsCopy.toast.conflictPrompt(agentName, skill.name),
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

  const handleInstall = async () => {
    if (!installSource.trim()) {
      toast({ ...skillsCopy.toast.installNeedSource, variant: 'danger' });
      return;
    }
    try {
      await runInstallSkill(installSource.trim(), false);
      toast({
        ...skillsCopy.toast.installOk,
        variant: 'success',
        duration: 8000,
      });
      setInstallOpen(false);
      setInstallSource('');
      await load();
    } catch (e) {
      toast({
        ...skillsCopy.toast.installFailed(e instanceof Error ? e.message : String(e)),
        variant: 'danger',
      });
    }
  };

  const handleOpenDir = async (path: string) => {
    const target = normalizeOpenPath(path) ?? path.trim();
    if (!target) {
      toast({ ...skillsCopy.toast.openPathMissing, variant: 'danger' });
      return;
    }
    try {
      await openPathInFileManager(target);
    } catch (e) {
      toast({
        ...skillsCopy.toast.openPathFailed(
          e instanceof Error ? e.message : FEATURE_NOT_WIRED,
        ),
        variant: 'danger',
      });
    }
  };

  /** 勾选行：仅对所选技能启用到已装工具（跳过已启用/冲突/不支持，不 force） */
  const handleBatchEnable = async () => {
    if (!skills || selected.size === 0) return;
    if (installedAgents.length === 0) {
      toast({ ...skillsCopy.toast.noAgents, variant: 'danger' });
      return;
    }
    setBatchSyncing(true);
    let enabled = 0;
    let conflictSkipped = 0;
    let blockedSkipped = 0;
    let failed = 0;
    const targets = installedAgents.map((a) => a.id);
    try {
      for (const skill of skills) {
        if (!selected.has(skill.id)) continue;
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
              setSkills((prev) =>
                prev ? applyCellState(prev, skill.id, agentId, result.state, true) : prev,
              );
              continue;
            }
            enabled++;
            setSkills((prev) =>
              prev ? applyCellState(prev, skill.id, agentId, result.state, result.conflict) : prev,
            );
          } catch {
            failed++;
          }
        }
      }
      const skipParts: string[] = [];
      if (conflictSkipped > 0) skipParts.push(`冲突 ${conflictSkipped}`);
      if (blockedSkipped > 0) skipParts.push(`不支持/不可用 ${blockedSkipped}`);
      if (failed > 0) skipParts.push(`失败 ${failed}`);
      toast({
        ...skillsCopy.toast.batchEnable(enabled, failed, skipParts),
        variant: failed > 0 ? 'danger' : 'success',
        duration: 8000,
      });
      setSelected(new Set());
      await load();
    } finally {
      setBatchSyncing(false);
    }
  };

  // 最优列集：仅已安装 Agent（含 Kimi 等无 skills 能力），用后端 mapStatus 解释灰色格
  // doctor 未完成时先用全量列，避免矩阵空列等待；detect 完成后未安装不占列
  const matrixAgents: AgentColumn[] =
    installedAgents.length > 0 || !agentsLoading ? installedAgents : AGENTS;
  const installedAgentIds = matrixAgents.map((a) => a.id);

  const goLibraryAndHighlight = useCallback(() => {
    const p = new URLSearchParams(searchParams);
    p.delete('tab'); // library 为默认 tab
    setSearchParams(p, { replace: true });
  }, [searchParams, setSearchParams]);

  const handleImportPrivate = async (
    skillId: string,
    agentId: AgentId,
    name: string,
    overwrite = false,
  ): Promise<boolean> => {
    const key = `${agentId}:${skillId}`;
    setImportingIds((prev) => new Set(prev).add(key));
    try {
      await runImportPrivateSkill(skillId, agentId, overwrite);
      const t = skillsCopy.toast.adoptOk(overwrite);
      toast({
        title: t.title,
        description: t.description,
        variant: 'success',
        actionLabel: t.actionLabel,
        onAction: goLibraryAndHighlight,
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
        ...skillsCopy.toast.adoptFailed(msg),
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

  const handleBatchAdopt = async (
    items: { skillId: string; agentId: AgentId; name: string }[],
  ) => {
    if (items.length === 0) return;
    setBatchAdopting(true);
    let ok = 0;
    let failed = 0;
    let conflict = 0;
    try {
      for (const item of items) {
        const key = `${item.agentId}:${item.skillId}`;
        setImportingIds((prev) => new Set(prev).add(key));
        try {
          await runImportPrivateSkill(item.skillId, item.agentId, false);
          ok++;
        } catch (e) {
          const msg = e instanceof Error ? e.message : String(e);
          if (msg.includes('skill.conflict') || msg.toLowerCase().includes('already exists')) {
            conflict++;
            setImportConflict({
              skillId: item.skillId,
              agentId: item.agentId,
              name: item.name,
            });
          } else {
            failed++;
          }
        } finally {
          setImportingIds((prev) => {
            const next = new Set(prev);
            next.delete(key);
            return next;
          });
        }
      }
      {
        const t = skillsCopy.toast.batchAdopt(ok, conflict, failed);
        toast({
          title: t.title,
          description: t.description,
          variant: failed > 0 ? 'danger' : 'success',
          actionLabel: t.actionLabel,
          onAction: t.actionLabel ? goLibraryAndHighlight : undefined,
          duration: ok > 0 ? 8000 : 5000,
        });
      }
      await load();
    } finally {
      setBatchAdopting(false);
    }
  };

  const handleUninstallPrivate = (
    skillId: string,
    agentId: AgentId,
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

  const confirmRemoveFromTool = async () => {
    if (!removeFromTool) return;
    const { skillId, agentId, name } = removeFromTool;
    setDangerBusy(true);
    try {
      await runUninstallSkill(skillId, agentId);
      toast({
        ...skillsCopy.toast.removeOk(AGENT_MAP[agentId]?.name ?? agentId, name),
        variant: 'success',
      });
      setRemoveFromTool(null);
      await load();
    } catch (e) {
      toast({
        ...skillsCopy.toast.removeFailed(e instanceof Error ? e.message : String(e)),
        variant: 'danger',
      });
    } finally {
      setDangerBusy(false);
    }
  };

  const activeKey = previewTarget ? skillPreviewActiveKey(previewTarget) : null;
  /** 动画壳宽 = 卡片宽 + 右侧画布 gutter（上下 padding 在壳内，不占横向） */
  const previewShellWidth = previewExpanded
    ? previewWidth + PREVIEW_FRAME_PAD_RIGHT
    : 0;
  const previewWidthTransition =
    !previewResizing && !reduceMotion ? 'motion-panel-width' : 'transition-none';

  const openSharedPreview = useCallback(
    (skill: Skill) => {
      requestOpenPreview({
        skillId: skill.id,
        name: skill.name,
        sourceDir: skill.sourceDir,
        privateAgent: null,
      });
    },
    [requestOpenPreview],
  );

  const openPrivatePreview = useCallback(
    (skill: InstalledSkillDto) => {
      requestOpenPreview({
        skillId: skill.id,
        name: skill.name,
        sourceDir: skill.sourceDir,
        privateAgent:
          skill.origin && skill.origin !== 'shared' ? (skill.origin as AgentId) : null,
      });
    },
    [requestOpenPreview],
  );

  /** ↑/↓ when preview open: move active among currently visible list (library / workspace). */
  useEffect(() => {
    if (!previewExpanded || !previewTarget) return;

    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
      if (shouldIgnoreListKeyboard(e.target)) return;
      if (tab !== 'library' && tab !== 'workspace') return;

      if (tab === 'library') {
        if (filtered.length === 0) return;
        e.preventDefault();
        const ids = filtered.map((s) => s.id);
        const currentId =
          previewTarget && !previewTarget.privateAgent ? previewTarget.skillId : null;
        let idx = currentId ? ids.indexOf(currentId) : -1;
        if (idx < 0) idx = e.key === 'ArrowDown' ? -1 : 0;
        const nextIdx =
          e.key === 'ArrowDown'
            ? Math.min(ids.length - 1, idx + 1)
            : Math.max(0, idx <= 0 ? 0 : idx - 1);
        const skill = filtered[nextIdx];
        if (skill) openSharedPreview(skill);
        return;
      }

      // workspace: navigate private installed rows that match active private scope
      const rows = (installedQuery.data ?? []).filter(
        (s) => isPrivateInstalledOrigin(s.origin),
      );
      if (rows.length === 0) return;
      e.preventDefault();
      const keys = rows.map((s) => privateSkillActiveKey(s.origin as AgentId, s.id));
      const cur = activeKey && activeKey.startsWith('agent:') ? activeKey : null;
      let idx = cur ? keys.indexOf(cur) : -1;
      if (idx < 0) idx = e.key === 'ArrowDown' ? -1 : 0;
      const nextIdx =
        e.key === 'ArrowDown'
          ? Math.min(keys.length - 1, idx + 1)
          : Math.max(0, idx <= 0 ? 0 : idx - 1);
      const skill = rows[nextIdx];
      if (skill) openPrivatePreview(skill);
    };

    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [
    previewExpanded,
    previewTarget,
    tab,
    filtered,
    installedQuery.data,
    activeKey,
    openSharedPreview,
    openPrivatePreview,
  ]);

  /** Enter on focused name already handled in row; Enter while preview open → focus document. */
  useEffect(() => {
    if (!previewExpanded) return;
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
  }, [previewExpanded]);

  return (
    <div className="flex h-full min-h-0 flex-col bg-canvas">
      <div className={cn('shrink-0 border-b border-border pt-4 pb-1', pageRhythm.workbenchX)}>
        <PageHeader
          size="compact"
          title={skillsCopy.page.title}
          description={skillsCopy.page.meta(skills?.length ?? '…', privateOnlyCount)}
          descriptionTip={skillsCopy.page.descriptionTip}
          actions={
            <Button variant="ghost" size="sm" onClick={() => setInstallOpen(true)}>
              <Plus className="h-3.5 w-3.5" /> {skillsCopy.page.installCta}
            </Button>
          }
        />
      </div>

      <div ref={splitRef} className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
        <div
          className={cn(
            'min-w-0 flex-1 overflow-x-auto overflow-y-auto bg-canvas',
            pageRhythm.workbenchX,
            pageRhythm.workbenchY,
          )}
        >
          <Tabs value={tab} onValueChange={(v) => setTab(parseSkillTab(v))} className="mb-2">
        <TabsList>
          <TabsTrigger value="library" className="gap-1.5">
            {skillsCopy.tabs.library}
            {skills != null ? (
              <Tip className={segmentedCountClass} label={skillsCopy.tabs.libraryBadge(skills.length)}>
                {skills.length}
              </Tip>
            ) : null}
          </TabsTrigger>
          <TabsTrigger value="workspace" className="gap-1.5">
            {skillsCopy.tabs.workspace}
            {privateOnlyCount > 0 ? (
              <Tip
                className="rounded-full bg-amber-500/15 px-1.5 py-0 text-2xs tabular-nums text-amber-700 dark:text-amber-300"
                label={skillsCopy.tabs.privateBadge(privateOnlyCount)}
              >
                {privateOnlyCount}
              </Tip>
            ) : null}
          </TabsTrigger>
          <TabsTrigger value="market" className="gap-1.5">
            <Store className="h-3.5 w-3.5" />
            {skillsCopy.tabs.market}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="library" className="mt-3 space-y-3">
          {error !== null ? (
            <ErrorState error={error} onRetry={load} />
          ) : loading ? (
            <TableSkeleton rows={8} cols={6} />
          ) : (
            <>
              <div className="flex flex-wrap items-center gap-3">
                <SearchField
                  className="w-64"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder={skillsCopy.filters.searchPlaceholder}
                />
                <SegmentedControl
                  value={filter}
                  onChange={setFilter}
                  options={FILTERS.map((f) => ({
                    value: f.id,
                    label: f.label,
                    count: filterCounts[f.id],
                  }))}
                  aria-label="启用状态过滤"
                />
                {selected.size > 0 ? (
                  <div className="ml-auto flex flex-wrap items-center gap-2">
                    <span className="text-xs text-muted">
                      {skillsCopy.filters.selectedCount(selected.size)}
                    </span>
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={() => void handleBatchEnable()}
                      disabled={batchSyncing}
                      title={skillsCopy.filters.batchEnableHint}
                    >
                      {batchSyncing
                        ? skillsCopy.filters.batchEnableBusy
                        : skillsCopy.filters.batchEnable}
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => setSelected(new Set())}>
                      {skillsCopy.filters.clearSelection}
                    </Button>
                  </div>
                ) : null}
              </div>

              {filtered.length === 0 ? (
                <EmptyState
                  icon={PackageSearch}
                  title={skillsCopy.empty.noMatchTitle}
                  description={
                    search || filter !== 'all'
                      ? skillsCopy.empty.noMatchFilter
                      : skillsCopy.empty.noMatchLibrary
                  }
                  actionLabel={
                    search || filter !== 'all' ? skillsCopy.empty.clearFilter : undefined
                  }
                  onAction={
                    search || filter !== 'all'
                      ? () => {
                          setSearch('');
                          setFilter('all');
                        }
                      : undefined
                  }
                />
              ) : (
                <SkillMatrix
                  skills={filtered}
                  selected={selected}
                  allSelected={allSelected}
                  pendingCells={pendingCells}
                  onToggleSelect={handleToggleSelect}
                  onToggleSelectAll={handleToggleSelectAll}
                  onCellClick={handleCellClick}
                  onOpenDir={(path) => void handleOpenDir(path)}
                  onPreview={openSharedPreview}
                  activeKey={
                    previewTarget && !previewTarget.privateAgent
                      ? sharedSkillActiveKey(previewTarget.skillId)
                      : activeKey?.startsWith('shared:')
                        ? activeKey
                        : null
                  }
                  agents={matrixAgents}
                  installedAgentIds={installedAgentIds}
                />
              )}
            </>
          )}
        </TabsContent>

        <TabsContent value="workspace" className="mt-3">
          {installedQuery.error ? (
            <ErrorState error={installedQuery.error} onRetry={() => void installedQuery.reload()} />
          ) : installedQuery.loading && !installedQuery.data ? (
            <TableSkeleton rows={6} cols={4} />
          ) : (
            <AgentWorkspace
              installed={installedQuery.data ?? []}
              installedAgents={matrixAgents}
              importingIds={importingIds}
              batchAdopting={batchAdopting}
              onOpenDir={(path) => void handleOpenDir(path)}
              onPreview={openPrivatePreview}
              activeKey={
                previewTarget?.privateAgent
                  ? privateSkillActiveKey(previewTarget.privateAgent, previewTarget.skillId)
                  : activeKey?.startsWith('agent:')
                    ? activeKey
                    : null
              }
              onAdopt={(skillId, agentId, name) => {
                void handleImportPrivate(skillId, agentId, name, false);
              }}
              onUninstall={(skillId, agentId, name, inLibrary) =>
                handleUninstallPrivate(skillId, agentId, name, inLibrary)
              }
              onBatchAdopt={(items) => void handleBatchAdopt(items)}
            />
          )}
        </TabsContent>

        <TabsContent value="market" className="mt-3">
          <div className="mb-3 flex flex-wrap items-center gap-3">
            <SearchField
              className="w-72"
              value={marketQuery}
              onChange={(e) => setMarketQuery(e.target.value)}
              placeholder={skillsCopy.filters.marketSearchPlaceholder}
            />
            <p className="text-xs text-muted">
              <button
                type="button"
                className="text-accent underline-offset-2 hover:underline"
                onClick={() => {
                  const url = marketHomeUrl(activeMarketProvider, skillMarketSource);
                  void openExternalLink(url).catch((e) => {
                    toast({
                      title: '无法打开链接',
                      description: e instanceof Error ? e.message : String(e),
                      variant: 'danger',
                    });
                  });
                }}
              >
                {marketResultLabel(activeMarketProvider, skillMarketSource)}
              </button>
              {' · '}
              {skillsCopy.market.suffix(
                skillMarketSource === 'auto' &&
                  (activeMarketProvider === 'skills.sh' ||
                    activeMarketProvider === 'skillhub.cn'),
              )}
            </p>
          </div>
          {market.loading ? (
            <TableSkeleton rows={4} cols={3} />
          ) : market.error ? (
            <ErrorState error={market.error} onRetry={market.reload} />
          ) : !market.data?.length ? (
            <EmptyState
              icon={Store}
              title={skillsCopy.empty.marketNoneTitle}
              description={skillsCopy.empty.marketNoneDesc}
            />
          ) : (
            <SkillMarketTable
              items={market.data}
              installingId={installingMarketId}
              onInstall={(item) => {
                void (async () => {
                  setInstallingMarketId(item.id);
                  try {
                    const skill = await runInstallMarketSkill(item.id, false);
                    const t = skillsCopy.toast.marketInstallOk(skill.name);
                    toast({
                      title: t.title,
                      description: t.description,
                      variant: 'success',
                      actionLabel: t.actionLabel,
                      onAction: goLibraryAndHighlight,
                      duration: 8000,
                    });
                    void market.reload();
                    await load();
                  } catch (e) {
                    const msg = e instanceof Error ? e.message : String(e);
                    if (msg.toLowerCase().includes('already exists')) {
                      toast({
                        ...skillsCopy.toast.marketExists(msg),
                        variant: 'danger',
                      });
                    } else {
                      toast({
                        ...skillsCopy.toast.installFailed(msg),
                        variant: 'danger',
                      });
                    }
                  } finally {
                    setInstallingMarketId(null);
                  }
                })();
              }}
            />
          )}
        </TabsContent>
          </Tabs>
        </div>

        {previewShellMounted && previewTarget ? (
          <>
            <div
              role="separator"
              aria-orientation="vertical"
              aria-label="调整预览宽度"
              aria-valuenow={previewWidth}
              aria-valuemin={PREVIEW_WIDTH_FLOOR}
              tabIndex={previewExpanded ? 0 : -1}
              onPointerDown={previewExpanded ? onPreviewResizeStart : undefined}
              onDoubleClick={previewExpanded ? resetPreviewWidth : undefined}
              onKeyDown={previewExpanded ? onPreviewSeparatorKeyDown : undefined}
              className={cn(
                'group relative z-10 w-1.5 shrink-0 cursor-col-resize bg-transparent outline-none',
                'hover:bg-accent/40 focus-visible:bg-accent/40 active:bg-accent/60',
                'before:absolute before:inset-y-0 before:-left-1.5 before:-right-1.5 before:content-[""]',
                !previewExpanded && 'pointer-events-none opacity-0',
              )}
            />
            {/*
              外层：宽度动画（卡片宽 + 右侧 gutter）。
              内层：py + pr 形成与画布/右边框的稳定间距；卡片本身 width 固定，右侧永远留白。
            */}
            <div
              className={cn(
                'h-full min-h-0 shrink-0 overflow-hidden',
                previewWidthTransition,
              )}
              style={{ width: previewShellWidth }}
              onTransitionEnd={onPreviewPaneTransitionEnd}
            >
              <div
                className="box-border flex h-full min-h-0"
                style={{
                  width: previewWidth + PREVIEW_FRAME_PAD_RIGHT,
                  paddingTop: PREVIEW_FRAME_PAD_Y,
                  paddingBottom: PREVIEW_FRAME_PAD_Y,
                  paddingRight: PREVIEW_FRAME_PAD_RIGHT,
                }}
              >
                <SkillMarkdownPreviewPanel
                  target={previewTarget}
                  open
                  width={previewWidth}
                  onClose={requestClosePreview}
                  onOpenDir={(path) => void handleOpenDir(path)}
                  contentRef={previewBodyRef}
                  className="h-full min-w-0 shrink-0"
                />
              </div>
            </div>
          </>
        ) : null}
      </div>

      <Dialog
        open={removeFromTool !== null}
        onOpenChange={(open) => !open && !dangerBusy && setRemoveFromTool(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {removeFromTool?.inLibrary
                ? skillsCopy.dialog.removeTitle
                : skillsCopy.dialog.deleteTitle}
            </DialogTitle>
            <DialogDescription>
              {removeFromTool &&
                (removeFromTool.inLibrary
                  ? skillsCopy.dialog.removeBody(
                      AGENT_MAP[removeFromTool.agentId]?.name ?? removeFromTool.agentId,
                      removeFromTool.name,
                    )
                  : skillsCopy.dialog.deleteBody(
                      AGENT_MAP[removeFromTool.agentId]?.name ?? removeFromTool.agentId,
                      removeFromTool.name,
                    ))}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={dangerBusy}
              onClick={() => setRemoveFromTool(null)}
            >
              {skillsCopy.dialog.conflictCancel}
            </Button>
            <Button
              variant="danger"
              disabled={dangerBusy}
              onClick={() => void confirmRemoveFromTool()}
            >
              {dangerBusy
                ? skillsCopy.dialog.busy
                : removeFromTool?.inLibrary
                  ? skillsCopy.dialog.removeConfirm
                  : skillsCopy.dialog.deleteConfirm}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={installOpen} onOpenChange={setInstallOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{skillsCopy.dialog.installTitle}</DialogTitle>
            <DialogDescription>{skillsCopy.dialog.installBody}</DialogDescription>
          </DialogHeader>
          <Input
            value={installSource}
            onChange={(e) => setInstallSource(e.target.value)}
            placeholder={skillsCopy.dialog.installPlaceholder}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setInstallOpen(false)}>
              {skillsCopy.dialog.conflictCancel}
            </Button>
            <Button onClick={() => void handleInstall()}>
              {skillsCopy.dialog.installConfirm}
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
            <DialogTitle>{skillsCopy.dialog.importConflictTitle}</DialogTitle>
            <DialogDescription>
              {importConflict && skillsCopy.dialog.importConflictBody(importConflict.name)}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setImportConflict(null)}>
              {skillsCopy.dialog.conflictCancel}
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
              {skillsCopy.dialog.importConflictConfirm}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
