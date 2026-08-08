import type { AgentCatalogPort } from '@/lib/backend/contracts';
import type { AgentCatalogEntryDto } from '@/lib/backend/contracts/agent-catalog-types';
import { delay, randomLatency } from '@/dev/mocks/delay';
import { MOCK_AGENT_CATALOG } from './fixtures/agent-catalog';

/** Mutable mock catalog rows (tests may push unknown-demo). */
let mockCatalog: AgentCatalogEntryDto[] = MOCK_AGENT_CATALOG.map((e) => ({
  ...e,
  installChannels: e.installChannels.map((c) => ({ ...c, requires: [...c.requires] })),
  capabilities: { ...e.capabilities },
}));

export function getMockAgentCatalog(): AgentCatalogEntryDto[] {
  return mockCatalog;
}

export function setMockAgentCatalog(entries: AgentCatalogEntryDto[]): void {
  mockCatalog = entries.map((e) => ({
    ...e,
    installChannels: e.installChannels.map((c) => ({ ...c, requires: [...c.requires] })),
    capabilities: { ...e.capabilities },
  }));
}

export function resetMockAgentCatalog(): void {
  setMockAgentCatalog(MOCK_AGENT_CATALOG);
}

export function createMockCatalogPort(): AgentCatalogPort {
  return {
    async listAgentCatalog() {
      await delay(randomLatency());
      return mockCatalog.map((e) => ({
        ...e,
        installChannels: e.installChannels.map((c) => ({ ...c, requires: [...c.requires] })),
        capabilities: { ...e.capabilities },
      }));
    },
    async getAgentCatalogEntry(key: string) {
      await delay(randomLatency());
      const hit = mockCatalog.find((e) => e.key === key);
      if (!hit) {
        throw new Error(`agent not found in catalog: ${key} [not_found]`);
      }
      return {
        ...hit,
        installChannels: hit.installChannels.map((c) => ({
          ...c,
          requires: [...c.requires],
        })),
        capabilities: { ...hit.capabilities },
      };
    },
  };
}
