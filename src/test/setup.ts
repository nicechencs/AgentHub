/**
 * Vitest setup — mock backend is selected via vitest.config.ts `#backend` alias.
 * Domain tests may call reset helpers from `@/dev/mocks/*` when needed.
 */
import { beforeEach } from 'vitest';
import { resetBackend, seedAgentCatalog } from '@/app/runtime';
import { resetMockAgentCatalog } from '@/dev/mocks/catalog';
import { MOCK_AGENT_CATALOG } from '@/dev/mocks/fixtures/agent-catalog';

beforeEach(() => {
  // Fresh backend per test file case unless a suite keeps state intentionally.
  resetBackend();
  resetMockAgentCatalog();
  // Product agent set + channels from mock catalog (not static AGENTS).
  seedAgentCatalog(MOCK_AGENT_CATALOG);
});
