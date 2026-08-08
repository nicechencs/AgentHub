import type { EnvRemediation, RuntimeId } from '@/lib/types';

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
      'native 安装脚本运行时。Windows 可分别识别 5.1 与 7(pwsh)，任一可用即可；macOS 仅检测 pwsh。',
    canAutoInstall: false,
    remediations: [
      {
        kind: 'hint',
        value:
          'Windows 通常自带 PowerShell 5.1；PowerShell 7 (pwsh) 可选但更推荐。AgentHub 不提供一键安装 PowerShell。若脚本被策略拦截，请调整 ExecutionPolicy。',
      },
      {
        kind: 'url',
        value:
          'https://learn.microsoft.com/powershell/scripting/install/installing-powershell',
        label: '安装 PowerShell 7',
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
