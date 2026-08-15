/**
 * Mock install catalog snapshot — mirrors core `catalog::list_install_catalog`
 * for the current host (dev:mock / vitest). Not a second production source.
 *
 * Windows native → `irm … | iex` + PowerShell requires.
 * macOS/Linux native → `curl … | bash` and no PowerShell dependency.
 * Codex has no Unix native channel (npm only).
 */
import type { AgentInstallCatalogEntryDto } from '@/lib/backend/contracts/install-types';
import { detectHostPlatform } from '@/lib/platform-detect';

const isWindows = detectHostPlatform() === 'windows';

export const MOCK_INSTALL_CATALOG: AgentInstallCatalogEntryDto[] = [
  {
    agentId: 'claude',
    channels: [
      {
        id: 'native',
        label: 'native 官方脚本',
        command: isWindows
          ? 'irm https://claude.ai/install.ps1 | iex'
          : 'curl -fsS https://claude.ai/install.sh | bash',
        requires: isWindows ? ['powershell'] : [],
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
      ...(isWindows
        ? [
            {
              id: 'native',
              label: 'native 官方脚本',
              command: 'irm https://openai.com/codex/install.ps1 | iex',
              requires: ['powershell' as const],
            },
          ]
        : []),
    ],
  },
  {
    agentId: 'kimi',
    channels: [
      {
        id: 'native',
        label: 'native 官方脚本',
        command: isWindows
          ? 'irm https://code.kimi.com/kimi-code/install.ps1 | iex'
          : 'curl -fsS https://code.kimi.com/kimi-code/install.sh | bash',
        requires: isWindows ? ['powershell'] : [],
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
        command: isWindows
          ? 'irm https://x.ai/cli/install.ps1 | iex'
          : 'curl -fsS https://x.ai/cli/install.sh | bash',
        requires: isWindows ? ['powershell'] : [],
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
        command: isWindows
          ? "irm 'https://cursor.com/install?win32=true' | iex"
          : 'curl -fsS https://cursor.com/install | bash',
        requires: isWindows ? ['powershell'] : [],
      },
    ],
  },
  {
    agentId: 'dsh',
    channels: [
      {
        id: 'npm',
        label: 'npm @deepseek-ai/dsh',
        command: 'npm i -g @deepseek-ai/dsh',
        requires: ['nodejs', 'npm'],
      },
    ],
  },
];
