import { describe, expect, it } from 'vitest';
import type { PluginEntry } from '@/lib/backend/contracts/plugin-types';
import {
  isPinnedNpmVersion,
  pluginVersionView,
  pluginVersionsMatch,
} from './plugin-version-model';

function pack(overrides: Partial<PluginEntry> & Pick<PluginEntry, 'agent' | 'name'>): PluginEntry {
  return {
    id: `${overrides.agent}:${overrides.name}`,
    source: 'live',
    components: [],
    ...overrides,
  };
}

describe('plugin version judgment', () => {
  it('treats x.y / x.y.z as a Pi npm pin, not latest or a range', () => {
    expect(isPinnedNpmVersion('1.2.3')).toBe(true);
    expect(isPinnedNpmVersion('v0.64.0')).toBe(true);
    expect(isPinnedNpmVersion('1.2.3-beta.1')).toBe(true);
    expect(isPinnedNpmVersion('latest')).toBe(false);
    expect(isPinnedNpmVersion('^1.2.3')).toBe(false);
    expect(isPinnedNpmVersion('v1')).toBe(false);
  });

  it('matches versions ignoring a leading v, but not a different prerelease', () => {
    expect(pluginVersionsMatch('0.64.0', 'v0.64.0')).toBe(true);
    expect(pluginVersionsMatch('1.2', '1.2.0')).toBe(true);
    expect(pluginVersionsMatch('0.64.0', '0.70.0')).toBe(false);
    expect(pluginVersionsMatch('1.2.3', '1.2.3-beta.1')).toBe(false);
  });

  it('does not claim Claude/Grok packs need a Pi upgrade check', () => {
    const view = pluginVersionView(
      pack({
        agent: 'claude',
        name: 'demo',
        version: '1.2.0',
        path: '~/.claude/plugins/cache/demo/1.2.0',
      }),
    );
    expect(view.kind).toBe('current');
    expect(view.versionLabel).toBe('1.2.0');
    expect(view.listBadge).toBeNull();
    expect(view.hintKey).toBeNull();
  });

  it('marks a live Claude or Grok pack without an install path as not installed', () => {
    const claude = pluginVersionView(
      pack({
        agent: 'claude',
        name: 'pack',
        marketplace: 'official',
        source: 'live',
      }),
    );
    expect(claude.kind).toBe('missing');
    expect(claude.listBadge).toBe('notInstalled');
    expect(claude.hintKey).toBe('plugins.detail.versionHintMissing');

    const grok = pluginVersionView(
      pack({
        agent: 'grok',
        name: 'gdrive',
        source: 'live',
      }),
    );
    expect(grok.listBadge).toBe('notInstalled');
  });

  it('does not treat a CLI-listed Claude pack without a path as missing', () => {
    const view = pluginVersionView(
      pack({
        agent: 'claude',
        name: 'demo',
        version: '1.2.0',
        source: 'cli',
      }),
    );
    expect(view.kind).toBe('current');
    expect(view.versionLabel).toBe('1.2.0');
    expect(view.listBadge).toBeNull();
  });

  it('treats an unpinned Pi npm pack as installed, not upgradable on this page', () => {
    const view = pluginVersionView(
      pack({
        agent: 'pi',
        name: 'pi-subagents',
        marketplace: 'npm',
        version: '0.64.0',
        path: '~/.pi/agent/npm/node_modules/pi-subagents',
      }),
    );
    expect(view.kind).toBe('current');
    expect(view.versionLabel).toBe('0.64.0');
    expect(view.listBadge).toBeNull();
    expect(view.hintKey).toBe('plugins.detail.versionHintUnpinned');
  });

  it('marks a matching Pi npm pin as specified, not a failure', () => {
    const view = pluginVersionView(
      pack({
        agent: 'pi',
        name: 'pi-subagents',
        marketplace: 'npm',
        version: '0.70.0',
        requestedVersion: '0.70.0',
        path: '~/.pi/agent/npm/node_modules/pi-subagents',
      }),
    );
    expect(view.kind).toBe('pinned');
    expect(view.listBadge).toBeNull();
    expect(view.hintKey).toBe('plugins.detail.versionHintPinned');
  });

  it('flags when the on-disk Pi npm version differs from the specified version', () => {
    const view = pluginVersionView(
      pack({
        agent: 'pi',
        name: 'pi-subagents',
        marketplace: 'npm',
        version: '0.64.0',
        requestedVersion: '0.70.0',
        path: '~/.pi/agent/npm/node_modules/pi-subagents',
      }),
    );
    expect(view.kind).toBe('mismatch');
    expect(view.installed).toBe('0.64.0');
    expect(view.requested).toBe('0.70.0');
    expect(view.listBadge).toBe('versionMismatch');
    expect(view.hintKey).toBe('plugins.detail.versionHintMismatch');
  });

  it('does not treat a Pi npm pin as installed when the directory has no version', () => {
    const view = pluginVersionView(
      pack({
        agent: 'pi',
        name: 'pi-subagents',
        marketplace: 'npm',
        requestedVersion: '0.70.0',
        path: '~/.pi/agent/npm/node_modules/pi-subagents',
      }),
    );
    expect(view.kind).toBe('mismatch');
    expect(view.versionLabel).toBeNull();
    expect(view.listBadge).toBe('versionMismatch');
  });

  it('flags a Pi pack listed in settings but missing on disk', () => {
    const view = pluginVersionView(
      pack({
        agent: 'pi',
        name: '@scope/other',
        marketplace: 'npm',
        version: '1.2.3',
        requestedVersion: '1.2.3',
      }),
    );
    expect(view.kind).toBe('missing');
    expect(view.versionLabel).toBeNull();
    expect(view.listBadge).toBe('notInstalled');
    expect(view.hintKey).toBe('plugins.detail.versionHintMissing');
  });

  it('does not treat a git ref as an npm version mismatch', () => {
    const view = pluginVersionView(
      pack({
        agent: 'pi',
        name: 'repo',
        marketplace: 'git',
        version: '9.0.0',
        requestedVersion: 'v1',
        path: '~/.pi/agent/git/github.com/user/repo',
      }),
    );
    expect(view.kind).toBe('git');
    expect(view.versionLabel).toBe('9.0.0');
    expect(view.listBadge).toBeNull();
    expect(view.hintKey).toBe('plugins.detail.versionHintGit');
  });

  it('treats a local Pi pack as local, not an npm pin', () => {
    const view = pluginVersionView(
      pack({
        agent: 'pi',
        name: 'local-ext',
        marketplace: 'local',
        version: '0.1.0',
        path: '~/.pi/agent/local-ext',
      }),
    );
    expect(view.kind).toBe('local');
    expect(view.hintKey).toBe('plugins.detail.versionHintLocal');
    expect(view.listBadge).toBeNull();
  });
});
