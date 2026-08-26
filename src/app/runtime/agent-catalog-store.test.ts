import { describe, expect, it, beforeEach, vi } from 'vitest';
import { AGENTS } from '@/config/agents';
import type { Backend } from '@/lib/backend/contracts';
import { MOCK_AGENT_CATALOG } from '@/dev/mocks/fixtures/agent-catalog';
import { resetBackend } from './backend-runtime';
import {
  getAgentCatalogSnapshot,
  loadAgentCatalog,
  resetAgentCatalogStore,
  seedAgentCatalog,
} from './agent-catalog-store';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

describe('agent-catalog-store', () => {
  beforeEach(() => {
    resetAgentCatalogStore();
  });

  it('seed marks ready and hydrates AGENTS', () => {
    seedAgentCatalog(MOCK_AGENT_CATALOG);
    const snap = getAgentCatalogSnapshot();
    expect(snap.status).toBe('ready');
    expect(snap.hydrated).toBe(true);
    expect(snap.entries).toHaveLength(MOCK_AGENT_CATALOG.length);
    expect(AGENTS).toHaveLength(MOCK_AGENT_CATALOG.length);
  });

  it('loadAgentCatalog success', async () => {
    const backend = {
      catalog: {
        listAgentCatalog: vi.fn(async () => MOCK_AGENT_CATALOG),
        getAgentCatalogEntry: vi.fn(),
      },
    } as unknown as Backend;

    const snap = await loadAgentCatalog(backend);
    expect(snap.status).toBe('ready');
    expect(backend.catalog.listAgentCatalog).toHaveBeenCalledOnce();
    expect(AGENTS.map((a) => a.id)).toContain('claude');
  });

  it('load failure clears product set and does not keep static agents', async () => {
    seedAgentCatalog(MOCK_AGENT_CATALOG);
    expect(AGENTS.length).toBeGreaterThan(0);

    const backend = {
      catalog: {
        listAgentCatalog: vi.fn(async () => {
          throw new Error('backend down');
        }),
        getAgentCatalogEntry: vi.fn(),
      },
    } as unknown as Backend;

    await expect(loadAgentCatalog(backend)).rejects.toThrow('backend down');
    const snap = getAgentCatalogSnapshot();
    expect(snap.status).toBe('error');
    expect(snap.hydrated).toBe(false);
    expect(AGENTS).toHaveLength(0);
  });

  it('does not let a pre-reset response overwrite the new catalog', async () => {
    const staleEntries = deferred<typeof MOCK_AGENT_CATALOG>();
    const nextEntries = deferred<typeof MOCK_AGENT_CATALOG>();
    const firstBackend = {
      catalog: {
        listAgentCatalog: vi.fn(() => staleEntries.promise),
        getAgentCatalogEntry: vi.fn(),
      },
    } as unknown as Backend;
    const secondBackend = {
      catalog: {
        listAgentCatalog: vi.fn(() => nextEntries.promise),
        getAgentCatalogEntry: vi.fn(),
      },
    } as unknown as Backend;

    const stale = loadAgentCatalog(firstBackend);
    await Promise.resolve();
    resetAgentCatalogStore();
    const next = loadAgentCatalog(secondBackend);
    expect(getAgentCatalogSnapshot().status).toBe('loading');
    expect(AGENTS).toHaveLength(0);

    staleEntries.resolve(MOCK_AGENT_CATALOG);
    await stale;

    expect(getAgentCatalogSnapshot().status).toBe('loading');
    expect(AGENTS).toHaveLength(0);

    const onlyCodex = MOCK_AGENT_CATALOG.filter((entry) => entry.key === 'codex');
    nextEntries.resolve(onlyCodex);
    const loaded = await next;

    expect(loaded.status).toBe('ready');
    expect(loaded.entries.map((entry) => entry.key)).toEqual(['codex']);
    expect(AGENTS.map((agent) => agent.id)).toEqual(['codex']);
  });

  it('does not let a stale failure clear a catalog seeded after reset', async () => {
    const staleEntries = deferred<typeof MOCK_AGENT_CATALOG>();
    const backend = {
      catalog: {
        listAgentCatalog: vi.fn(() => staleEntries.promise),
        getAgentCatalogEntry: vi.fn(),
      },
    } as unknown as Backend;

    const stale = loadAgentCatalog(backend);
    await Promise.resolve();
    resetAgentCatalogStore();
    seedAgentCatalog(MOCK_AGENT_CATALOG.filter((entry) => entry.key === 'kimi'));
    expect(AGENTS.map((agent) => agent.id)).toEqual(['kimi']);

    staleEntries.reject(new Error('backend down'));
    await stale;
    expect(AGENTS.map((agent) => agent.id)).toEqual(['kimi']);
    expect(getAgentCatalogSnapshot()).toMatchObject({
      status: 'ready',
      hydrated: true,
    });
  });

  it('resetBackend clears the catalog with other runtime stores', () => {
    seedAgentCatalog(MOCK_AGENT_CATALOG);
    expect(AGENTS.length).toBeGreaterThan(0);
    resetBackend();
    expect(AGENTS).toHaveLength(0);
    expect(getAgentCatalogSnapshot()).toMatchObject({
      status: 'idle',
      hydrated: false,
      entries: [],
    });
  });
});
