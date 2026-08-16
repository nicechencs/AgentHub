import { afterEach, describe, expect, it, vi } from 'vitest';
import { pickDirectory } from '@/lib/api/settings';
import { createMockSettingsPort, promptForDirectoryPath } from './settings';

describe('promptForDirectoryPath', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('trims a typed path and forwards title plus default', () => {
    const prompt = vi.fn(() => '  /tmp/demo  ');
    vi.stubGlobal('window', { prompt });
    expect(promptForDirectoryPath('选择工作目录', '/Users/me')).toBe('/tmp/demo');
    expect(prompt).toHaveBeenCalledWith('选择工作目录', '/Users/me');
  });

  it('uses empty default and the fallback title when omitted', () => {
    const prompt = vi.fn(() => '/tmp/x');
    vi.stubGlobal('window', { prompt });
    expect(promptForDirectoryPath()).toBe('/tmp/x');
    expect(prompt).toHaveBeenCalledWith('选择工作目录（输入完整路径）', '');
  });

  it('returns null when cancelled or blank', () => {
    vi.stubGlobal('window', { prompt: () => null });
    expect(promptForDirectoryPath()).toBeNull();
    vi.stubGlobal('window', { prompt: () => '   ' });
    expect(promptForDirectoryPath()).toBeNull();
  });

  it('returns null when prompt is unavailable', () => {
    vi.stubGlobal('window', {});
    expect(promptForDirectoryPath()).toBeNull();
  });
});

describe('mock SettingsPort.pickDirectory', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('exposes a typed path through the façade', async () => {
    const prompt = vi.fn(() => '/Users/me/work');
    vi.stubGlobal('window', { prompt });
    const port = createMockSettingsPort();
    await expect(port.pickDirectory({ title: '选择工作目录', defaultPath: '/tmp' })).resolves.toBe(
      '/Users/me/work',
    );
    expect(prompt).toHaveBeenCalledWith('选择工作目录', '/tmp');
    await expect(pickDirectory({ defaultPath: '/tmp' })).resolves.toBe('/Users/me/work');
  });
});
