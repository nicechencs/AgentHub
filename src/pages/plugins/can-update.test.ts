import { describe, expect, it } from 'vitest';
import {
  canRefreshPluginMarketplace,
  canUpdateAllPlugins,
  canUpdateListedPlugin,
} from './can-update';

describe('plugin update actions', () => {
  it('keeps marketplace refresh separate from package updates', () => {
    expect(canRefreshPluginMarketplace('claude')).toBe(true);
    expect(canRefreshPluginMarketplace('grok')).toBe(true);
    expect(canRefreshPluginMarketplace('pi')).toBe(false);
  });

  it('uses per-pack updates for Claude/Grok and one all-pack action for Pi', () => {
    expect(canUpdateListedPlugin('claude', 'user')).toBe(true);
    expect(canUpdateListedPlugin('grok', 'user')).toBe(true);
    expect(canUpdateListedPlugin('claude', 'project')).toBe(false);
    expect(canUpdateListedPlugin('grok', null)).toBe(false);
    expect(canUpdateListedPlugin('pi', 'user')).toBe(false);
    expect(canUpdateAllPlugins('pi')).toBe(true);
    expect(canUpdateAllPlugins('codex')).toBe(false);
  });
});
