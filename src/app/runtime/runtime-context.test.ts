import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { AGENTS } from '@/config/agents';
import type { Backend } from '@/lib/backend/contracts';
import { MOCK_AGENT_CATALOG } from '@/dev/mocks/fixtures/agent-catalog';
import { getAgentCatalogSnapshot, seedAgentCatalog } from './agent-catalog-store';
import { getAgentStatusSnapshot, loadAgentStatuses } from './agent-status-store';
import { getAppUpdateAvailable, setAppUpdateAvailable } from './app-update-store';
import { resetBackend, setBackend } from './backend-runtime';
import { getConnectionPoolSnapshot, loadConnectionPool } from './connection-pool-store';
import { RUNTIME_STORE_RESETS } from './runtime-context';
import { getTicketWalletSnapshot, loadTicketWallet } from './ticket-wallet-store';

const runtimeDir = path.dirname(fileURLToPath(import.meta.url));

function exportedStoreResets(): string[] {
  const names: string[] = [];
  for (const file of readdirSync(runtimeDir)) {
    if (!file.endsWith('-store.ts')) continue;
    const src = readFileSync(path.join(runtimeDir, file), 'utf8');
    const re = /export function (reset\w+Store)\s*\(/g;
    let match: RegExpExecArray | null;
    while ((match = re.exec(src))) names.push(match[1]);
  }
  return names.sort();
}

function stubBackend(): Backend {
  return {
    agent: { listAgents: async () => [{ agentId: 'claude', installed: true }] },
    account: { listAccounts: async () => [] },
    provider: { listProviders: async () => [] },
    ticket: {
      listWallet: async () => ({ tickets: [], bindings: [], surfaceGroups: [] }),
    },
  } as unknown as Backend;
}

async function fillRuntimeStores(): Promise<void> {
  seedAgentCatalog(MOCK_AGENT_CATALOG);
  setAppUpdateAvailable({
    version: '9.9.9',
    currentVersion: '0.1.0',
    notes: 'pending',
    date: null,
  });
  const backend = stubBackend();
  await Promise.all([
    loadAgentStatuses(backend),
    loadConnectionPool(backend),
    loadTicketWallet(backend),
  ]);
}

function expectStoresIdle(): void {
  expect(getAgentCatalogSnapshot()).toMatchObject({
    status: 'idle',
    hydrated: false,
    entries: [],
  });
  expect(AGENTS).toHaveLength(0);
  expect(getAgentStatusSnapshot().state).toBe('idle');
  expect(getConnectionPoolSnapshot().state).toBe('idle');
  expect(getTicketWalletSnapshot().state).toBe('idle');
  expect(getAppUpdateAvailable()).toBeNull();
}

describe('runtime context reset registry', () => {
  it('registers every exported store reset, including app-update', () => {
    const registered = RUNTIME_STORE_RESETS.map((reset) => reset.name).sort();
    expect(registered).toEqual(exportedStoreResets());
    expect(registered).toContain('resetAppUpdateStore');
  });

  it('setBackend / resetBackend reset stores only through the registry', () => {
    const src = readFileSync(path.join(runtimeDir, 'backend-runtime.ts'), 'utf8');
    expect(src).toMatch(/resetRuntimeContext\(\)/);
    expect(src).not.toMatch(
      /reset(?:AgentCatalog|AgentStatus|ConnectionPool|TicketWallet|AppUpdate)Store/,
    );
  });

  it('setBackend clears the catalog with other runtime stores', async () => {
    await fillRuntimeStores();
    expect(getAgentCatalogSnapshot().status).toBe('ready');
    expect(AGENTS.length).toBeGreaterThan(0);
    expect(getAgentStatusSnapshot().state).toBe('ready');
    expect(getConnectionPoolSnapshot().state).toBe('ready');
    expect(getTicketWalletSnapshot().state).toBe('ready');
    expect(getAppUpdateAvailable()?.version).toBe('9.9.9');

    setBackend(stubBackend());
    expectStoresIdle();
  });

  it('resetBackend clears the catalog with other runtime stores', async () => {
    await fillRuntimeStores();
    expect(getAppUpdateAvailable()?.version).toBe('9.9.9');

    resetBackend();
    expectStoresIdle();
  });
});
