import { describe, expect, it } from 'vitest';
import {
  isAbsoluteFsPath,
  normalizeOpenPath,
  projectOpenCandidates,
} from './path-open';

describe('normalizeOpenPath', () => {
  it('accepts Windows backslash paths', () => {
    expect(normalizeOpenPath('D:\\demo_chen\\2026\\AgentHub')).toBe(
      'D:\\demo_chen\\2026\\AgentHub',
    );
  });

  it('normalizes Windows forward-slash paths', () => {
    expect(normalizeOpenPath('D:/demo_chen/2026/AgentHub')).toBe(
      'D:\\demo_chen\\2026\\AgentHub',
    );
  });

  it('strips cwd/ storage key prefix (new project path format)', () => {
    expect(normalizeOpenPath('cwd/D:/Users/demo/app')).toBe('D:\\Users\\demo\\app');
    expect(normalizeOpenPath('cwd/C:\\Users\\demo\\app')).toBe('C:\\Users\\demo\\app');
  });

  it('rejects relative, opaque, and ungrouped keys', () => {
    expect(normalizeOpenPath('projects/-C-Users-demo')).toBeNull();
    expect(normalizeOpenPath('dir/some-bucket')).toBeNull();
    expect(normalizeOpenPath('__ungrouped__')).toBeNull();
    expect(normalizeOpenPath('')).toBeNull();
    expect(normalizeOpenPath(null)).toBeNull();
    expect(normalizeOpenPath(undefined)).toBeNull();
  });

  it('accepts Unix absolute paths', () => {
    expect(normalizeOpenPath('/Users/demo/app')).toBe('/Users/demo/app');
  });
});

describe('isAbsoluteFsPath', () => {
  it('detects drive / unc / unix', () => {
    expect(isAbsoluteFsPath('C:\\a')).toBe(true);
    expect(isAbsoluteFsPath('c:/a')).toBe(true);
    expect(isAbsoluteFsPath('\\\\server\\share')).toBe(true);
    expect(isAbsoluteFsPath('/home/x')).toBe(true);
    expect(isAbsoluteFsPath('relative/path')).toBe(false);
  });
});

describe('projectOpenCandidates', () => {
  it('prefers actualPath then storagePath and de-dupes', () => {
    expect(
      projectOpenCandidates({
        actualPath: 'D:/work/app',
        storagePath: 'C:\\Users\\demo\\.claude\\projects\\-D-work-app',
      }),
    ).toEqual(['D:\\work\\app', 'C:\\Users\\demo\\.claude\\projects\\-D-work-app']);

    expect(
      projectOpenCandidates({
        actualPath: 'D:/work/app',
        storagePath: 'D:\\work\\app',
      }),
    ).toEqual(['D:\\work\\app']);
  });

  it('returns empty when neither path is openable', () => {
    expect(
      projectOpenCandidates({
        actualPath: 'dir/opaque',
        storagePath: null,
      }),
    ).toEqual([]);
  });

  it('falls back to storage when actual is missing or invalid', () => {
    expect(
      projectOpenCandidates({
        actualPath: null,
        storagePath: 'C:\\Users\\demo\\.grok\\sessions',
      }),
    ).toEqual(['C:\\Users\\demo\\.grok\\sessions']);

    expect(
      projectOpenCandidates({
        actualPath: 'cwd/relative-not-abs',
        storagePath: 'C:\\Users\\demo\\.codex\\sessions',
      }),
    ).toEqual(['C:\\Users\\demo\\.codex\\sessions']);
  });
});
