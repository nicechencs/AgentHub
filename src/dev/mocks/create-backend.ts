import { seedAgentCatalog } from '@/app/runtime/agent-catalog-store';
import type { Backend, CreateBackend } from '@/lib/backend/contracts';
import { createMockAccountPort, restoreMockAccount } from './account';
import { createMockAgentPort } from './agent';
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
import { createMockProviderPort, restoreMockProvider } from './provider';
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
  // Seed full agent catalog (ids / names / channels / capabilities).
  seedAgentCatalog(MOCK_AGENT_CATALOG);

  const backend = {
    account: createMockAccountPort(),
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
