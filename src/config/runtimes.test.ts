import { describe, expect, it } from 'vitest';
import {
  RUNTIME_MAP,
  runtimesForPlatform,
  runtimeRemediationsForPlatform,
} from './runtimes';

describe('runtime remediation platform filtering', () => {
  it('offers Homebrew and official links on macOS without winget', () => {
    const rows = runtimeRemediationsForPlatform(RUNTIME_MAP.nodejs.remediations, 'macos');
    expect(rows.some((row) => row.kind === 'brew' && row.value === 'brew install node')).toBe(true);
    expect(rows.some((row) => row.kind === 'url' && row.value.includes('nodejs.org'))).toBe(true);
    expect(rows.some((row) => row.kind === 'winget')).toBe(false);
  });

  it('keeps winget only on Windows', () => {
    const rows = runtimeRemediationsForPlatform(RUNTIME_MAP.nodejs.remediations, 'windows');
    expect(rows.some((row) => row.kind === 'winget')).toBe(true);
    expect(rows.some((row) => row.kind === 'brew')).toBe(false);
  });

  it('filters package-manager rows when the host is unknown', () => {
    const rows = runtimeRemediationsForPlatform(RUNTIME_MAP.git.remediations, 'unknown');
    expect(rows.some((row) => row.kind === 'winget' || row.kind === 'brew')).toBe(false);
    expect(rows.some((row) => row.kind === 'url')).toBe(true);
  });

  it('offers copyable distro commands on Linux without winget or brew', () => {
    const rows = runtimeRemediationsForPlatform(RUNTIME_MAP.nodejs.remediations, 'linux');
    expect(rows.some((row) => row.kind === 'command' && row.value.includes('apt-get'))).toBe(true);
    expect(rows.some((row) => row.kind === 'command' && row.value.includes('zypper'))).toBe(true);
    expect(rows.some((row) => row.kind === 'command' && row.value.includes('apk add'))).toBe(true);
    expect(rows.some((row) => row.kind === 'hint' && row.value.includes('不要套用 apt-get'))).toBe(
      true,
    );
    expect(rows.some((row) => row.kind === 'url' && row.value.includes('nodejs.org'))).toBe(true);
    expect(rows.some((row) => row.kind === 'winget' || row.kind === 'brew')).toBe(false);
  });

  it('never offers apt commands on Windows or macOS', () => {
    for (const platform of ['windows', 'macos'] as const) {
      const rows = runtimeRemediationsForPlatform(RUNTIME_MAP.nodejs.remediations, platform);
      expect(rows.some((row) => (row.value ?? '').includes('apt-get'))).toBe(false);
      expect(rows.some((row) => (row.value ?? '').includes('zypper'))).toBe(false);
    }
  });

  it('omits PowerShell from host runtime list outside Windows', () => {
    expect(runtimesForPlatform('macos').map((r) => r.id)).not.toContain('powershell');
    expect(runtimesForPlatform('linux').map((r) => r.id)).not.toContain('powershell');
    expect(runtimesForPlatform('windows').map((r) => r.id)).toContain('powershell');
  });
});
