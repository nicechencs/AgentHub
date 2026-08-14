import {
  DEFAULT_BACKEND_FEATURES,
  type Backend,
  type CreateBackend,
} from '@/lib/backend/contracts';
import { createTauriAccountPort } from './account';
import { createTauriAdapterPort } from './adapter';
import { createTauriAgentPort } from './agent';
import { createTauriBackupPort } from './backup';
import { createTauriCatalogPort } from './catalog';
import { createTauriChatPort } from './chat';
import { createTauriConfigPort } from './config';
import { createTauriDashboardPort } from './dashboard';
import { createTauriDoctorPort } from './doctor';
import { createTauriEnvPort } from './env';
import { createTauriInstallPort } from './install';
import { createTauriMcpPort } from './mcp';
import { createTauriProjectPort } from './project';
import { createTauriProviderPort } from './provider';
import { createTauriSettingsPort } from './settings';
import { createTauriSkillPort } from './skill';
import { createTauriUpdatePort } from './update';
import { createTauriUsagePort } from './usage';
import { createTauriTrashPort } from './trash';

/** Production / default dev backend: Tauri only (fail-closed outside shell). */
export const createBackend: CreateBackend = () => {
  const backend = {
    // Undo / latency / export package are not wired in production yet.
    // UI must hide those actions; port methods still throw if invoked.
    features: { ...DEFAULT_BACKEND_FEATURES },
    account: createTauriAccountPort(),
    adapter: createTauriAdapterPort(),
    catalog: createTauriCatalogPort(),
    config: createTauriConfigPort(),
    backup: createTauriBackupPort(),
    chat: createTauriChatPort(),
    project: createTauriProjectPort(),
    provider: createTauriProviderPort(),
    settings: createTauriSettingsPort(),
    skill: createTauriSkillPort(),
    usage: createTauriUsagePort(),
    dashboard: createTauriDashboardPort(),
    doctor: createTauriDoctorPort(),
    install: createTauriInstallPort(),
    update: createTauriUpdatePort(),
    trash: createTauriTrashPort(),
    mcp: createTauriMcpPort(),
  } as Backend;

  backend.env = createTauriEnvPort(backend);
  backend.agent = createTauriAgentPort(backend);
  return backend;
};

export default createBackend;
