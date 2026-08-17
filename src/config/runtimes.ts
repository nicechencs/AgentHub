import type { EnvRemediation, RuntimeId } from '@/lib/types';
import type { HostPlatform } from '@/lib/platform-detect';

export interface RuntimeMeta {
  id: RuntimeId;
  name: string;
  /** 简短展示名(环境条) */
  shortName: string;
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
    description: 'Claude / Codex 等 npm 渠道的硬依赖',
    minVersion: '18',
    canAutoInstall: true,
    remediations: [
      {
        kind: 'winget',
        value: 'winget install OpenJS.NodeJS.LTS',
        label: '用 winget 安装 LTS',
        platform: 'windows',
      },
      {
        kind: 'brew',
        value: 'brew install node',
        label: '用 Homebrew 安装 Node.js',
        platform: 'macos',
      },
      {
        kind: 'command',
        value: 'sudo apt-get install -y nodejs npm',
        label: 'Debian/Ubuntu 安装 Node.js',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo dnf install -y nodejs npm',
        label: 'Fedora 安装 Node.js',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo pacman -S --needed nodejs npm',
        label: 'Arch 安装 Node.js',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo zypper install -y nodejs npm',
        label: 'openSUSE 安装 Node.js',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo apk add nodejs npm',
        label: 'Alpine 安装 Node.js',
        platform: 'linux',
      },
      {
        kind: 'hint',
        value:
          '其他发行版不要套用 apt-get。请用本机包管理器，或打开 Node.js 官网安装 LTS。',
        platform: 'linux',
      },
      {
        kind: 'url',
        value: 'https://nodejs.org/',
        label: '打开 Node.js 官网',
      },
      {
        kind: 'hint',
        value: '安装完成后请完全退出并重启 AgentHub,再点「重新检测」(GUI 可能继承旧 PATH)。',
      },
    ],
  },
  {
    id: 'npm',
    name: 'npm',
    shortName: 'npm',
    description: '通常随 Node.js 安装;若 node 在而 npm 不在,请修复 PATH 或重装 Node',
    canAutoInstall: false,
    remediations: [
      {
        kind: 'command',
        value: 'node -v && npm -v',
        label: '检测 node / npm',
      },
      {
        kind: 'hint',
        value: 'npm 一般随 Node 安装。若仅缺 npm:重装 Node LTS,或检查 PATH 是否包含 npm 目录。',
      },
      {
        kind: 'url',
        value: 'https://nodejs.org/',
        label: '重装 Node.js LTS',
      },
    ],
  },
  {
    id: 'powershell',
    name: 'PowerShell',
    shortName: 'PS',
    description:
      'Windows native 安装脚本运行时。可分别识别 5.1 与 7(pwsh)，任一可用即可。macOS/Linux 不检测 PowerShell，native 安装走官方 bash/sh。',
    canAutoInstall: false,
    remediations: [
      {
        kind: 'hint',
        value:
          'Windows 通常自带 PowerShell 5.1；PowerShell 7 (pwsh) 可选但更推荐。AgentHub 不提供一键安装 PowerShell。若脚本被策略拦截，请调整 ExecutionPolicy。',
        platform: 'windows',
      },
      {
        kind: 'url',
        value:
          'https://learn.microsoft.com/powershell/scripting/install/installing-powershell',
        label: '安装 PowerShell 7',
        platform: 'windows',
      },
    ],
  },
  {
    id: 'git',
    name: 'Git',
    shortName: 'Git',
    description:
      'Skills 市场与 git URL 安装依赖（git clone / pull）。Agent 安装渠道不强制要求，但缺失时无法从远程装技能。',
    canAutoInstall: true,
    remediations: [
      {
        kind: 'winget',
        value: 'winget install --id Git.Git -e --source winget',
        label: '用 winget 安装 Git',
        platform: 'windows',
      },
      {
        kind: 'brew',
        value: 'brew install git',
        label: '用 Homebrew 安装 Git',
        platform: 'macos',
      },
      {
        kind: 'command',
        value: 'sudo apt-get install -y git',
        label: 'Debian/Ubuntu 安装 Git',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo dnf install -y git',
        label: 'Fedora 安装 Git',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo pacman -S --needed git',
        label: 'Arch 安装 Git',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo zypper install -y git',
        label: 'openSUSE 安装 Git',
        platform: 'linux',
      },
      {
        kind: 'command',
        value: 'sudo apk add git',
        label: 'Alpine 安装 Git',
        platform: 'linux',
      },
      {
        kind: 'hint',
        value: '其他发行版不要套用 apt-get。请用本机包管理器，或打开 Git 官网下载。',
        platform: 'linux',
      },
      {
        kind: 'url',
        value: 'https://git-scm.com/downloads',
        label: '打开 Git 官网下载',
      },
      {
        kind: 'hint',
        value:
          '安装完成后请完全退出并重启 AgentHub，再点「重新检测」（GUI 可能继承旧 PATH）。',
      },
    ],
  },
];

export const RUNTIME_MAP: Record<RuntimeId, RuntimeMeta> = Object.fromEntries(
  RUNTIMES.map((r) => [r.id, r]),
) as Record<RuntimeId, RuntimeMeta>;
