import { seedAgentCatalog } from '@/app/runtime/agent-catalog-store';
import type { Backend, CreateBackend } from '@/lib/backend/contracts';
import { createMockAccountPort, getMockAccountById, restoreMockAccount } from './account';
import { createMockAdapterPort, resetMockAdapters } from './adapter';
import { createMockAgentPort, resetMockAgentVisibility } from './agent';
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

/** Browser / vitest backend — never selected by production build. */
export const createBackend: CreateBackend = () => {
  // Factory 创建干净状态（无需生产 port 上的 resetForTests）
  resetChatMock();
  resetProjectMock();
  resetMockAgentCatalog();
  resetMockConfig();
  resetMockTrash();
  resetMockAdapters();
  resetMockProviders();
  resetMockAgentVisibility();
  // Seed full agent catalog (ids / names / channels / capabilities).
  seedAgentCatalog(MOCK_AGENT_CATALOG);

  const backend = {
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

export default createBackend;
