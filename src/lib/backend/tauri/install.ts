import type { InstallPort } from '@/lib/backend/contracts';
import type {
  AgentInstallCatalogEntryDto,
  InstallOutcome,
  InstallProgressPayload,
} from '@/lib/backend/contracts/install-types';
import type { AgentKey, RuntimeId } from '@/lib/types';
import { logger } from '@/lib/logger';
import { onInstallProgress } from './install-events';
import { invoke } from './invoke';

const log = logger.scope('backend:tauri:install');

export function createTauriInstallPort(): InstallPort {
  return {
    async listInstallCatalog() {
      try {
        return await invoke<AgentInstallCatalogEntryDto[]>('list_install_catalog');
      } catch (e) {
        log.error('list_install_catalog failed', e);
        throw e;
      }
    },

    async installRuntime(runtimeId: RuntimeId, channel?: string) {
      try {
        // Omit an unspecified channel so Rust can select the host-native
        // package manager (Homebrew on macOS, winget on Windows).
        const args = channel === undefined ? { runtimeId } : { runtimeId, channel };
        return await invoke<InstallOutcome>('install_runtime', args);
      } catch (e) {
        log.error('install_runtime failed', e);
        throw e;
      }
    },

    async installAgentCmd(agentId: AgentKey, channel: string, installDeps = false) {
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

    async upgradeAgentCmd(agentId: AgentKey) {
      try {
        return await invoke<InstallOutcome>('upgrade_agent', { agentId });
      } catch (e) {
        log.error('upgrade_agent failed', e);
        throw e;
      }
    },

    async uninstallAgentCmd(agentId: AgentKey, purgeConfig: boolean) {
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

    async launchAgentProgram(kind: 'cli' | 'app', path: string) {
      try {
        await invoke('launch_agent_program', { kind, path });
      } catch (e) {
        log.error('launch_agent_program failed', e);
        throw e;
      }
    },

    async openAgentConfigDir(agentId: AgentKey) {
      try {
        return await invoke<string>('open_agent_config_dir', { agentId });
      } catch (e) {
        log.error('open_agent_config_dir failed', e);
        throw e;
      }
    },

    async getAgentLivePaths(agentId: AgentKey) {
      try {
        return await invoke<{
          config: string;
          auth?: string | null;
          extra?: string[];
          openDir: string;
        }>('get_agent_live_paths', { agentId });
      } catch (e) {
        log.error('get_agent_live_paths failed', e);
        throw e;
      }
    },

    onProgress(handler: (payload: InstallProgressPayload) => void) {
      return onInstallProgress(handler);
    },
  };
}
