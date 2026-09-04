import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import type { RuntimeDetect } from '@/lib/types';
import {
  envSoftwareAction,
  envSoftwareActionLabel,
  envSoftwareColumnLabel,
  envSoftwareStatusLabel,
  envSoftwareVersion,
} from './env-software-list-model';

function runtime(
  id: RuntimeDetect['id'],
  status: RuntimeDetect['status'],
  extra: Partial<RuntimeDetect> = {},
): RuntimeDetect {
  return { id, status, remediations: [], ...extra };
}

describe('env software list model', () => {
  it('uses existing Agents / env words for headers', () => {
    const t = createTranslator('zh');
    expect(envSoftwareColumnLabel('software', t)).toBe('软件');
    expect(envSoftwareColumnLabel('status', t)).toBe('状态');
    expect(envSoftwareColumnLabel('version', t)).toBe('版本');
    expect(envSoftwareColumnLabel('note', t)).toBe('说明');
    expect(envSoftwareColumnLabel('actions', t)).toBe('操作');
    expect(envSoftwareActionLabel('install', t)).toBe('安装');
    expect(envSoftwareActionLabel('upgrade', t)).toBe('升级');
    expect(envSoftwareActionLabel('repair', t)).toBe('修复');
    expect(envSoftwareStatusLabel('ok', t)).toBe('就绪');
    expect(envSoftwareVersion(runtime('git', 'missing'))).toBe('—');
    expect(envSoftwareVersion(runtime('git', 'ok', { version: '2.43.0' }))).toBe('2.43.0');
  });

  it('installs missing Node/Git on macOS and only exposes ready upgrades after a remote match', () => {
    const missing = [runtime('nodejs', 'missing'), runtime('npm', 'missing'), runtime('git', 'missing')];
    expect(envSoftwareAction(missing[0], missing, 'macos')).toBe('install');
    expect(envSoftwareAction(missing[2], missing, 'macos')).toBe('install');

    const ready = [
      runtime('nodejs', 'ok', { version: '20.11.1' }),
      runtime('npm', 'ok', { version: '10.2.4' }),
      runtime('git', 'ok', { version: '2.43.0' }),
    ];
    expect(envSoftwareAction(ready[0], ready, 'macos')).toBeNull();
    expect(envSoftwareAction(ready[2], ready, 'macos')).toBeNull();
    expect(envSoftwareAction(ready[0], ready, 'macos', {
      runtimeId: 'nodejs', state: 'update_available', latestVersion: '24.20.0',
    })).toBe('upgrade');
    expect(envSoftwareAction(ready[2], ready, 'macos', {
      runtimeId: 'git', state: 'update_available', latestVersion: '2.55.0',
    })).toBe('upgrade');
  });

  it('repairs PATH issues and Linux missing packages; PowerShell has no upgrade', () => {
    const broken = [runtime('nodejs', 'broken_path', { path: '/usr/bin/node' })];
    expect(envSoftwareAction(broken[0], broken, 'macos')).toBe('repair');

    const linuxMissing = [runtime('nodejs', 'missing'), runtime('git', 'missing')];
    expect(envSoftwareAction(linuxMissing[0], linuxMissing, 'linux')).toBe('repair');
    expect(envSoftwareAction(linuxMissing[1], linuxMissing, 'linux')).toBe('repair');

    const ps = [runtime('powershell', 'ok', { version: '5.1' })];
    expect(envSoftwareAction(ps[0], ps, 'windows')).toBeNull();
    expect(envSoftwareAction(runtime('powershell', 'missing'), [runtime('powershell', 'missing')], 'windows')).toBe(
      'repair',
    );
  });

  it('upgrades outdated Node but does not present npm as a Node.js upgrade', () => {
    const outdated = [
      runtime('nodejs', 'outdated', { version: '16.0.0' }),
      runtime('npm', 'ok', { version: '8.0.0' }),
    ];
    expect(envSoftwareAction(outdated[0], outdated, 'windows')).toBe('upgrade');
    expect(envSoftwareAction(outdated[1], outdated, 'windows')).toBeNull();
  });
});
