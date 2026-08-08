import type { InstallPort } from '@/lib/backend/contracts';
import type {
  AgentInstallCatalogEntryDto,
  InstallOutcome,
} from '@/lib/backend/contracts/install-types';
import type { AgentId, RuntimeId } from '@/lib/types';
import { logger } from '@/lib/logger';
import { invoke } from './invoke';

const log = logger.scope('backend:tauri:install');

export function createTauriInstallPort(): InstallPort {
  return {
    async listInstallCatalog() {
      try {
        return await invoke<AgentInstallCatalogEntryDto[]>('list_install_catalog_cmd');
      } catch (e) {
        log.error('list_install_catalog failed', e);
        throw e;
      }
    },

    async installRuntime(runtimeId: RuntimeId, channel = 'winget') {
      try {
        return await invoke<InstallOutcome>('install_runtime', { runtimeId, channel });
      } catch (e) {
        log.error('install_runtime failed', e);
        throw e;
      }
    },

    async installAgentCmd(agentId: AgentId, channel: string, installDeps = false) {
      try {
        return await invoke<InstallOutcome>('install_agent', {
          agentId,
          channel,
          installDeps,
        });
      } catch (e) {
        log.error('install_agent failed', e);
        throw e;
      }
    },

    async upgradeAgentCmd(agentId: AgentId) {
      try {
        return await invoke<InstallOutcome>('upgrade_agent', { agentId });
      } catch (e) {
        log.error('upgrade_agent failed', e);
        throw e;
      }
    },

    async uninstallAgentCmd(agentId: AgentId, purgeConfig: boolean) {
      try {
        return await invoke<InstallOutcome>('uninstall_agent', {
          agentId,
          purgeConfig,
        });
      } catch (e) {
        log.error('uninstall_agent failed', e);
        throw e;
      }
    },

    async openAgentConfigDir(agentId: AgentId) {
      try {
        return await invoke<string>('open_agent_config_dir', { agentId });
      } catch (e) {
        log.error('open_agent_config_dir failed', e);
        throw e;
      }
    },
  };
}
