import type { Backend, BackendFeatures, CreateBackend } from '@/lib/backend/contracts';
import { createTauriAccountPort } from './account';
import { createTauriAdapterPort } from './adapter';
import { createTauriAgentPort } from './agent';
import { createTauriTicketPort } from './ticket';
import { createTauriBackupPort } from './backup';
import { createTauriCatalogPort } from './catalog';
import { createTauriChatPort } from './chat';
import { createTauriConfigPort } from './config';
import { createTauriDashboardPort } from './dashboard';
import { createTauriDoctorPort } from './doctor';
import { createTauriEnvPort } from './env';
import { createTauriInstallPort } from './install';
import { createTauriMcpPort } from './mcp';
import { createTauriPluginPort } from './plugins';
import { createTauriProjectPort } from './project';
import { createTauriProviderPort } from './provider';
import { createTauriSettingsPort } from './settings';
import { createTauriSkillPort } from './skill';
import { createTauriUpdatePort } from './update';
import { createTauriUsagePort } from './usage';
import { createTauriTrashPort } from './trash';

/** Production / default dev backend: Tauri only (fail-closed outside shell). */
export const createBackend: CreateBackend = () => {
  const features: BackendFeatures = {
    providerUndoSwitch: true,
    providerTestLatency: true,
    accountUndoSwitch: true,
  };

  const backend = {
    features,
    account: createTauriAccountPort(),
    adapter: createTauriAdapterPort(),
    ticket: createTauriTicketPort(),
    catalog: createTauriCatalogPort(),
    config: createTauriConfigPort(),
    backup: createTauriBackupPort(),
    chat: createTauriChatPort(),
    project: createTauriProjectPort(),
    provider: createTauriProviderPort(),
    settings: createTauriSettingsPort(),
    skill: createTauriSkillPort(),
    usage: createTauriUsagePort(),
    doctor: createTauriDoctorPort(),
    install: createTauriInstallPort(),
    update: createTauriUpdatePort(),
    trash: createTauriTrashPort(),
    mcp: createTauriMcpPort(),
    plugins: createTauriPluginPort(),
  } as Backend;

  backend.env = createTauriEnvPort(backend);
  backend.agent = createTauriAgentPort(backend);
  backend.dashboard = createTauriDashboardPort(backend);
  return backend;
};

export default createBackend;
