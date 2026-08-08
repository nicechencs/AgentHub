/**
 * Mock install catalog snapshot — mirrors core `catalog::list_install_catalog`
 * for Windows-oriented commands (dev:mock / vitest). Not a second production source.
 */
import type { AgentInstallCatalogEntryDto } from '@/lib/backend/contracts/install-types';

export const MOCK_INSTALL_CATALOG: AgentInstallCatalogEntryDto[] = [
  {
    agentId: 'claude',
    channels: [
      {
        id: 'native',
        label: 'native 官方脚本',
        command: 'irm https://claude.ai/install.ps1 | iex',
        requires: ['powershell'],
      },
      {
        id: 'npm',
        label: 'npm @anthropic-ai/claude-code',
        command: 'npm i -g @anthropic-ai/claude-code',
        requires: ['nodejs', 'npm'],
      },
    ],
  },
  {
    agentId: 'codex',
    channels: [
      {
        id: 'npm',
        label: 'npm @openai/codex',
        command: 'npm i -g @openai/codex',
        requires: ['nodejs', 'npm'],
      },
      {
        id: 'native',
        label: 'native 官方脚本',
        command: 'irm https://openai.com/codex/install.ps1 | iex',
        requires: ['powershell'],
      },
    ],
  },
  {
    agentId: 'kimi',
    channels: [
      {
        id: 'native',
        label: 'native 官方脚本',
        command: 'irm https://code.kimi.com/kimi-code/install.ps1 | iex',
        requires: ['powershell'],
      },
      {
        id: 'npm',
        label: 'npm @moonshot-ai/kimi-code',
        command: 'npm i -g @moonshot-ai/kimi-code',
        requires: ['nodejs', 'npm'],
      },
    ],
  },
  {
    agentId: 'grok',
    channels: [
      {
        id: 'native',
        label: 'native 官方脚本',
        command: 'irm https://x.ai/cli/install.ps1 | iex',
        requires: ['powershell'],
      },
    ],
  },
  {
    agentId: 'pi',
    channels: [
      {
        id: 'npm',
        label: 'npm @earendil-works/pi-coding-agent',
        command: 'npm i -g --ignore-scripts @earendil-works/pi-coding-agent',
        requires: ['nodejs', 'npm'],
      },
    ],
  },
  {
    agentId: 'workbuddy',
    channels: [
      {
        id: 'native',
        label: '官网 Setup（打开安装页）',
        command: 'https://www.codebuddy.cn/work/',
        requires: [],
      },
    ],
  },
  {
    agentId: 'cursor',
    channels: [
      {
        id: 'native',
        label: 'Cursor Agent CLI 官方脚本',
        command: "irm 'https://cursor.com/install?win32=true' | iex",
        requires: ['powershell'],
      },
    ],
  },
];
