import { describe, expect, it } from 'vitest';
import {
  formatRuntimeInstallFailureLines,
  resolveAutoInstallPlan,
  runtimeChannelForPlan,
} from './env-plan';
import type { RuntimeDetect } from '@/lib/types';

function missing(
  id: RuntimeDetect['id'],
  remediations: RuntimeDetect['remediations'] = [],
): RuntimeDetect {
  return { id, status: 'missing', remediations };
}

describe('runtimeChannelForPlan', () => {
  it('uses brew on macOS', () => {
    expect(runtimeChannelForPlan('macos')).toBe('brew');
  });

  it('uses winget on Windows and manual remediations on Linux/unknown', () => {
    expect(runtimeChannelForPlan('windows')).toBe('winget');
    expect(runtimeChannelForPlan('linux')).toBe('manual');
    expect(runtimeChannelForPlan('unknown')).toBe('manual');
  });
});

describe('resolveAutoInstallPlan', () => {
  it('skips one-click targets on Linux', () => {
    const plan = resolveAutoInstallPlan([missing('nodejs'), missing('git')], undefined, 'linux');
    expect(plan.targets).toEqual([]);
    expect(plan.skipped).toEqual(['nodejs', 'git']);
  });

  it('keeps one-click Node/Git on Windows', () => {
    const plan = resolveAutoInstallPlan([missing('nodejs'), missing('git')], undefined, 'windows');
    expect(plan.targets).toEqual(['nodejs', 'git']);
    expect(plan.skipped).toEqual([]);
  });

  it('keeps one-click Node/Git on macOS when remediations are empty', () => {
    const plan = resolveAutoInstallPlan([missing('nodejs'), missing('git')], undefined, 'macos');
    expect(plan.targets).toEqual(['nodejs', 'git']);
    expect(plan.skipped).toEqual([]);
  });

  it('keeps one-click Node/Git on macOS when doctor still offers Homebrew', () => {
    const brew = { kind: 'brew' as const, value: 'brew install node' };
    const plan = resolveAutoInstallPlan(
      [missing('nodejs', [brew]), missing('git', [{ kind: 'brew', value: 'brew install git' }])],
      undefined,
      'macos',
    );
    expect(plan.targets).toEqual(['nodejs', 'git']);
    expect(plan.skipped).toEqual([]);
  });

  it('skips one-click on macOS when Homebrew is missing', () => {
    const manual = [
      { kind: 'url' as const, value: 'https://nodejs.org/' },
      { kind: 'hint' as const, value: '未找到 Homebrew，无法一键安装' },
    ];
    const plan = resolveAutoInstallPlan(
      [missing('nodejs', manual), missing('git', [{ kind: 'url', value: 'https://git-scm.com/downloads' }])],
      undefined,
      'macos',
    );
    expect(plan.targets).toEqual([]);
    expect(plan.skipped).toEqual(['nodejs', 'git']);
  });

  it('skips one-click on macOS when steps include the Homebrew installer', () => {
    const plan = resolveAutoInstallPlan(
      [
        missing('nodejs', [
          { kind: 'command', value: '/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"' },
          { kind: 'url', value: 'https://brew.sh/' },
        ]),
      ],
      undefined,
      'macos',
    );
    expect(plan.targets).toEqual([]);
    expect(plan.skipped).toEqual(['nodejs']);
  });
});

describe('formatRuntimeInstallFailureLines', () => {
  it('shows Homebrew/official steps instead of internal brew logs', () => {
    const lines = formatRuntimeInstallFailureLines({
      ok: false,
      action: 'env_install',
      logs: [
        '# install runtime nodejs via brew (node)',
        'not found: command not found: brew (install Homebrew from https://brew.sh/)',
        'remediation: /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"',
      ],
      message: '未找到 Homebrew，无法一键安装。请先安装 Homebrew（https://brew.sh/），或从官网手动安装。完成后完全退出并重启 AgentHub 再检测。',
      code: 'env.not_ready',
      details: {
        hint: '未找到 Homebrew，无法一键安装。请先安装 Homebrew（https://brew.sh/），或从官网手动安装。完成后完全退出并重启 AgentHub 再检测。',
        remediations: [
          {
            command:
              '/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"',
            url: 'https://nodejs.org/',
            text: '未找到 Homebrew，无法一键安装 Node.js。请先安装 Homebrew，或打开官网手动安装。',
          },
          { url: 'https://brew.sh/', text: '安装 Homebrew 后，完全退出并重启 AgentHub，即可使用一键安装。' },
        ],
      },
    });
    expect(lines[0]).toContain('未找到 Homebrew');
    expect(lines.join('\n')).toContain('https://nodejs.org/');
    expect(lines.join('\n')).toContain('https://brew.sh/');
    expect(lines.join('\n')).toContain('Homebrew/install');
    expect(lines.join('\n')).not.toContain('# install runtime');
    expect(lines.join('\n')).not.toContain('command not found: brew');
    expect(lines.join('\n')).not.toContain('remediation:');
  });
});
