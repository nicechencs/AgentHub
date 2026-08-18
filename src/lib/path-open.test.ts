import { describe, expect, it } from 'vitest';
import {
  decodeClaudeProjectDir,
  isAbsoluteFsPath,
  looksLikeClaudeEncodedDir,
  normalizeOpenPath,
  projectOpenCandidates,
  restoreProjectWorkspacePath,
  verifiedProjectWorkspacePath,
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

describe('decodeClaudeProjectDir / looksLikeClaudeEncodedDir', () => {
  it('restores Windows and Unix Claude encodings', () => {
    expect(decodeClaudeProjectDir('-C-Users-demo-app')).toBe('C:\\Users\\demo\\app');
    expect(decodeClaudeProjectDir('-Users-foo-bar')).toBe('/Users/foo/bar');
    expect(looksLikeClaudeEncodedDir('-C-Users-demo-app')).toBe(true);
    expect(looksLikeClaudeEncodedDir('-Users-foo-bar')).toBe(true);
    expect(looksLikeClaudeEncodedDir('sessions')).toBe(false);
    expect(looksLikeClaudeEncodedDir('d-demo-workspace-2026-AgentHub')).toBe(false);
    expect(looksLikeClaudeEncodedDir('-C')).toBe(false);
  });
});

describe('restoreProjectWorkspacePath', () => {
  it('prefers a format-valid actualPath', () => {
    expect(
      restoreProjectWorkspacePath({
        agentId: 'claude',
        actualPath: 'D:/work/app',
        relativePath: 'projects/-D-work-app',
        storagePath: 'C:\\Users\\demo\\.claude\\projects\\-D-work-app',
      }),
    ).toBe('D:\\work\\app');
  });

  it('restores a Claude encoded dir when actualPath is missing', () => {
    expect(
      restoreProjectWorkspacePath({
        agentId: 'claude',
        actualPath: null,
        relativePath: 'projects/-C-Users-demo-app',
        storagePath: 'C:\\Users\\demo\\.claude\\projects\\-C-Users-demo-app',
      }),
    ).toBe('C:\\Users\\demo\\app');
  });

  it('does not invent a workspace for non-Claude agents', () => {
    expect(
      restoreProjectWorkspacePath({
        agentId: 'kimi',
        actualPath: null,
        relativePath: 'sessions',
        storagePath: 'C:\\Users\\demo\\.kimi-code\\sessions',
      }),
    ).toBeNull();
  });

  it('rejects a restore that is not an absolute path', () => {
    expect(
      restoreProjectWorkspacePath({
        agentId: 'claude',
        actualPath: null,
        relativePath: 'projects/-C',
        storagePath: 'C:\\Users\\demo\\.claude\\projects\\-C',
      }),
    ).toBeNull();
  });
});

describe('verifiedProjectWorkspacePath', () => {
  it('only accepts a format-valid actualPath from the backend', () => {
    expect(
      verifiedProjectWorkspacePath({
        agentId: 'claude',
        actualPath: 'D:/work/app',
        relativePath: 'projects/-D-work-app',
        storagePath: 'C:\\Users\\demo\\.claude\\projects\\-D-work-app',
      }),
    ).toBe('D:\\work\\app');
  });

  it('does not treat a client-side Claude restore as verified', () => {
    expect(
      verifiedProjectWorkspacePath({
        agentId: 'claude',
        actualPath: null,
        relativePath: 'projects/-C-Users-demo-app',
        storagePath: 'C:\\Users\\demo\\.claude\\projects\\-C-Users-demo-app',
      }),
    ).toBeNull();
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

  it('does not open an unverified Claude restore, only the storage dir', () => {
    expect(
      projectOpenCandidates({
        agentId: 'claude',
        actualPath: null,
        relativePath: 'projects/-D-work-app',
        storagePath: 'C:\\Users\\demo\\.claude\\projects\\-D-work-app',
      }),
    ).toEqual(['C:\\Users\\demo\\.claude\\projects\\-D-work-app']);
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
