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

/** Per-row action on the Agents environment list. */
export function envSoftwareAction(
  runtime: RuntimeDetect,
  runtimes: RuntimeDetect[],
  platform: HostPlatform = detectHostPlatform(),
  update?: RuntimeUpdateInfo,
): EnvSoftwareAction | null {
  const canInstall = envSoftwareCanAuto(runtimes, runtime, platform, false);
  const canUpgrade = envSoftwareCanAuto(runtimes, runtime, platform, true);

  switch (runtime.status) {
    case 'missing':
      return canInstall ? 'install' : 'repair';
    case 'outdated':
      return canUpgrade ? 'upgrade' : 'repair';
    case 'broken_path':
      return 'repair';
    case 'ok':
      if (update?.state === 'update_available') return 'upgrade';
      return null;
  }
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
