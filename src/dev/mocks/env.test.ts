import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createMockEnvPort, resetRuntimesDemo } from './env';
import type { Backend } from '@/lib/backend/contracts';

const backend = {} as Backend;

describe('mock env install platform paths', () => {
  beforeEach(async () => {
    await resetRuntimesDemo();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('defaults to brew on macOS and writes Homebrew-style paths', async () => {
    vi.stubGlobal('navigator', {
      platform: 'MacIntel',
      userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5)',
    });
    await resetRuntimesDemo();
    const env = createMockEnvPort(backend);

    const listed = await env.listRuntimes();
    expect(listed.map((item) => item.id)).toEqual(['nodejs', 'npm', 'git']);
    expect(listed.some((item) => item.id === 'powershell')).toBe(false);
    await expect(env.checkRuntimeUpdates(['nodejs'])).resolves.toMatchObject([
      { runtimeId: 'nodejs', state: 'not_installed' },
    ]);

    const detailed = await env.installRuntimeDetailed('nodejs');
    expect(detailed.logs.some((line) => line.includes('brew install node'))).toBe(true);
    expect(detailed.logs.some((line) => line.includes('winget'))).toBe(false);

    // Assert the install return value: node vitest has no durable localStorage.
    const node = await env.installRuntime('nodejs');
    const git = await env.installRuntime('git');
    expect(node.path).toBe('/opt/homebrew/bin/node');
    expect(git.path).toBe('/opt/homebrew/bin/git');
  });

  it('keeps winget paths on Windows mocks', async () => {
    vi.stubGlobal('navigator', {
      platform: 'Win32',
      userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)',
    });
    await resetRuntimesDemo();
    const env = createMockEnvPort(backend);

    const listed = await env.listRuntimes();
    expect(listed.some((item) => item.id === 'powershell')).toBe(true);

    const detailed = await env.installRuntimeDetailed('nodejs');
    expect(detailed.logs.some((line) => line.includes('winget install'))).toBe(true);
    const node = await env.installRuntime('nodejs');
    expect(node.path).toContain('Program Files');
  });

  it('does not one-click install on Linux mocks', async () => {
    vi.stubGlobal('navigator', {
      platform: 'Linux x86_64',
      userAgent: 'Mozilla/5.0 (X11; Linux x86_64)',
    });
    await resetRuntimesDemo();
    const env = createMockEnvPort(backend);

    const listed = await env.listRuntimes();
    expect(listed.map((item) => item.id)).toEqual(['nodejs', 'npm', 'git']);
    expect(listed.some((item) => item.id === 'powershell')).toBe(false);

    const detailed = await env.installRuntimeDetailed('nodejs');
    expect(detailed.ok).toBe(false);
    expect(detailed.code).toBe('env.not_ready');
    expect(detailed.logs.some((line) => line.includes('apt-get'))).toBe(true);
    expect(detailed.logs.some((line) => line.includes('winget'))).toBe(false);

    await expect(env.installRuntime('git')).rejects.toThrow(/Linux/);
  });
});
