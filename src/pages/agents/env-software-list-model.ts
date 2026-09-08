/**
 * Agents-page environment software list: which row action to show.
 */
import { RUNTIME_MAP, runtimeDescriptionKey } from '@/config/runtimes';
import { resolveAutoInstallPlan } from '@/lib/env-plan';
import type { TranslateFn } from '@/lib/i18n';
import {
  detectHostPlatform,
  type HostPlatform,
} from '@/lib/platform-detect';
import type { EnvStatus, RuntimeDetect, RuntimeUpdateInfo } from '@/lib/types';

export type EnvSoftwareAction = 'install' | 'upgrade' | 'repair';

export type EnvSoftwareColumnKey = 'software' | 'status' | 'version' | 'note' | 'actions';

/** Agents 页运行环境：有待修项才默认展开，全部就绪则收起。 */
export function envSoftwareListOpenByDefault(
  runtimes: readonly Pick<RuntimeDetect, 'status'>[],
): boolean {
  return runtimes.some((runtime) => runtime.status !== 'ok');
}

export const ENV_SOFTWARE_FLEX_COLUMN: EnvSoftwareColumnKey = 'note';

export function envSoftwareColumnLabel(
  key: EnvSoftwareColumnKey,
  t: TranslateFn,
): string {
  switch (key) {
    case 'software':
      return t('chrome.env.software');
    case 'status':
      return t('agents.table.status');
    case 'version':
      return t('agents.table.version');
    case 'note':
      return t('agents.table.note');
    case 'actions':
      return t('agents.table.actions');
  }
}

export function envSoftwareStatusLabel(status: EnvStatus, t: TranslateFn): string {
  switch (status) {
    case 'ok':
      return t('chrome.env.statusOk');
    case 'outdated':
      return t('chrome.env.statusOutdated');
    case 'broken_path':
      return t('chrome.env.statusBrokenPath');
    case 'missing':
      return t('chrome.env.statusMissing');
  }
}

export function envSoftwareVersion(runtime: RuntimeDetect): string {
  return runtime.version?.trim() ? runtime.version : '—';
}

export function envSoftwareNoteKey(id: RuntimeDetect['id']): ReturnType<typeof runtimeDescriptionKey> {
  return runtimeDescriptionKey(id);
}

export function envSoftwareCanAuto(
  runtimes: RuntimeDetect[],
  runtime: RuntimeDetect,
  platform: HostPlatform,
  includeReady: boolean,
): boolean {
  const plan = resolveAutoInstallPlan(runtimes, [runtime.id], platform, includeReady);
  return plan.targets.length > 0;
}

export type EnvSoftwareUpgradeKind = 'in_app' | 'open_setup' | 'hint_only';

export type EnvSoftwareControl = {
  action: EnvSoftwareAction;
  /** Not an in-app upgrade — gray the button. */
  muted: boolean;
  kind: EnvSoftwareUpgradeKind;
  /** Green arrow: a newer version is available. */
  upgradable: boolean;
};

function hasHttpsUrl(value?: string): boolean {
  const url = value?.trim();
  return Boolean(url && /^https:\/\//i.test(url));
}

function envSoftwareCanAutoUpgrade(
  canUpgrade: boolean,
  update?: RuntimeUpdateInfo,
): boolean {
  if (update?.canAutoUpgrade === false) return false;
  if (update?.canAutoUpgrade === true) return true;
  return canUpgrade;
}

/** Per-row action on the Agents environment list. */
export function envSoftwareControl(
  runtime: RuntimeDetect,
  runtimes: RuntimeDetect[],
  platform: HostPlatform = detectHostPlatform(),
  update?: RuntimeUpdateInfo,
): EnvSoftwareControl {
  const canInstall = envSoftwareCanAuto(runtimes, runtime, platform, false);
  const canUpgrade = envSoftwareCanAuto(runtimes, runtime, platform, true);
  const kind = hasHttpsUrl(update?.setupUrl) ? 'open_setup' : 'hint_only';

  switch (runtime.status) {
    case 'missing':
      return {
        action: canInstall ? 'install' : 'repair',
        muted: false,
        kind: 'in_app',
        upgradable: false,
      };
    case 'broken_path':
      return {
        action: 'repair',
        muted: false,
        kind: 'in_app',
        upgradable: false,
      };
    case 'outdated':
    case 'ok': {
      const auto = envSoftwareCanAutoUpgrade(canUpgrade, update);
      const upgradable =
        runtime.status === 'outdated' || update?.state === 'update_available';
      if (auto) {
        return {
          action: 'upgrade',
          muted: false,
          kind: 'in_app',
          upgradable,
        };
      }
      return {
        action: runtime.status === 'outdated' && !canUpgrade ? 'repair' : 'upgrade',
        muted: runtime.status !== 'outdated' || canUpgrade,
        kind: runtime.status === 'outdated' && !canUpgrade ? 'in_app' : kind,
        upgradable: false,
      };
    }
  }
}

/** Per-row action on the Agents environment list. */
export function envSoftwareAction(
  runtime: RuntimeDetect,
  runtimes: RuntimeDetect[],
  platform: HostPlatform = detectHostPlatform(),
  update?: RuntimeUpdateInfo,
): EnvSoftwareAction {
  return envSoftwareControl(runtime, runtimes, platform, update).action;
}

export function envSoftwareUpgradeTitle(
  control: EnvSoftwareControl,
  t: TranslateFn,
  update?: RuntimeUpdateInfo,
  checking = false,
): string {
  if (checking) return t('chrome.env.checkingUpdate');
  if (control.action !== 'upgrade') return envSoftwareActionLabel(control.action, t);
  if (control.muted) {
    const note = update?.note?.trim();
    if (control.kind === 'open_setup') {
      return t('chrome.env.clickOfficial', {
        note: note || t('chrome.env.manualUpdate'),
      });
    }
    return note || t('chrome.env.unsupportedUpgrade');
  }
  if (control.upgradable) {
    return update?.latestVersion
      ? t('chrome.env.updateAvailable', { version: update.latestVersion })
      : t('chrome.env.upgradeLatest');
  }
  if (update?.state === 'up_to_date') {
    return update.latestVersion
      ? t('chrome.env.latestForceVersion', { version: update.latestVersion })
      : t('chrome.env.latestForce');
  }
  if (update?.note) {
    return t('chrome.env.unknownForceNote', { note: update.note });
  }
  return t('chrome.env.unknownForce');
}

export function envSoftwareActionLabel(
  action: EnvSoftwareAction,
  t: TranslateFn,
): string {
  switch (action) {
    case 'install':
      return t('chrome.env.install');
    case 'upgrade':
      return t('chrome.env.upgrade');
    case 'repair':
      return t('chrome.env.repair');
  }
}

export function envSoftwareName(runtime: RuntimeDetect): string {
  return RUNTIME_MAP[runtime.id]?.name ?? runtime.id;
}
