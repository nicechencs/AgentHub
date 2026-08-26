import { afterEach, describe, expect, it, vi } from 'vitest';
import type { AgentProject, ProjectMetadataFile } from '@/lib/types';

const { listAgentProjectsMock, getProjectMetadataMock } = vi.hoisted(() => ({
  listAgentProjectsMock: vi.fn(),
  getProjectMetadataMock: vi.fn(),
}));

vi.mock('@/lib/api/project', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api/project')>();
  return {
    ...actual,
    listAgentProjects: (...args: unknown[]) => listAgentProjectsMock(...args),
    getProjectMetadata: (...args: unknown[]) => getProjectMetadataMock(...args),
  };
});

import {
  clearProjectsDataCache,
  fetchAgentProjectsShared,
  fetchProjectMetadataShared,
  getProjectsModuleCache,
  ingestProjectsByAgent,
  invalidateProjects,
  isCurrentProjectsRequest,
  projectCountsFromCache,
  projectListCacheKey,
  readCachedShowHidden,
  rememberProjectAgent,
  rememberedProjectAgent,
  shouldShowProjectListSkeleton,
  writeShowHidden,
} from './useProjects';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

function project(id: string, agentId: string): AgentProject {
  return {
    id,
    agentId,
    title: id,
    storagePath: `/p/${id}`,
    relativePath: id,
    sessionCount: 1,
    sizeBytes: 1,
    updatedAt: '2026-01-01T00:00:00.000Z',
  };
}

function meta(showHiddenProjects: boolean): ProjectMetadataFile {
  return { version: 1, showHiddenProjects, projects: {} };
}

describe('projects request guards', () => {
  it('accepts only the current generation', () => {
    expect(isCurrentProjectsRequest(1, 2)).toBe(false);
    expect(isCurrentProjectsRequest(2, 2)).toBe(true);
  });

  it('keys lists by agent and hidden flag', () => {
    expect(projectListCacheKey('claude', false)).toBe('claude|0');
    expect(projectListCacheKey('claude', true)).toBe('claude|1');
  });
});

describe('project list skeleton', () => {
  it('does not flash when stale data is already in hand', () => {
    expect(
      shouldShowProjectListSkeleton({
        listLoading: false,
        data: [project('c1', 'claude')],
        error: null,
        agentsLoading: true,
        hiddenReady: false,
      }),
    ).toBe(false);
  });

  it('shows skeleton only when this visit has no list yet', () => {
    expect(
      shouldShowProjectListSkeleton({
        listLoading: true,
        data: null,
        error: null,
        agentsLoading: false,
        hiddenReady: true,
      }),
    ).toBe(true);
    expect(
      shouldShowProjectListSkeleton({
        listLoading: false,
        data: null,
        error: null,
        agentsLoading: true,
        hiddenReady: true,
      }),
    ).toBe(false);
    expect(
      shouldShowProjectListSkeleton({
        listLoading: false,
        data: null,
        error: null,
        agentsLoading: false,
        hiddenReady: false,
      }),
    ).toBe(true);
    expect(
      shouldShowProjectListSkeleton({
        listLoading: false,
        data: null,
        error: new Error('nope'),
        agentsLoading: false,
        hiddenReady: true,
      }),
    ).toBe(false);
  });
});

describe('remembered project agent', () => {
  afterEach(() => {
    clearProjectsDataCache();
  });

  it('stores the last tab and forgets it on cache clear', () => {
    expect(rememberedProjectAgent()).toBeNull();
    rememberProjectAgent('kimi');
    expect(rememberedProjectAgent()).toBe('kimi');
    clearProjectsDataCache();
    expect(rememberedProjectAgent()).toBeNull();
  });
});

describe('projects module cache', () => {
  afterEach(() => {
    clearProjectsDataCache();
    listAgentProjectsMock.mockReset();
    getProjectMetadataMock.mockReset();
  });

  it('single-flights the same agent list request', async () => {
    const pending = deferred<AgentProject[]>();
    listAgentProjectsMock.mockReturnValueOnce(pending.promise);

    const first = fetchAgentProjectsShared('claude', false);
    const second = fetchAgentProjectsShared('claude', false);
    expect(second).toBe(first);
    expect(listAgentProjectsMock).toHaveBeenCalledTimes(1);

    pending.resolve([project('c1', 'claude')]);
    await expect(first).resolves.toEqual([project('c1', 'claude')]);
    expect(getProjectsModuleCache().lists['claude|0']).toEqual([project('c1', 'claude')]);
  });

  it('does not let a pre-invalidate list response overwrite the new cache', async () => {
    const stale = deferred<AgentProject[]>();
    const next = deferred<AgentProject[]>();
    listAgentProjectsMock.mockReturnValueOnce(stale.promise).mockReturnValueOnce(next.promise);

    const staleLoad = fetchAgentProjectsShared('claude', false);
    await Promise.resolve();
    invalidateProjects();
    const nextLoad = fetchAgentProjectsShared('claude', false);
    expect(listAgentProjectsMock).toHaveBeenCalledTimes(2);

    const joined = fetchAgentProjectsShared('claude', false);
    expect(joined).toBe(nextLoad);

    stale.resolve([project('stale', 'claude')]);
    await staleLoad;
    expect(getProjectsModuleCache().lists['claude|0']).toBeUndefined();

    next.resolve([project('fresh', 'claude')]);
    await expect(nextLoad).resolves.toEqual([project('fresh', 'claude')]);
    expect(getProjectsModuleCache().lists['claude|0']).toEqual([project('fresh', 'claude')]);
  });

  it('splits an all-agent scan into per-agent lists including empty tabs', async () => {
    listAgentProjectsMock.mockResolvedValueOnce([project('c1', 'claude')]);
    await fetchAgentProjectsShared(null, false, ['claude', 'codex']);

    expect(getProjectsModuleCache().lists['claude|0']).toEqual([project('c1', 'claude')]);
    expect(getProjectsModuleCache().lists['codex|0']).toEqual([]);
    expect(projectCountsFromCache(['claude', 'codex'], false)).toEqual({
      counts: { claude: 1, codex: 0 },
      missing: [],
    });
  });

  it('does not let an older all-agent scan overwrite a newer per-agent write', async () => {
    const all = deferred<AgentProject[]>();
    const one = deferred<AgentProject[]>();
    listAgentProjectsMock.mockReturnValueOnce(all.promise).mockReturnValueOnce(one.promise);

    const allLoad = fetchAgentProjectsShared(null, false, ['claude', 'codex']);
    const oneLoad = fetchAgentProjectsShared('claude', false);

    one.resolve([project('fresh', 'claude')]);
    await oneLoad;
    all.resolve([project('stale', 'claude')]);
    await allLoad;

    expect(getProjectsModuleCache().lists['claude|0']).toEqual([project('fresh', 'claude')]);
    expect(getProjectsModuleCache().lists['codex|0']).toEqual([]);
  });

  it('skips ingest when the per-agent list was written after the scan started', () => {
    ingestProjectsByAgent([project('fresh', 'claude')], false, ['claude'], 2);
    ingestProjectsByAgent([project('stale', 'claude')], false, ['claude'], 1);
    expect(getProjectsModuleCache().lists['claude|0']).toEqual([project('fresh', 'claude')]);
  });

  it('reports missing counts until that agent list is cached', () => {
    expect(projectCountsFromCache(['claude', 'codex'], false)).toEqual({
      counts: {},
      missing: ['claude', 'codex'],
    });
    ingestProjectsByAgent([project('c1', 'claude')], false, ['claude'], 1);
    expect(projectCountsFromCache(['claude', 'codex'], false)).toEqual({
      counts: { claude: 1 },
      missing: ['codex'],
    });
  });

  it('caches showHidden from metadata and ignores a stale in-flight result after toggle', async () => {
    const stale = deferred<ProjectMetadataFile>();
    getProjectMetadataMock.mockReturnValueOnce(stale.promise);

    const first = fetchProjectMetadataShared();
    writeShowHidden(true);
    stale.resolve(meta(false));
    await expect(first).resolves.toBe(true);
    expect(readCachedShowHidden()).toBe(true);
  });

  it('remembers the last project agent across cache reads', () => {
    rememberProjectAgent('kimi');
    expect(rememberedProjectAgent()).toBe('kimi');
    clearProjectsDataCache();
    expect(rememberedProjectAgent()).toBeNull();
    expect(readCachedShowHidden()).toBeNull();
  });
});
