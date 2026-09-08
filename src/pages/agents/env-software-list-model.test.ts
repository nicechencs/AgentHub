import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import type { RuntimeDetect } from '@/lib/types';
import {
  envSoftwareAction,
  envSoftwareActionLabel,
  envSoftwareColumnLabel,
  envSoftwareControl,
  envSoftwareListOpenByDefault,
  envSoftwareStatusLabel,
  envSoftwareUpgradeTitle,
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
  it('collapses the environment list unless install or PATH still needs a fix', () => {
    expect(envSoftwareListOpenByDefault([])).toBe(false);
    expect(
      envSoftwareListOpenByDefault([
        runtime('nodejs', 'ok'),
        runtime('git', 'ok'),
      ]),
    ).toBe(false);
    expect(envSoftwareListOpenByDefault([runtime('git', 'outdated')])).toBe(false);
    expect(
      envSoftwareListOpenByDefault([
        runtime('nodejs', 'ok'),
        runtime('git', 'missing'),
      ]),
    ).toBe(true);
    expect(envSoftwareListOpenByDefault([runtime('git', 'broken_path')])).toBe(true);
  });

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

  it('installs missing Node/Git on macOS and keeps a gray force-upgrade on ready rows', () => {
    const missing = [runtime('nodejs', 'missing'), runtime('npm', 'missing'), runtime('git', 'missing')];
    expect(envSoftwareAction(missing[0], missing, 'macos')).toBe('install');
    expect(envSoftwareAction(missing[2], missing, 'macos')).toBe('install');

    const ready = [
      runtime('nodejs', 'ok', { version: '20.11.1' }),
      runtime('npm', 'ok', { version: '10.2.4' }),
      runtime('git', 'ok', { version: '2.43.0' }),
    ];
    expect(envSoftwareControl(ready[0], ready, 'macos')).toEqual({
      action: 'upgrade',
      muted: false,
      kind: 'in_app',
      upgradable: false,
    });
    expect(envSoftwareControl(ready[2], ready, 'macos')).toEqual({
      action: 'upgrade',
      muted: false,
      kind: 'in_app',
      upgradable: false,
    });
    expect(envSoftwareControl(ready[0], ready, 'macos', {
      runtimeId: 'nodejs', state: 'update_available', latestVersion: '24.20.0',
    })).toMatchObject({ action: 'upgrade', muted: false, kind: 'in_app', upgradable: true });
    expect(envSoftwareControl(ready[2], ready, 'macos', {
      runtimeId: 'git', state: 'update_available', latestVersion: '2.55.0',
    })).toMatchObject({ action: 'upgrade', muted: false, kind: 'in_app', upgradable: true });
  });

  it('repairs PATH issues and Linux missing packages; PowerShell has no upgrade', () => {
    const broken = [runtime('nodejs', 'broken_path', { path: '/usr/bin/node' })];
    expect(envSoftwareAction(broken[0], broken, 'macos')).toBe('repair');

    const linuxMissing = [runtime('nodejs', 'missing'), runtime('git', 'missing')];
    expect(envSoftwareAction(linuxMissing[0], linuxMissing, 'linux')).toBe('repair');
    expect(envSoftwareAction(linuxMissing[1], linuxMissing, 'linux')).toBe('repair');

    const linuxReady = [runtime('nodejs', 'ok', { version: '20.11.1' }), runtime('git', 'ok', { version: '2.43.0' })];
    expect(envSoftwareControl(linuxReady[0], linuxReady, 'linux')).toEqual({
      action: 'upgrade',
      muted: true,
      kind: 'hint_only',
      upgradable: false,
    });

    const ps = [runtime('powershell', 'ok', { version: '5.1' })];
    expect(envSoftwareControl(ps[0], ps, 'windows')).toEqual({
      action: 'upgrade',
      muted: true,
      kind: 'hint_only',
      upgradable: false,
    });
    expect(envSoftwareControl(ps[0], ps, 'windows', {
      runtimeId: 'powershell',
      state: 'up_to_date',
      setupUrl: 'https://learn.microsoft.com/powershell',
      canAutoUpgrade: false,
    })).toEqual({
      action: 'upgrade',
      muted: true,
      kind: 'open_setup',
      upgradable: false,
    });
    expect(envSoftwareAction(runtime('powershell', 'missing'), [runtime('powershell', 'missing')], 'windows')).toBe(
      'repair',
    );
  });

  it('upgrades outdated Node but does not present npm as a Node.js upgrade', () => {
    const outdated = [
      runtime('nodejs', 'outdated', { version: '16.0.0' }),
      runtime('npm', 'ok', { version: '8.0.0' }),
    ];
    expect(envSoftwareControl(outdated[0], outdated, 'windows')).toMatchObject({
      action: 'upgrade',
      muted: false,
      kind: 'in_app',
      upgradable: true,
    });
    expect(envSoftwareControl(outdated[1], outdated, 'windows')).toMatchObject({
      action: 'upgrade',
      muted: false,
      kind: 'in_app',
      upgradable: false,
    });
  });

  it('uses Agent-style force-upgrade copy when no newer version is available', () => {
    const t = createTranslator('zh');
    const ready = envSoftwareControl(
      runtime('nodejs', 'ok', { version: '20.11.1' }),
      [runtime('nodejs', 'ok', { version: '20.11.1' })],
      'macos',
      { runtimeId: 'nodejs', state: 'up_to_date', latestVersion: '20.11.1' },
    );
    expect(envSoftwareUpgradeTitle(ready, t, {
      runtimeId: 'nodejs',
      state: 'up_to_date',
      latestVersion: '20.11.1',
    })).toBe('已是最新 20.11.1 · 点击可强制升级');
    expect(envSoftwareUpgradeTitle(ready, t, { runtimeId: 'nodejs', state: 'unknown' })).toBe(
      '未能检测更新 · 点击可强制升级',
    );
  });
});
