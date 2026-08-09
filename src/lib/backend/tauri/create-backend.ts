import type { Backend, CreateBackend } from '@/lib/backend/contracts';
import { createTauriAccountPort } from './account';
import { createTauriAgentPort } from './agent';
import { createTauriBackupPort } from './backup';
import { createTauriCatalogPort } from './catalog';
import { createTauriChatPort } from './chat';
import { createTauriConfigPort } from './config';
import { createTauriDashboardPort } from './dashboard';
import { createTauriDoctorPort } from './doctor';
import { createTauriEnvPort } from './env';
import { createTauriInstallPort } from './install';
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
    account: createTauriAccountPort(),
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
  } as Backend;

  backend.env = createTauriEnvPort(backend);
  backend.agent = createTauriAgentPort(backend);
  return backend;
};

export default createBackend;
