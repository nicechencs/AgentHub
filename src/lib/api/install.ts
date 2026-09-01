/**
 * Install command façade — production uses Tauri install port.
 */
import { getBackend } from '@/app/runtime';
import type {
  InstallOutcome,
  InstallProgressPayload,
} from '@/lib/backend/contracts/install-types';
import { isProgressForAgent } from '@/lib/backend/contracts/install-types';
import type { AgentId, RuntimeId } from '@/lib/types';

export type { InstallOutcome, InstallProgressPayload };
export { isProgressForAgent };

export async function installRuntime(
  runtimeId: RuntimeId,
  /** Omit to let the host pick brew (macOS) or winget (Windows). */
  channel?: string,
): Promise<InstallOutcome> {
  return getBackend().install.installRuntime(runtimeId, channel);
}

export async function installAgentCmd(
  agentId: AgentId,
  channel: string,
  installDeps: boolean = false,
): Promise<InstallOutcome> {
  return getBackend().install.installAgentCmd(agentId, channel, installDeps);
}

export async function upgradeAgentCmd(agentId: AgentId): Promise<InstallOutcome> {
  return getBackend().install.upgradeAgentCmd(agentId);
}

export async function uninstallAgentCmd(
  agentId: AgentId,
  purgeConfig: boolean,
): Promise<InstallOutcome> {
  return getBackend().install.uninstallAgentCmd(agentId, purgeConfig);
}

export async function openAgentConfigDir(agentId: AgentId): Promise<string> {
  return getBackend().install.openAgentConfigDir(agentId);
}

export async function launchAgentProgram(
  kind: 'cli' | 'app',
  path: string,
): Promise<void> {
  return getBackend().install.launchAgentProgram(kind, path);
}

export async function getAgentLivePaths(agentId: AgentId): Promise<{
  config: string;
  auth?: string | null;
  extra?: string[];
  openDir: string;
}> {
  return getBackend().install.getAgentLivePaths(agentId);
}

/** Subscribe to live install/upgrade/uninstall log lines. */
export async function onInstallProgress(
  handler: (payload: InstallProgressPayload) => void,
): Promise<() => void> {
  return getBackend().install.onProgress(handler);
}
