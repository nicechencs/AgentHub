import type { AgentId, RuntimeId } from '@/lib/types';
import type {
  AgentInstallCatalogEntryDto,
  InstallOutcome,
  InstallProgressPayload,
} from './install-types';

export interface InstallPort {
  /** Core catalog install channels (allowlisted URLs / npm packages). */
  listInstallCatalog(): Promise<AgentInstallCatalogEntryDto[]>;
  installRuntime(runtimeId: RuntimeId, channel?: string): Promise<InstallOutcome>;
  installAgentCmd(
    agentId: AgentId,
    channel: string,
    installDeps?: boolean,
  ): Promise<InstallOutcome>;
  upgradeAgentCmd(agentId: AgentId): Promise<InstallOutcome>;
  uninstallAgentCmd(agentId: AgentId, purgeConfig: boolean): Promise<InstallOutcome>;
  openAgentConfigDir(agentId: AgentId): Promise<string>;
  /** Start a CLI in a new terminal, or a desktop app. */
  launchAgentProgram(kind: 'cli' | 'app', path: string): Promise<void>;
  getAgentLivePaths(agentId: AgentId): Promise<{
    config: string;
    auth?: string | null;
    extra?: string[];
    openDir: string;
  }>;
  /** Subscribe to live install/upgrade/uninstall log lines. Returns an unsubscribe function. */
  onProgress(
    handler: (payload: InstallProgressPayload) => void,
  ): Promise<() => void> | (() => void);
}
