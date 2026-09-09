import { describe, expect, it } from 'vitest';
import { canInstallListedPlugin, canUninstallListedPlugin } from './can-install';

describe('canInstallListedPlugin', () => {
  it('allows Claude and Grok only', () => {
    expect(canInstallListedPlugin('claude')).toBe(true);
    expect(canInstallListedPlugin('grok')).toBe(true);
    expect(canUninstallListedPlugin('grok')).toBe(true);
  });

  it('hides install for planned and unsupported agents', () => {
    expect(canInstallListedPlugin('codex')).toBe(false);
    expect(canInstallListedPlugin('pi')).toBe(false);
    expect(canUninstallListedPlugin('cursor')).toBe(false);
    expect(canInstallListedPlugin('dsh')).toBe(false);
  });
});
