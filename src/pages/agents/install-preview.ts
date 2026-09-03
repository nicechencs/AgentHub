import { AGENT_MAP } from '@/config/agents';
import { RUNTIME_MAP } from '@/config/runtimes';
import { runtimeChannelForPlan } from '@/lib/env-plan';
import { detectHostPlatform, type HostPlatform } from '@/lib/platform-detect';
import type { AgentKey, RuntimeId } from '@/lib/types';

/**
 * Feature-local install command preview (copy / display only).
 * Does not execute installs and does not claim success.
 *
 * Upgrade reuses the same platform-aware channel the agent was installed with:
 * npm → `npm i -g …@latest`; native → Windows `irm … | iex` / macOS `curl … | bash`.
 * The CLI entrypoint remains `agenthub agent upgrade` on every host.
 */
export function buildAgentInstallPreview(
  agentId: AgentKey,
  action: 'install' | 'upgrade',
  channel?: string,
  platform: HostPlatform = detectHostPlatform(),
): string[] {
  const meta = AGENT_MAP[agentId];
  const name = meta?.name ?? agentId;
  if (action === 'upgrade') {
    const lines = [`$ agenthub agent upgrade ${agentId}`, `# target: ${name}`];
    const channelId =
      channel ??
      meta?.installChannels.find((c) => c.id === 'npm')?.id ??
      meta?.installChannels[0]?.id;
    const chMeta = meta?.installChannels.find((c) => c.id === channelId);
    if (chMeta?.command) {
      // Show the platform-specific underlying command (already filtered by catalog).
      lines.push(`# underlying (${platform}): ${chMeta.command}`);
    } else if (platform === 'windows') {
      lines.push('# underlying: Windows re-runs allowlisted native .ps1 or npm latest');
    } else {
      lines.push('# underlying: macOS/Linux re-runs allowlisted install.sh or npm latest');
    }
    return lines;
  }
  const ch = channel ? ` --channel ${channel}` : '';
  const lines = [`$ agenthub agent install ${agentId}${ch}`, `# target: ${name}`];
  const chMeta = channel
    ? meta?.installChannels.find((c) => c.id === channel)
    : meta?.installChannels[0];
  if (chMeta?.command) {
    lines.push(`# underlying (${platform}): ${chMeta.command}`);
  }
  return lines;
}

export function buildEnvInstallPreview(
  targets: RuntimeId[],
  channel: string = runtimeChannelForPlan(),
): string[] {
  if (targets.length === 0) return ['# no auto-install targets'];
  return targets.map((id) => {
    const meta = RUNTIME_MAP[id === 'npm' ? 'nodejs' : id];
    if (id === 'powershell') {
      // PowerShell is Windows-only and never one-click installed.
      return `# PowerShell is Windows-only; native installers on macOS use bash/sh`;
    }
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
    if (
      channel === 'manual' ||
      channel === 'apt' ||
      channel === 'dnf' ||
      channel === 'pacman' ||
      channel === 'zypper' ||
      channel === 'apk'
    ) {
      if (id === 'nodejs' || id === 'npm') {
        return `# Linux: sudo apt-get install -y nodejs npm   # or dnf/pacman/zypper/apk; unknown distros: https://nodejs.org/ LTS`;
      }
      if (id === 'git') {
        return `# Linux: sudo apt-get install -y git   # or dnf/pacman/zypper/apk; unknown distros: https://git-scm.com/downloads`;
      }
      return `# Linux has no one-click installer for ${id}; use the distro package manager or official download`;
    }
    return `$ agenthub env install ${id} --channel ${channel}`;
  });
}
