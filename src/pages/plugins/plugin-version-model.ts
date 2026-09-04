import type { PluginEntry } from '@/lib/backend/contracts/plugin-types';
import type { MessageKey } from '@/lib/i18n';

export type PluginVersionKind = 'current' | 'pinned' | 'mismatch' | 'missing' | 'git' | 'local';

export type PluginListBadge = 'notInstalled' | 'versionMismatch';

export type PluginVersionView = {
  kind: PluginVersionKind;
  /** On-disk version when the install path exists. */
  installed: string | null;
  /** Spec pin / git ref when present. */
  requested: string | null;
  /** List row version; omitted when the pack is not on disk. */
  versionLabel: string | null;
  listBadge: PluginListBadge | null;
  hintKey: MessageKey | null;
};

const PINNED_NPM_VERSION =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)(?:\.(0|[1-9]\d*))?(?:[-+].+)?$/;

export function isPinnedNpmVersion(raw?: string | null): boolean {
  if (!raw?.trim()) return false;
  return PINNED_NPM_VERSION.test(raw.trim().replace(/^[vV]/, ''));
}

function normalizeVersion(raw: string): string {
  const token = raw.replace(/^[vV]/, '');
  const match = token.match(/^(\d+)\.(\d+)(?:\.(\d+))?(.*)$/);
  if (!match) return token;
  return `${match[1]}.${match[2]}.${match[3] ?? '0'}${match[4] ?? ''}`;
}

export function pluginVersionsMatch(left?: string | null, right?: string | null): boolean {
  const a = trimmed(left);
  const b = trimmed(right);
  if (!a || !b) return false;
  if (a === b) return true;
  return normalizeVersion(a) === normalizeVersion(b);
}

function trimmed(raw?: string | null): string | null {
  const value = raw?.trim() ?? '';
  return value ? value : null;
}

export function pluginVersionView(plugin: PluginEntry): PluginVersionView {
  const requested = trimmed(plugin.requestedVersion);
  const rawVersion = trimmed(plugin.version);
  const onDisk = Boolean(trimmed(plugin.path));
  const installed = onDisk ? rawVersion : null;

  if (plugin.agent !== 'pi') {
    // CLI list is already "installed"; live rows without a path are settings-only.
    if (!onDisk && plugin.source !== 'cli') {
      return {
        kind: 'missing',
        installed: null,
        requested: null,
        versionLabel: null,
        listBadge: 'notInstalled',
        hintKey: 'plugins.detail.versionHintMissing',
      };
    }
    return {
      kind: 'current',
      installed: rawVersion,
      requested: null,
      versionLabel: rawVersion,
      listBadge: null,
      hintKey: null,
    };
  }

  const marketplace = plugin.marketplace ?? '';

  if (marketplace === 'local') {
    return {
      kind: 'local',
      installed,
      requested: null,
      versionLabel: installed,
      listBadge: onDisk ? null : 'notInstalled',
      hintKey: onDisk ? 'plugins.detail.versionHintLocal' : 'plugins.detail.versionHintMissing',
    };
  }

  if (marketplace === 'git') {
    return {
      kind: 'git',
      installed,
      requested,
      versionLabel: installed,
      listBadge: onDisk ? null : 'notInstalled',
      hintKey: onDisk ? 'plugins.detail.versionHintGit' : 'plugins.detail.versionHintMissing',
    };
  }

  if (!onDisk) {
    return {
      kind: 'missing',
      installed: null,
      requested,
      versionLabel: null,
      listBadge: 'notInstalled',
      hintKey: 'plugins.detail.versionHintMissing',
    };
  }

  if (isPinnedNpmVersion(requested)) {
    if (installed && pluginVersionsMatch(installed, requested)) {
      return {
        kind: 'pinned',
        installed,
        requested,
        versionLabel: installed,
        listBadge: null,
        hintKey: 'plugins.detail.versionHintPinned',
      };
    }
    return {
      kind: 'mismatch',
      installed,
      requested,
      versionLabel: installed,
      listBadge: 'versionMismatch',
      hintKey: 'plugins.detail.versionHintMismatch',
    };
  }

  return {
    kind: 'current',
    installed,
    requested,
    versionLabel: installed,
    listBadge: null,
    hintKey: 'plugins.detail.versionHintUnpinned',
  };
}
