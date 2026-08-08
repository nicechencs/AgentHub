import { describe, expect, it, beforeEach, vi } from 'vitest';
import { AGENTS } from '@/config/agents';
import type { Backend } from '@/lib/backend/contracts';
import { MOCK_AGENT_CATALOG } from '@/dev/mocks/fixtures/agent-catalog';
import {
  getAgentCatalogSnapshot,
  loadAgentCatalog,
  resetAgentCatalogStore,
  seedAgentCatalog,
} from './agent-catalog-store';

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
});
