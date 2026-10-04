import { describe, expect, it } from 'vitest';
import { canInstallListedPlugin, canUninstallListedPlugin } from './can-install';

describe('canInstallListedPlugin', () => {
  it('allows Claude, Codex, Grok, and Pi', () => {
    expect(canInstallListedPlugin('claude')).toBe(true);
    expect(canInstallListedPlugin('codex')).toBe(true);
    expect(canInstallListedPlugin('grok')).toBe(true);
    expect(canInstallListedPlugin('pi')).toBe(true);
    expect(canUninstallListedPlugin('grok')).toBe(true);
    expect(canUninstallListedPlugin('pi')).toBe(true);
  });

  it('hides install for unsupported agents', () => {
    expect(canUninstallListedPlugin('cursor')).toBe(false);
    expect(canInstallListedPlugin('dsh')).toBe(false);
  });
});
