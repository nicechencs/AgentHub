import { RUNTIME_MAP, runtimesForPlatform } from '@/config/runtimes';
import type { Backend, EnvPort } from '@/lib/backend/contracts';
import { delay, randomLatency } from '@/dev/mocks/delay';
import {
  detectHostPlatform,
  getRuntimeInstallChannel,
  type HostPlatform,
} from '@/lib/platform-detect';
import { loadJson, saveJson } from '@/lib/ui-preferences';
import type { EnvStatus, RuntimeDetect, RuntimeId } from '@/lib/types';

const STORAGE_KEY = 'agenthub:runtime-state';

type RuntimeState = Partial<
  Record<RuntimeId, { status: EnvStatus; version?: string; path?: string }>
>;

function defaultChannel(): string {
  return getRuntimeInstallChannel(detectHostPlatform());
}

function defaultState(): RuntimeState {
  const platform = detectHostPlatform();
  const state: RuntimeState = {
    nodejs: { status: 'missing' },
    npm: { status: 'missing' },
    git: { status: 'missing' },
  };
  // PowerShell is a Windows-only shared runtime; omit on macOS/Linux mocks.
  if (platform === 'windows') {
    state.powershell = {
      status: 'ok',
      version: '5.1',
      path: 'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe',
    };
  }
  return state;
}

function readState(): RuntimeState {
  return loadJson<RuntimeState>(STORAGE_KEY, defaultState());
}

function writeState(state: RuntimeState): void {
  saveJson(STORAGE_KEY, state);
}

function toDetect(
  id: RuntimeId,
  s: RuntimeState[RuntimeId] | undefined,
): RuntimeDetect {
  const meta = RUNTIME_MAP[id];
  const status = s?.status ?? 'missing';
  return {
    id,
    status,
    version: s?.version,
    path: s?.path,
    minRequired: meta.minVersion,
    remediations: meta.remediations,
  };
}

/** Platform-shaped installed paths so macOS mock dogfood never shows Windows paths. */
function mockInstalledRuntime(
  id: 'nodejs' | 'npm' | 'git',
  platform: HostPlatform,
): { status: EnvStatus; version: string; path: string } {
  if (platform === 'macos') {
    if (id === 'nodejs') {
      return { status: 'ok', version: '20.11.1', path: '/opt/homebrew/bin/node' };
    }
    if (id === 'npm') {
      return { status: 'ok', version: '10.2.4', path: '/opt/homebrew/bin/npm' };
    }
    return { status: 'ok', version: '2.43.0', path: '/opt/homebrew/bin/git' };
  }
  if (platform === 'linux' || platform === 'unknown') {
    if (id === 'nodejs') {
      return { status: 'ok', version: '20.11.1', path: '/usr/bin/node' };
    }
    if (id === 'npm') {
      return { status: 'ok', version: '10.2.4', path: '/usr/bin/npm' };
    }
    return { status: 'ok', version: '2.43.0', path: '/usr/bin/git' };
  }
  if (id === 'nodejs') {
    return {
      status: 'ok',
      version: '20.11.1',
      path: 'C:\\Program Files\\nodejs\\node.exe',
    };
  }
  if (id === 'npm') {
    return {
      status: 'ok',
      version: '10.2.4',
      path: 'C:\\Program Files\\nodejs\\npm.cmd',
    };
  }
  return {
    status: 'ok',
    version: '2.43.0.windows.1',
    path: 'C:\\Program Files\\Git\\cmd\\git.exe',
  };
}

function mockEnvInstallLogs(id: RuntimeId, channel: string): string[] {
  const meta = RUNTIME_MAP[id === 'npm' ? 'nodejs' : id];
  if ((id === 'nodejs' || id === 'npm') && channel === 'winget') {
    return [
      `$ winget install OpenJS.NodeJS.LTS`,
      `Found Node.js LTS [OpenJS.NodeJS.LTS]`,
      `This application is licensed to you by its owner.`,
      `Downloading https://nodejs.org/.../node-v20.11.1-x64.msi`,
      `  ████████████████████  100%`,
      `Successfully installed`,
      `验证: node -v → v20.11.1`,
      `验证: npm -v → 10.2.4`,
      `✓ Node.js + npm 环境就绪(mock)`,
    ];
  }
  if (id === 'git' && channel === 'winget') {
    return [
      `$ winget install -e --id Git.Git`,
      `Found Git [Git.Git]`,
      `This application is licensed to you by its owner.`,
      `Downloading Git for Windows...`,
      `  ████████████████████  100%`,
      `Successfully installed`,
      `验证: git --version → git version 2.43.0.windows.1`,
      `✓ Git 环境就绪(mock)`,
    ];
  }
  if ((id === 'nodejs' || id === 'npm') && channel === 'brew') {
    return [
      `$ brew install node`,
      `==> Downloading https://ghcr.io/v2/homebrew/core/node/...`,
      `==> Pouring node--20.11.1.arm64_sonoma.bottle.tar.gz`,
      `🍺  /opt/homebrew/Cellar/node/20.11.1: 2,000 files`,
      `验证: node -v → v20.11.1`,
      `验证: npm -v → 10.2.4`,
      `✓ Node.js + npm 环境就绪(mock)`,
    ];
  }
  if (id === 'git' && channel === 'brew') {
    return [
      `$ brew install git`,
      `==> Downloading https://ghcr.io/v2/homebrew/core/git/...`,
      `==> Pouring git--2.43.0.arm64_sonoma.bottle.tar.gz`,
      `🍺  /opt/homebrew/Cellar/git/2.43.0: 1,500 files`,
      `验证: git --version → git version 2.43.0`,
      `✓ Git 环境就绪(mock)`,
    ];
  }
  if (channel === 'manual') {
    const command =
      id === 'git'
        ? 'sudo apt-get install -y git'
        : 'sudo apt-get install -y nodejs npm';
    return [
      `# Linux has no one-click ${meta.name} installer`,
      `remediation: ${command}`,
      `remediation url: ${id === 'git' ? 'https://git-scm.com/downloads' : 'https://nodejs.org/'}`,
      `Install with your distro package manager or the official download, then restart AgentHub.`,
    ];
  }
  return [
    `$ agenthub env install ${id} --channel ${channel}`,
    `正在一键安装 ${meta.name}...`,
    `✓ 完成(mock)`,
  ];
}

export function createMockEnvPort(_backend: Backend): EnvPort {
  return {
    async listRuntimes() {
      await delay(randomLatency(180, 320));
      const state = readState();
      const hostRuntimes = runtimesForPlatform(detectHostPlatform());
      return hostRuntimes.map((m) => toDetect(m.id, state[m.id]));
    },

    async getRuntime(id) {
      await delay(randomLatency(180, 320));
      const state = readState();
      return toDetect(id, state[id]);
    },

    async installRuntimeDetailed(id, channel = defaultChannel()) {
      if (channel === 'manual') {
        await delay(randomLatency(80, 160));
        return {
          ok: false,
          action: 'install_runtime',
          logs: mockEnvInstallLogs(id, channel),
          message:
            'Linux 不提供一键包管理安装。请用发行版包管理器或官网安装后，完全退出并重启 AgentHub 再检测。',
          code: 'env.not_ready',
          details: {
            agent: null,
            channel: 'manual',
            missing: [id === 'npm' ? 'nodejs' : id],
          },
        };
      }
      await this.installRuntime(id, channel);
      return {
        ok: true,
        action: 'install_runtime',
        logs: mockEnvInstallLogs(id, channel),
        message: 'mock ok',
      };
    },

    async installRuntime(id, channel = defaultChannel()) {
      await delay(randomLatency());
      const state = readState();
      const meta = RUNTIME_MAP[id];
      const platform = detectHostPlatform();
      if (channel === 'manual') {
        throw new Error(
          'Linux 不提供一键包管理安装,请按修复步骤用发行版包管理器或官网安装后重新检测',
        );
      }

      if (id === 'nodejs' || id === 'npm') {
        if (!RUNTIME_MAP.nodejs.canAutoInstall) {
          throw new Error('Node.js 不支持一键安装,请按修复步骤手动处理');
        }
        state.nodejs = mockInstalledRuntime('nodejs', platform);
        state.npm = mockInstalledRuntime('npm', platform);
        writeState(state);
        return toDetect(id, state[id]);
      }

      if (id === 'git') {
        if (!meta.canAutoInstall) {
          throw new Error('Git 不支持一键安装,请按修复步骤手动处理');
        }
        state.git = mockInstalledRuntime('git', platform);
        writeState(state);
        return toDetect(id, state.git);
      }

      if (!meta.canAutoInstall) {
        throw new Error(`${meta.name} 不支持一键安装,请按修复步骤手动处理`);
      }

      state[id] = {
        status: 'ok',
        version: '1.0.0',
        path: platform === 'windows' ? `C:\\Tools\\${id}.exe` : `/usr/local/bin/${id}`,
      };
      writeState(state);
      return toDetect(id, state[id]);
    },

    async installRuntimesBatch(targets, channel = defaultChannel()) {
      const results: RuntimeDetect[] = [];
      const ordered = [...new Set(targets)].sort((a, b) => {
        if (a === 'nodejs') return -1;
        if (b === 'nodejs') return 1;
        return 0;
      });
      for (const id of ordered) {
        results.push(await this.installRuntime(id, channel));
      }
      return results;
    },
  };
}

/** Mock-only demo controls — not part of production EnvPort. */
export async function simulateBrokenPath(id: RuntimeId = 'nodejs'): Promise<void> {
  await delay(100);
  const state = readState();
  state[id] = {
    status: 'broken_path',
    version: state[id]?.version ?? '20.11.1',
  };
  writeState(state);
}

export async function resetRuntimesDemo(): Promise<void> {
  await delay(100);
  writeState(defaultState());
}
