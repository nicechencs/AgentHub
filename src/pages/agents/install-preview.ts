import { AGENT_MAP } from '@/config/agents';
import { RUNTIME_MAP } from '@/config/runtimes';
import { runtimeChannelForPlan } from '@/lib/env-plan';
import type { AgentId, RuntimeId } from '@/lib/types';

/**
 * Feature-local install command preview (copy / display only).
 * Does not execute installs and does not claim success.
 */
export function buildAgentInstallPreview(
  agentId: AgentId,
  action: 'install' | 'upgrade',
  channel?: string,
): string[] {
  const name = AGENT_MAP[agentId]?.name ?? agentId;
  if (action === 'upgrade') {
    return [`$ agenthub agent upgrade ${agentId}`, `# target: ${name}`];
  }
  const ch = channel ? ` --channel ${channel}` : '';
  return [`$ agenthub agent install ${agentId}${ch}`, `# target: ${name}`];
}

export function buildEnvInstallPreview(
  targets: RuntimeId[],
  channel: string = runtimeChannelForPlan(),
): string[] {
  if (targets.length === 0) return ['# no auto-install targets'];
  return targets.map((id) => {
    const meta = RUNTIME_MAP[id === 'npm' ? 'nodejs' : id];
    if ((id === 'nodejs' || id === 'npm') && channel === 'winget') {
      return `$ winget install OpenJS.NodeJS.LTS  # ${meta?.name ?? id}`;
    }
    if ((id === 'nodejs' || id === 'npm') && channel === 'brew') {
      return `$ brew install node  # ${meta?.name ?? id}`;
    }
    if (id === 'git' && channel === 'winget') {
      return `$ winget install -e --id Git.Git  # ${meta?.name ?? id}`;
    }
    if (id === 'git' && channel === 'brew') {
      return `$ brew install git  # ${meta?.name ?? id}`;
    }
    if (id === 'powershell' && channel === 'brew') {
      return `$ brew install --cask powershell  # ${meta?.name ?? id}`;
    }
    return `$ agenthub env install ${id} --channel ${channel}`;
  });
}
