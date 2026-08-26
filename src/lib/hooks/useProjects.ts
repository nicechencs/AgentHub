/**
 * Projects 数据 hooks：进程内 SWR。
 * 项目尚未引入 React Query，这里用轻量订阅 + 代数失效 + 按 (agentId, includeHidden) 缓存。
 *
 * 加载策略：
 * - 进程内 list / showHidden / lastAgent 缓存：离页再进可立刻用旧数据，后台 revalidate
 * - inflight 去重：同 key 并发只打一次后端
 * - loading = 无 data 且无 error（有 stale 时不闪 skeleton）
 * - 角标优先从已缓存的分 agent 列表计数，缺才全量扫盘
 */
import { useCallback, useEffect, useState } from 'react';
import { getProjectMetadata, listAgentProjects } from '@/lib/api/project';
import type { AgentId, AgentProject } from '@/lib/types';
import { loadString, saveString, StorageKey } from '@/lib/ui-preferences';

/** Guard cache writes against invalidate generation. */
export function isCurrentProjectsRequest(
  requestGeneration: number,
  currentGeneration: number,
): boolean {
  return requestGeneration === currentGeneration;
}

/** Skeleton only when this visit has no list yet. Stale data must keep the tree. */
export function shouldShowProjectListSkeleton(input: {
  listLoading: boolean;
  data: unknown;
  error: unknown;
  agentsLoading: boolean;
  hiddenReady: boolean;
}): boolean {
  return (
    input.listLoading ||
    (input.data == null && input.error == null && !input.hiddenReady)
  );
}

export function projectListCacheKey(
  agentId: AgentId,
  includeHidden: boolean,
): string {
  return `${agentId}|${includeHidden ? 1 : 0}`;
}

function allListCacheKey(includeHidden: boolean): string {
  return projectListCacheKey('all', includeHidden);
}

export function projectCountsFromCache(
  agentIds: readonly AgentId[],
  includeHidden: boolean,
): { counts: Partial<Record<AgentId, number>>; missing: AgentId[] } {
  const counts: Partial<Record<AgentId, number>> = {};
  const missing: AgentId[] = [];
  for (const id of agentIds) {
    const rows = lists.get(projectListCacheKey(id, includeHidden));
    if (rows) counts[id] = rows.length;
    else missing.push(id);
  }
  return { counts, missing };
}

const dataListeners = new Set<() => void>();
const invalidateListeners = new Set<() => void>();

let fetchGeneration = 0;
let invalidateVersion = 0;
let writeClock = 0;
let showHiddenCache: boolean | null = null;
let lastAgentIdCache: AgentId | null = null;
const lists = new Map<string, AgentProject[]>();
const keyClock = new Map<string, number>();
const listInflight = new Map<string, Promise<AgentProject[]>>();
let metadataInflight: Promise<boolean> | null = null;

function notifyData() {
  dataListeners.forEach((listener) => listener());
}

function notifyInvalidate() {
  invalidateListeners.forEach((listener) => listener());
}

function stampKey(key: string, clock: number) {
  keyClock.set(key, clock);
}

function writeAgentList(agentId: AgentId, includeHidden: boolean, rows: AgentProject[], clock: number) {
  const key = projectListCacheKey(agentId, includeHidden);
  stampKey(key, clock);
  lists.set(key, rows);
  notifyData();
}

export function ingestProjectsByAgent(
  rows: AgentProject[],
  includeHidden: boolean,
  agentIds: readonly AgentId[],
  startedAt: number,
): void {
  const grouped = new Map<string, AgentProject[]>();
  for (const id of agentIds) grouped.set(id, []);
  for (const row of rows) {
    const bucket = grouped.get(row.agentId);
    if (bucket) bucket.push(row);
    else grouped.set(row.agentId, [row]);
  }
  for (const [id, list] of grouped) {
    const key = projectListCacheKey(id, includeHidden);
    if ((keyClock.get(key) ?? 0) > startedAt) continue;
    stampKey(key, startedAt);
    lists.set(key, list);
  }
  notifyData();
}

export function readCachedProjectList(
  agentId: AgentId,
  includeHidden: boolean,
): AgentProject[] | null {
  return lists.get(projectListCacheKey(agentId, includeHidden)) ?? null;
}

export function rememberedProjectAgent(): AgentId | null {
  if (lastAgentIdCache) return lastAgentIdCache;
  const stored = loadString(StorageKey.projectsLastAgent, '');
  lastAgentIdCache = stored || null;
  return lastAgentIdCache;
}

export function rememberProjectAgent(id: AgentId) {
  lastAgentIdCache = id || null;
  if (id) saveString(StorageKey.projectsLastAgent, id);
}

export function readCachedShowHidden(): boolean | null {
  return showHiddenCache;
}

export function writeShowHidden(value: boolean) {
  showHiddenCache = value;
  fetchGeneration += 1;
  listInflight.clear();
  metadataInflight = null;
  notifyData();
}

/** 失效进行中的请求；保留 data 以免切页/刷新闪骨架。 */
export function invalidateProjects() {
  fetchGeneration += 1;
  invalidateVersion += 1;
  listInflight.clear();
  metadataInflight = null;
  notifyInvalidate();
}

/** 测试 / 登出等场景：丢掉进程内 data。 */
export function clearProjectsDataCache() {
  lists.clear();
  keyClock.clear();
  showHiddenCache = null;
  lastAgentIdCache = null;
  saveString(StorageKey.projectsLastAgent, '');
  fetchGeneration += 1;
  invalidateVersion += 1;
  writeClock = 0;
  listInflight.clear();
  metadataInflight = null;
}

export function getProjectsModuleCache(): {
  showHidden: boolean | null;
  lastAgentId: AgentId | null;
  lists: Record<string, AgentProject[]>;
} {
  return {
    showHidden: showHiddenCache,
    lastAgentId: lastAgentIdCache,
    lists: Object.fromEntries(lists),
  };
}

export function fetchProjectMetadataShared(): Promise<boolean> {
  if (metadataInflight) return metadataInflight;
  const requestGeneration = fetchGeneration;
  const request = getProjectMetadata()
    .then((meta) => {
      const value = !!meta.showHiddenProjects;
      if (!isCurrentProjectsRequest(requestGeneration, fetchGeneration)) {
        return showHiddenCache ?? value;
      }
      showHiddenCache = value;
      notifyData();
      return value;
    })
    .finally(() => {
      if (metadataInflight === request) metadataInflight = null;
    });
  metadataInflight = request;
  return request;
}

export function fetchAgentProjectsShared(
  agentId: AgentId | null,
  includeHidden: boolean,
  ingestIds: readonly AgentId[] = [],
): Promise<AgentProject[]> {
  const key = agentId ? projectListCacheKey(agentId, includeHidden) : allListCacheKey(includeHidden);
  const existing = listInflight.get(key);
  if (existing) return existing;

  const requestGeneration = fetchGeneration;
  const startedAt = ++writeClock;
  const request = listAgentProjects(agentId, includeHidden)
    .then((rows) => {
      if (!isCurrentProjectsRequest(requestGeneration, fetchGeneration)) return rows;
      if (agentId) {
        if ((keyClock.get(key) ?? 0) > startedAt) {
          return lists.get(key) ?? rows;
        }
        writeAgentList(agentId, includeHidden, rows, startedAt);
        return rows;
      }
      const ids = ingestIds.length > 0 ? ingestIds : uniqueAgentIds(rows);
      ingestProjectsByAgent(rows, includeHidden, ids, startedAt);
      return rows;
    })
    .finally(() => {
      if (listInflight.get(key) === request) listInflight.delete(key);
    });
  listInflight.set(key, request);
  return request;
}

function uniqueAgentIds(rows: AgentProject[]): AgentId[] {
  const ids: AgentId[] = [];
  const seen = new Set<string>();
  for (const row of rows) {
    if (seen.has(row.agentId)) continue;
    seen.add(row.agentId);
    ids.push(row.agentId);
  }
  return ids;
}

function useInvalidateVersion(): number {
  const [version, setVersion] = useState(invalidateVersion);
  useEffect(() => {
    const listener = () => setVersion(invalidateVersion);
    invalidateListeners.add(listener);
    return () => {
      invalidateListeners.delete(listener);
    };
  }, []);
  return version;
}

function useDataVersion(): number {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const listener = () => setTick((current) => current + 1);
    dataListeners.add(listener);
    return () => {
      dataListeners.delete(listener);
    };
  }, []);
  return tick;
}

export function useProjectShowHidden() {
  const [showHidden, setShowHiddenState] = useState(() => showHiddenCache ?? false);
  const [ready, setReady] = useState(() => showHiddenCache !== null);

  useEffect(() => {
    const listener = () => {
      if (showHiddenCache !== null) {
        setShowHiddenState(showHiddenCache);
        setReady(true);
      }
    };
    dataListeners.add(listener);
    void fetchProjectMetadataShared()
      .then((value) => {
        setShowHiddenState(value);
        setReady(true);
      })
      .catch(() => {
        setReady(true);
      });
    return () => {
      dataListeners.delete(listener);
    };
  }, []);

  const setShowHidden = useCallback((value: boolean) => {
    writeShowHidden(value);
    setShowHiddenState(value);
    setReady(true);
  }, []);

  return { showHidden, ready, setShowHidden };
}

export function useAgentProjectList(
  agentId: AgentId | null,
  includeHidden: boolean,
  enabled: boolean,
) {
  const invalidate = useInvalidateVersion();
  const cacheKey = agentId ? projectListCacheKey(agentId, includeHidden) : '';
  const [data, setDataState] = useState<AgentProject[] | null>(() =>
    agentId ? readCachedProjectList(agentId, includeHidden) : null,
  );
  const [error, setError] = useState<unknown>(null);
  const [fetching, setFetching] = useState(false);
  const [seenKey, setSeenKey] = useState(cacheKey);

  if (seenKey !== cacheKey) {
    setSeenKey(cacheKey);
    setDataState(agentId ? readCachedProjectList(agentId, includeHidden) : null);
    setError(null);
  }

  const replaceProjectListFromMutation = useCallback(
    (next: AgentProject[] | ((prev: AgentProject[]) => AgentProject[])) => {
      setDataState((prev) => {
        const rows = typeof next === 'function' ? next(prev ?? []) : next;
        if (agentId) {
          writeClock += 1;
          writeAgentList(agentId, includeHidden, rows, writeClock);
        }
        return rows;
      });
    },
    [agentId, includeHidden],
  );

  const reload = useCallback(async () => {
    if (!enabled || !agentId) return;
    const requestGeneration = fetchGeneration;
    setFetching(true);
    try {
      const rows = await fetchAgentProjectsShared(agentId, includeHidden);
      if (!isCurrentProjectsRequest(requestGeneration, fetchGeneration)) return;
      setDataState(rows);
      setError(null);
    } catch (err) {
      if (!isCurrentProjectsRequest(requestGeneration, fetchGeneration)) return;
      if (readCachedProjectList(agentId, includeHidden) == null) {
        setError(err);
      } else {
        console.error('[useProjects] refresh failed, keeping stale rows', err);
      }
    } finally {
      if (isCurrentProjectsRequest(requestGeneration, fetchGeneration)) {
        setFetching(false);
      }
    }
  }, [enabled, agentId, includeHidden]);

  useEffect(() => {
    if (!enabled || !agentId) return;
    const cached = readCachedProjectList(agentId, includeHidden);
    if (cached) setDataState(cached);
    void reload();
  }, [enabled, agentId, includeHidden, invalidate, reload]);

  return {
    data,
    error,
    loading: enabled && data == null && error == null,
    refreshing: enabled && fetching && data != null,
    reload,
    replaceProjectListFromMutation,
  };
}

export function useAgentProjectCounts(
  agentIds: readonly AgentId[],
  includeHidden: boolean,
  enabled: boolean,
) {
  useDataVersion();
  const agentKey = agentIds.join(',');
  const snapshot = projectCountsFromCache(agentIds, includeHidden);

  const reload = useCallback(async () => {
    if (!enabled || !agentKey) return;
    const ids = agentKey.split(',') as AgentId[];
    await fetchAgentProjectsShared(null, includeHidden, ids);
  }, [enabled, agentKey, includeHidden]);

  useEffect(() => {
    if (!enabled || !agentKey) return;
    const ids = agentKey.split(',') as AgentId[];
    if (projectCountsFromCache(ids, includeHidden).missing.length === 0) return;
    void reload().catch(() => {});
  }, [enabled, agentKey, includeHidden, reload]);

  return { counts: snapshot.counts, reload };
}
