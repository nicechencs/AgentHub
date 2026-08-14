import { seedAgentCatalog } from '@/app/runtime/agent-catalog-store';
import type { Backend, BackendFeatures, CreateBackend } from '@/lib/backend/contracts';
import {
  createMockAccountPort,
  getMockAccountById,
  resetMockAccounts,
  restoreMockAccount,
} from './account';
import { createMockAdapterPort, resetMockAdapters } from './adapter';
import { createMockAgentPort, resetMockAgentStatuses } from './agent';
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
import { createMockProjectPort, resetProjectMock } from './project';
import {
  createMockProviderPort,
  getMockProviderById,
  removeMockProvider,
  resetMockProviders,
  restoreMockProvider,
  upsertMockProvider,
} from './provider';
import { createMockSettingsPort } from './settings';
import { createMockSkillPort } from './skill';
import { createMockUpdatePort } from './update';
import { createMockUsagePort } from './usage';
import { createMockTrashPort, resetMockTrash } from './trash';

/** Mock implements switch undo + latency demos; export package stays closed. */
export const MOCK_BACKEND_FEATURES: BackendFeatures = {
  providerUndoSwitch: true,
  providerTestLatency: true,
  accountUndoSwitch: true,
  backupExport: false,
};

/** Browser / vitest backend — never selected by production build. */
export const createBackend: CreateBackend = () => {
  // Factory 创建干净状态（无需生产 port 上的 resetForTests）
  resetChatMock();
  resetProjectMock();
  resetMockAgentCatalog();
  resetMockConfig();
  resetMockTrash();
  resetMockAdapters();
  resetMockAccounts();
  resetMockProviders();
  resetMockAgentStatuses();
  // Seed full agent catalog (ids / names / channels / capabilities).
  seedAgentCatalog(MOCK_AGENT_CATALOG);

  const backend = {
    features: { ...MOCK_BACKEND_FEATURES },
    account: createMockAccountPort(),
    adapter: createMockAdapterPort({
      getAccountById: getMockAccountById,
      getProviderById: getMockProviderById,
      upsertGeneratedProvider: upsertMockProvider,
      removeGeneratedProvider: removeMockProvider,
    }),
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
  } as Backend;

  backend.env = createMockEnvPort(backend);
  backend.agent = createMockAgentPort(backend);
  return backend;
};

/**
 * Opt-in Adapter / ConnectFlow seed. Default `createBackend()` stays an empty pool.
 * Enable after the factory, e.g. `getBackend(); seedConnectFlowAdapterFixtures();`.
 */
export { seedConnectFlowAdapterFixtures } from './connect-flow-fixtures';

export default createBackend;
