import { seedAgentCatalog } from '@/app/runtime/agent-catalog-store';
import type { Backend, BackendFeatures, CreateBackend } from '@/lib/backend/contracts';
import {
  createMockAccountPort,
  getMockAccountById,
  listMockAccounts,
  resetMockAccounts,
  restoreMockAccount,
} from './account';
import {
  createMockAdapterPort,
  getMockBridgeStatusSync,
  listMockAdapterProfiles,
  removeMockAdapterBinding,
  resetMockAdapters,
} from './adapter';
import { createMockAgentPort, resetMockAgentStatuses, resetMockAgentVisibility } from './agent';
import { createMockBackupPort } from './backup';
import { createMockCatalogPort, resetMockAgentCatalog } from './catalog';
import { createMockChatPort, resetChatMock } from './chat';
import { createMockConfigPort, resetMockConfig } from './config';
import { createMockDashboardPort } from './dashboard';
import { createMockDoctorPort } from './doctor';
import { createMockEnvPort } from './env';
import { MOCK_AGENT_CATALOG } from './fixtures/agent-catalog';
import { createMockInstallPort } from './install';
import { createMockMcpPort } from './mcp';
import { createMockPluginPort, resetMockPlugins } from './plugins';
import { createMockProjectPort, resetProjectMock } from './project';
import {
  createMockProviderPort,
  getMockProviderById,
  listMockProviders,
  removeMockProvider,
  resetMockProviders,
  restoreMockProvider,
  upsertMockProvider,
} from './provider';
import { createMockSettingsPort } from './settings';
import { createMockSkillPort, resetMockSkills } from './skill';
import { createMockUpdatePort } from './update';
import { createMockUsagePort, resetMockUsage } from './usage';
import { createMockTrashPort, resetMockTrash } from './trash';
import { createMockTicketPort } from './ticket';
import { seedConnectFlowAdapterFixtures } from './connect-flow-fixtures';

/** Mock implements switch undo + latency demos; export package stays closed. */
export const MOCK_BACKEND_FEATURES: BackendFeatures = {
  providerUndoSwitch: true,
  providerTestLatency: true,
  accountUndoSwitch: true,
  backupExport: false,
};

/**
 * Browser mock backend — never selected by production build.
 *
 * Interactive `pnpm dev:mock` seeds demo ConnectFlow credentials after reset
 * (Kimi membership + Anthropic API + unknown + OAuth account, Pi marked installed)
 * so Adapter plan/apply and the ticket wallet are reachable. The vitest factory
 * stays an empty pool: no seed when `import.meta.env.VITEST` is set or
 * `import.meta.env.MODE === 'test'`.
 */
export const createBackend: CreateBackend = () => {
  // Factory 创建干净状态（无需生产 port 上的 resetForTests）
  resetChatMock();
  resetProjectMock();
  resetMockAgentCatalog();
  resetMockConfig();
  resetMockSkills();
  resetMockUsage();
  resetMockTrash();
  resetMockAdapters();
  resetMockAccounts();
  resetMockProviders();
  resetMockAgentStatuses();
  resetMockAgentVisibility();
  resetMockPlugins();
  // Seed full agent catalog (ids / names / channels / capabilities).
  seedAgentCatalog(MOCK_AGENT_CATALOG);

  const adapter = createMockAdapterPort({
    getAccountById: getMockAccountById,
    getProviderById: getMockProviderById,
    upsertGeneratedProvider: upsertMockProvider,
    removeGeneratedProvider: removeMockProvider,
  });

  const ticket = createMockTicketPort({
    sources: {
      listAccounts: listMockAccounts,
      listProviders: listMockProviders,
      listProfiles: listMockAdapterProfiles,
      getBridgeStatus: getMockBridgeStatusSync,
    },
    adapter: {
      planAdapter: (request) => adapter.plan(request),
      applyAdapter: (request) => adapter.apply(request),
      removeBinding: (profileId) => removeMockAdapterBinding(profileId),
    },
  });

  if (!import.meta.env.VITEST && import.meta.env.MODE !== 'test') {
    seedConnectFlowAdapterFixtures({
      includeUnknown: true,
      includeOauthAccount: true,
      seedBindings: true,
    });
  }

  const backend = {
    features: { ...MOCK_BACKEND_FEATURES },
    account: createMockAccountPort(),
    adapter,
    ticket,
    catalog: createMockCatalogPort(),
    config: createMockConfigPort(),
    backup: createMockBackupPort(),
    chat: createMockChatPort(),
    project: createMockProjectPort(),
    provider: createMockProviderPort(),
    settings: createMockSettingsPort(),
    skill: createMockSkillPort(),
    usage: createMockUsagePort(),
    dashboard: createMockDashboardPort(),
    doctor: createMockDoctorPort(),
    install: createMockInstallPort(),
    update: createMockUpdatePort(),
    trash: createMockTrashPort({
      restoreAccount: restoreMockAccount,
      restoreProvider: restoreMockProvider,
    }),
    mcp: createMockMcpPort(),
    plugins: createMockPluginPort(),
  } as Backend;

  backend.env = createMockEnvPort(backend);
  backend.agent = createMockAgentPort(backend);
  return backend;
};

/**
 * Opt-in Adapter / ConnectFlow seed. Tests keep an empty pool from `createBackend()`
 * and call this after `getBackend()` when they need apply-ready fixtures.
 * Interactive `dev:mock` already seeds inside `createBackend()`.
 */
export { seedConnectFlowAdapterFixtures };

export default createBackend;
