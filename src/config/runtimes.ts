import type { EnvRemediation, RuntimeId } from '@/lib/types';
import type { HostPlatform } from '@/lib/platform-detect';

export interface RuntimeMeta {
  id: RuntimeId;
  name: string;
  /** 简短展示名(环境条) */
  shortName: string;
  /** English source-of-truth description; UI prefers env.runtimes.<id>.description via t(). */
  description: string;
  minVersion?: string;
  /** 是否支持在 App 内引导自动安装 */
  canAutoInstall: boolean;
  remediations: EnvRemediation[];
}

/**
 * Keep package-manager guidance honest for the host webview. Backend doctor
 * data may come from an older core and omit a platform marker, so filter by
 * both the explicit marker and the remediation kind.
 */
export function runtimeRemediationsForPlatform(
  remediations: EnvRemediation[],
  platform: HostPlatform = 'unknown',
): EnvRemediation[] {
  return remediations.filter((item) => {
    if (item.platform && item.platform !== platform) return false;
    if (item.kind === 'winget') return platform === 'windows';
    if (item.kind === 'brew') return platform === 'macos';
    return true;
  });
}

/** Shared runtimes relevant on the current host (PowerShell is Windows-only). */
export function runtimesForPlatform(
  platform: HostPlatform = 'unknown',
): RuntimeMeta[] {
  if (platform === 'windows') return RUNTIMES;
  return RUNTIMES.filter((r) => r.id !== 'powershell');
}

/** 共享运行时元数据(docs/agenthub-plan.md §5.7.2) */
export const RUNTIMES: RuntimeMeta[] = [
  {
    id: 'nodejs',
    name: 'Node.js',
    shortName: 'Node',
    description: 'Hard dependency for npm-based channels like Claude / Codex',
    minVersion: '18',
    canAutoInstall: true,
    remediations: [
      {
        kind: 'winget',
        value: 'winget install OpenJS.NodeJS.LTS',
        label: 'Install LTS via winget',
        platform: 'windows',
      },
      {
        kind: 'brew',
        value: 'brew install node',
        label: 'Install Node.js via Homebrew',
        platform: 'macos',
      },
      {
        kind: 'command',
        value: 'sudo apt-get install -y nodejs npm',
        label: 'Install Node.js on Debian/Ubuntu',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo dnf install -y nodejs npm',
        label: 'Install Node.js on Fedora',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo pacman -S --needed nodejs npm',
        label: 'Install Node.js on Arch',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo zypper install -y nodejs npm',
        label: 'Install Node.js on openSUSE',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo apk add nodejs npm',
        label: 'Install Node.js on Alpine',
        platform: 'linux',
      },
      {
        kind: 'hint',
        value:
          "Other distros: don't just use apt-get. Use your local package manager, or open the Node.js website to install the LTS release.",
        platform: 'linux',
      },
      {
        kind: 'url',
        value: 'https://nodejs.org/',
        label: 'Open the Node.js website',
      },
      {
        kind: 'hint',
        value: 'After installing, fully quit and restart AgentHub, then click "Re-detect" (the GUI may still have the old PATH).',
      },
    ],
  },
  {
    id: 'npm',
    name: 'npm',
    shortName: 'npm',
    description: 'Usually installed with Node.js; if node is present but npm is missing, fix PATH or reinstall Node',
    canAutoInstall: false,
    remediations: [
      {
        kind: 'command',
        value: 'node -v && npm -v',
        label: 'Check node / npm',
      },
      {
        kind: 'hint',
        value: 'npm is usually installed with Node. If only npm is missing, reinstall Node LTS, or check whether PATH includes the npm directory.',
      },
      {
        kind: 'url',
        value: 'https://nodejs.org/',
        label: 'Reinstall Node.js LTS',
      },
    ],
  },
  {
    id: 'powershell',
    name: 'PowerShell',
    shortName: 'PS',
    description:
      "Runtime for Windows native install scripts. Detects either 5.1 or 7 (pwsh); either one is enough. macOS/Linux don't check PowerShell — native installs use the official bash/sh scripts.",
    canAutoInstall: false,
    remediations: [
      {
        kind: 'hint',
        value:
          "Windows usually ships with PowerShell 5.1; PowerShell 7 (pwsh) is optional but recommended. AgentHub doesn't offer a one-click PowerShell install. If scripts are blocked by policy, adjust ExecutionPolicy.",
        platform: 'windows',
      },
      {
        kind: 'url',
        value:
          'https://learn.microsoft.com/powershell/scripting/install/installing-powershell',
        label: 'Install PowerShell 7',
        platform: 'windows',
      },
    ],
  },
  {
    id: 'git',
    name: 'Git',
    shortName: 'Git',
    description:
      "Dependency for the Skills marketplace and git URL installs (git clone / pull). Not required by Agent install channels, but skills can't be installed from a remote source without it.",
    canAutoInstall: true,
    remediations: [
      {
        kind: 'winget',
        value: 'winget install --id Git.Git -e --source winget',
        label: 'Install Git via winget',
        platform: 'windows',
      },
      {
        kind: 'brew',
        value: 'brew install git',
        label: 'Install Git via Homebrew',
        platform: 'macos',
      },
      {
        kind: 'command',
        value: 'sudo apt-get install -y git',
        label: 'Install Git on Debian/Ubuntu',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo dnf install -y git',
        label: 'Install Git on Fedora',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo pacman -S --needed git',
        label: 'Install Git on Arch',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo zypper install -y git',
        label: 'Install Git on openSUSE',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo apk add git',
        label: 'Install Git on Alpine',
        platform: 'linux',
      },
      {
        kind: 'hint',
        value: "Other distros: don't just use apt-get. Use your local package manager, or open the Git website to download it.",
        platform: 'linux',
      },
      {
        kind: 'url',
        value: 'https://git-scm.com/downloads',
        label: 'Open the Git website',
      },
      {
        kind: 'hint',
        value:
          'After installing, fully quit and restart AgentHub, then click "Re-detect" (the GUI may still have the old PATH).',
      },
    ],
  },
];

export const RUNTIME_MAP: Record<RuntimeId, RuntimeMeta> = Object.fromEntries(
  RUNTIMES.map((r) => [r.id, r]),
) as Record<RuntimeId, RuntimeMeta>;

/** i18n key for a runtime's localized description; falls back to `meta.description` (English) when absent. */
export function runtimeDescriptionKey(id: RuntimeId): `env.runtimes.${RuntimeId}.description` {
  return `env.runtimes.${id}.description`;
}
