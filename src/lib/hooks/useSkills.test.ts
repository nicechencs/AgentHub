import { afterEach, describe, expect, it, vi } from 'vitest';
import type { InstalledSkillDto } from '@/lib/backend/contracts/skill-types';
import type { Skill } from '@/lib/types';

const { listSkillsMock, listSkillCatalogMock } = vi.hoisted(() => ({
  listSkillsMock: vi.fn(),
  listSkillCatalogMock: vi.fn(),
}));

vi.mock('@/lib/api/skill', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/api/skill')>();
  return {
    ...actual,
    listSkills: (...args: unknown[]) => listSkillsMock(...args),
    listSkillCatalog: (...args: unknown[]) => listSkillCatalogMock(...args),
  };
});

import {
  clearSkillsDataCache,
  createAsyncSubscriptionCoordinator,
  fetchCatalogShared,
  fetchSkillsShared,
  getSkillsModuleCache,
  invalidateSkills,
  isCurrentSkillsMarketRequest,
  isCurrentSkillsResourceRequest,
} from './useSkills';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

function skill(id: string): Skill {
  return {
    id,
    name: id,
    description: '',
    projections: [],
    projectionByAgent: {},
    conflicts: [],
  };
}

function catalogRow(id: string): InstalledSkillDto {
  return {
    id,
    name: id,
    description: '',
    sourceDir: `/skills/${id}`,
    rootLabel: 'shared',
    rootDir: '/skills',
    origin: 'shared',
    projectable: true,
    source: null,
    projections: [],
  };
}

describe('skills async request guards', () => {
  it('accepts only the newest market query request', () => {
    expect(isCurrentSkillsMarketRequest('new', 'old', 2, 1)).toBe(false);
    expect(isCurrentSkillsMarketRequest('new', 'new', 2, 1)).toBe(false);
    expect(isCurrentSkillsMarketRequest('new', 'new', 2, 2)).toBe(true);
  });

  it('single-flights watcher subscription and unsubscribes expired resolutions', async () => {
    const resolvers: Array<(unsubscribe: () => void) => void> = [];
    let subscribeCalls = 0;
    const unsubscribed: string[] = [];
    const coordinator = createAsyncSubscriptionCoordinator(() => {
      subscribeCalls += 1;
      return new Promise<() => void>((resolve) => {
        resolvers.push((unsubscribe) => resolve(unsubscribe));
      });
    });
    const handler = () => {};

    // StrictMode-style setup -> cleanup -> setup before the first subscribe resolves.
    coordinator.retain(handler);
    await Promise.resolve();
    coordinator.release();
    coordinator.retain(handler);
    await Promise.resolve();
    expect(subscribeCalls).toBe(2);

    resolvers[0]?.(() => unsubscribed.push('stale'));
    await Promise.resolve();
    await Promise.resolve();
    expect(unsubscribed).toEqual(['stale']);

    resolvers[1]?.(() => unsubscribed.push('live'));
    await Promise.resolve();
    await Promise.resolve();
    coordinator.release();
    expect(unsubscribed).toEqual(['stale', 'live']);
  });

  it('accepts only the current skills/catalog generation', () => {
    expect(isCurrentSkillsResourceRequest(1, 2)).toBe(false);
    expect(isCurrentSkillsResourceRequest(2, 2)).toBe(true);
  });
});

describe('skills/catalog inflight generation', () => {
  afterEach(() => {
    clearSkillsDataCache();
    listSkillsMock.mockReset();
    listSkillCatalogMock.mockReset();
  });

  it('does not let a pre-invalidate skills response overwrite the new cache or clear a newer inflight', async () => {
    const stale = deferred<Skill[]>();
    const next = deferred<Skill[]>();
    listSkillsMock.mockReturnValueOnce(stale.promise).mockReturnValueOnce(next.promise);

    const staleLoad = fetchSkillsShared();
    await Promise.resolve();
    invalidateSkills('skills');
    const nextLoad = fetchSkillsShared();
    expect(listSkillsMock).toHaveBeenCalledTimes(2);

    const joined = fetchSkillsShared();
    expect(joined).toBe(nextLoad);
    expect(listSkillsMock).toHaveBeenCalledTimes(2);

    stale.resolve([skill('stale')]);
    await staleLoad;
    expect(getSkillsModuleCache().skills).toBeNull();

    next.resolve([skill('fresh')]);
    await expect(nextLoad).resolves.toEqual([skill('fresh')]);
    await expect(joined).resolves.toEqual([skill('fresh')]);
    expect(getSkillsModuleCache().skills).toEqual([skill('fresh')]);
  });

  it('does not let a pre-invalidate catalog response overwrite the new cache or clear a newer inflight', async () => {
    const stale = deferred<InstalledSkillDto[]>();
    const next = deferred<InstalledSkillDto[]>();
    listSkillCatalogMock.mockReturnValueOnce(stale.promise).mockReturnValueOnce(next.promise);

    const staleLoad = fetchCatalogShared();
    await Promise.resolve();
    invalidateSkills('catalog');
    const nextLoad = fetchCatalogShared();
    expect(listSkillCatalogMock).toHaveBeenCalledTimes(2);

    const joined = fetchCatalogShared();
    expect(joined).toBe(nextLoad);
    expect(listSkillCatalogMock).toHaveBeenCalledTimes(2);

    stale.resolve([catalogRow('stale')]);
    await staleLoad;
    expect(getSkillsModuleCache().catalog).toBeNull();

    next.resolve([catalogRow('fresh')]);
    await expect(nextLoad).resolves.toEqual([catalogRow('fresh')]);
    await expect(joined).resolves.toEqual([catalogRow('fresh')]);
    expect(getSkillsModuleCache().catalog).toEqual([catalogRow('fresh')]);
  });
});
