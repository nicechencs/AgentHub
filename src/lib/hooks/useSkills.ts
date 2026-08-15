/**
 * Skills 数据 hooks：list / catalog / market + 写操作后分 key invalidate。
 * 项目尚未引入 React Query，这里用轻量订阅 + 分资源版本号失效 + 进程内 SWR 缓存。
 * Tauri 下会订阅 `skills-fs-changed`（目录变更防抖事件）自动 bump。
 *
 * 加载策略：
 * - `enabled` 控制是否请求（Tab 懒加载）
 * - 进程内 data cache：离页再进可立刻用旧数据，后台 revalidate
 * - inflight 去重：同 key 并发只打一次后端
 * - loading = 无 data 且无 error（有 stale 时不闪 skeleton）
 */
import { useCallback, useEffect, useState } from 'react';
import {
  importPrivateSkillToShared,
  installMarketSkill,
  installSkillFromSource,
  listSkillCatalog,
  listSkills,
  onSkillsFsChanged,
  projectSkill,
  searchSkillMarket,
  syncAll,
  toggleSkillSync,
  uninstallSkill,
  updateSkill,
  type CoreSkill,
  type InstalledSkillDto,
  type SkillListingDto,
  type SkillProjectResultDto,
} from '@/lib/api/skill';
import type { AgentId, Skill } from '@/lib/types';

export type SkillsCacheKey = 'skills' | 'catalog' | 'market';

const ALL_KEYS: SkillsCacheKey[] = ['skills', 'catalog', 'market'];

const listeners = new Set<() => void>();
const versions: Record<SkillsCacheKey, number> = {
  skills: 0,
  catalog: 0,
  market: 0,
};
let fsWatchStarted = false;
let fsWatchSubscribers = 0;
let fsWatchUnsub: (() => void) | null = null;

/** 进程内最后一次成功结果（跨路由 unmount 仍保留） */
let skillsData: Skill[] | null = null;
let catalogData: InstalledSkillDto[] | null = null;
let marketData: { query: string; rows: SkillListingDto[] } | null = null;

/**
 * 市场源/配置变更代数。切换 skills.sh ↔ skillhub.cn 后递增，
 * 防止旧 inflight 写回错误源结果。
 */
let marketGeneration = 0;

/** 同 key 请求合并 */
let skillsInflight: Promise<Skill[]> | null = null;
let catalogInflight: Promise<InstalledSkillDto[]> | null = null;
const marketInflight = new Map<string, Promise<SkillListingDto[]>>();

function bump(keys: SkillsCacheKey[]) {
  for (const k of keys) {
    versions[k] += 1;
  }
  listeners.forEach((l) => l());
}

function dropMarketCache() {
  marketGeneration += 1;
  marketData = null;
  marketInflight.clear();
}

/** 失效技能相关缓存版本；默认全量。 */
export function invalidateSkills(keys?: SkillsCacheKey | SkillsCacheKey[]) {
  const list = keys == null ? ALL_KEYS : Array.isArray(keys) ? keys : [keys];
  // 市场结果与「市场源」绑定：失效时必须丢掉 data / inflight，
  // 否则会继续展示上一源（skills.sh / skillhub.cn）的榜单。
  if (list.includes('market')) {
    dropMarketCache();
  }
  bump(list);
}

/** 测试 / 登出等场景：丢掉进程内 data。 */
export function clearSkillsDataCache() {
  skillsData = null;
  catalogData = null;
  dropMarketCache();
  skillsInflight = null;
  catalogInflight = null;
}

/** 当前某 key 的缓存版本；技能页本地 state 可随此值重载。 */
export function useSkillsCacheVersion(key: SkillsCacheKey = 'skills'): number {
  return useCacheVersion(key);
}

/** Subscribe to skill-directory changes while any hook consumer is mounted. */
function retainSkillsFsWatch() {
  if (typeof window === 'undefined') return;
  fsWatchSubscribers += 1;
  if (fsWatchStarted) return;
  fsWatchStarted = true;
  void onSkillsFsChanged(() => {
    // 外部目录变更：共享库矩阵 + agent 目录都可能变
    invalidateSkills(['skills', 'catalog']);
  }).then((unsub) => {
    fsWatchUnsub = unsub;
    if (fsWatchSubscribers === 0) {
      releaseSkillsFsWatch(true);
    }
  }).catch(() => {
    fsWatchStarted = false;
  });
}

function releaseSkillsFsWatch(force = false) {
  if (!force) {
    fsWatchSubscribers = Math.max(0, fsWatchSubscribers - 1);
    if (fsWatchSubscribers > 0) return;
  } else {
    fsWatchSubscribers = 0;
  }
  fsWatchUnsub?.();
  fsWatchUnsub = null;
  fsWatchStarted = false;
}

function useCacheVersion(key: SkillsCacheKey): number {
  const [, setTick] = useState(0);
  useEffect(() => {
    retainSkillsFsWatch();
    const l = () => setTick((t) => t + 1);
    listeners.add(l);
    return () => {
      listeners.delete(l);
      releaseSkillsFsWatch();
    };
  }, []);
  return versions[key];
}

export type SkillsQueryOptions = {
  /** false 时不发起请求（Tab 懒加载） */
  enabled?: boolean;
};

type SetStateAction<T> = T | ((prev: T) => T);

async function fetchSkillsShared(): Promise<Skill[]> {
  if (!skillsInflight) {
    skillsInflight = listSkills()
      .then((rows) => {
        skillsData = rows;
        return rows;
      })
      .finally(() => {
        skillsInflight = null;
      });
  }
  return skillsInflight;
}

async function fetchCatalogShared(): Promise<InstalledSkillDto[]> {
  if (!catalogInflight) {
    catalogInflight = listSkillCatalog()
      .then((rows) => {
        catalogData = rows;
        return rows;
      })
      .finally(() => {
        catalogInflight = null;
      });
  }
  return catalogInflight;
}

async function fetchMarketShared(query: string): Promise<SkillListingDto[]> {
  const gen = marketGeneration;
  const key = `${gen}::${query}`;
  let p = marketInflight.get(key);
  if (!p) {
    p = searchSkillMarket(query)
      .then((rows) => {
        // 仅在仍是当前代数时写入；设置切换后旧请求不得污染缓存
        if (gen === marketGeneration) {
          marketData = { query, rows };
        }
        return rows;
      })
      .finally(() => {
        marketInflight.delete(key);
      });
    marketInflight.set(key, p);
  }
  return p;
}

export function useSkillsList(opts: SkillsQueryOptions = {}) {
  const { enabled = true } = opts;
  const version = useCacheVersion('skills');
  const [data, setDataState] = useState<Skill[] | null>(() => skillsData);
  const [error, setError] = useState<unknown>(null);
  const [fetching, setFetching] = useState(false);

  const setData = useCallback((action: SetStateAction<Skill[] | null>) => {
    setDataState((prev) => {
      const next = typeof action === 'function' ? action(prev) : action;
      skillsData = next;
      return next;
    });
  }, []);

  const reload = useCallback(async () => {
    if (!enabled) return;
    setFetching(true);
    try {
      const rows = await fetchSkillsShared();
      setDataState(rows);
      setError(null);
    } catch (e) {
      // 有 stale 时保留旧表，只记 error（ErrorState 由调用方决定是否盖住）
      if (skillsData == null) {
        setError(e);
      }
    } finally {
      setFetching(false);
    }
  }, [enabled]);

  useEffect(() => {
    if (!enabled) return;
    // 其他实例可能已写入 module cache
    if (skillsData != null) setDataState(skillsData);
    void reload();
  }, [enabled, version, reload]);

  return {
    data,
    error,
    /** 首屏 / 懒加载：尚无成功数据时显示 skeleton */
    loading: enabled && data == null && error == null,
    refreshing: enabled && fetching && data != null,
    reload,
    /** 乐观更新本地矩阵，并写回进程缓存 */
    setData,
  };
}

export function useSkillCatalog(opts: SkillsQueryOptions = {}) {
  const { enabled = true } = opts;
  const version = useCacheVersion('catalog');
  const [data, setDataState] = useState<InstalledSkillDto[] | null>(() => catalogData);
  const [error, setError] = useState<unknown>(null);
  const [fetching, setFetching] = useState(false);

  const setData = useCallback((action: SetStateAction<InstalledSkillDto[] | null>) => {
    setDataState((prev) => {
      const next = typeof action === 'function' ? action(prev) : action;
      catalogData = next;
      return next;
    });
  }, []);

  const reload = useCallback(async () => {
    if (!enabled) return;
    setFetching(true);
    try {
      const rows = await fetchCatalogShared();
      setDataState(rows);
      setError(null);
    } catch (e) {
      if (catalogData == null) setError(e);
    } finally {
      setFetching(false);
    }
  }, [enabled]);

  useEffect(() => {
    if (!enabled) return;
    if (catalogData != null) setDataState(catalogData);
    void reload();
  }, [enabled, version, reload]);

  return {
    data,
    error,
    loading: enabled && data == null && error == null,
    refreshing: enabled && fetching && data != null,
    reload,
    /** 乐观更新 catalog 行，并写回进程缓存 */
    setData,
  };
}

export function useSkillMarket(query: string, opts: SkillsQueryOptions = {}) {
  const { enabled = true } = opts;
  const version = useCacheVersion('market');
  const [data, setDataState] = useState<SkillListingDto[] | null>(() =>
    marketData && marketData.query === query ? marketData.rows : null,
  );
  const [error, setError] = useState<unknown>(null);
  const [fetching, setFetching] = useState(false);

  const reload = useCallback(async () => {
    if (!enabled) return;
    const gen = marketGeneration;
    setFetching(true);
    try {
      const rows = await fetchMarketShared(query);
      // 源已切换则丢弃本次结果（由新 effect / reload 接管）
      if (gen !== marketGeneration) return;
      setDataState(rows);
      setError(null);
    } catch (e) {
      if (gen !== marketGeneration) return;
      if (!(marketData && marketData.query === query)) setError(e);
    } finally {
      if (gen === marketGeneration) setFetching(false);
    }
  }, [enabled, query]);

  useEffect(() => {
    if (!enabled) return;
    if (marketData && marketData.query === query) {
      setDataState(marketData.rows);
    } else {
      // query 变了或源已失效：先清空，避免继续展示上一市场源
      setDataState(null);
      setError(null);
    }
    void reload();
  }, [enabled, version, reload, query]);

  return {
    data,
    error,
    loading: enabled && data == null && error == null,
    refreshing: enabled && fetching && data != null,
    reload,
  };
}

export async function runToggleSkill(
  skillId: string,
  agentId: AgentId,
  opts?: { force?: boolean },
) {
  const result = await toggleSkillSync(skillId, agentId, opts);
  invalidateSkills(['skills', 'catalog']);
  return result;
}

export async function runSyncAll() {
  const result = await syncAll();
  invalidateSkills(['skills', 'catalog']);
  return result;
}

export async function runInstallSkill(source: string, overwrite = false) {
  const skill = await installSkillFromSource(source, overwrite);
  invalidateSkills(['skills', 'catalog', 'market']);
  return skill;
}

export async function runImportPrivateSkill(
  skillId: string,
  agentId: AgentId,
  overwrite = false,
) {
  const skill = await importPrivateSkillToShared(skillId, agentId, overwrite);
  invalidateSkills(['skills', 'catalog', 'market']);
  return skill;
}

export async function runUninstallSkill(skillId: string, privateAgent?: AgentId) {
  await uninstallSkill(skillId, privateAgent);
  invalidateSkills(['skills', 'catalog', 'market']);
}

export async function runUpdateSkill(skillId: string) {
  const skill = await updateSkill(skillId);
  invalidateSkills(['skills', 'catalog', 'market']);
  return skill;
}

export async function runProjectSkill(
  skillId: string,
  agentId: AgentId,
  mode: 'link' | 'copy' = 'link',
): Promise<SkillProjectResultDto> {
  const result = await projectSkill(skillId, agentId, mode);
  invalidateSkills(['skills', 'catalog']);
  return result;
}

export async function runInstallMarketSkill(
  skillId: string,
  overwrite = false,
): Promise<CoreSkill> {
  const skill = await installMarketSkill(skillId, overwrite);
  invalidateSkills(['skills', 'catalog', 'market']);
  return skill;
}
