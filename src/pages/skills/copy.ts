/**
 * Skills copy helpers. Leaf strings live in `src/lib/i18n/locales`.
 * Pass `t` from useI18n / createTranslator; tests use createTranslator('zh').
 */
import type { SkillMapStatus, SkillSyncState } from '@/lib/types';
import type { TranslateFn } from '@/lib/i18n';
import type { LocalFilter } from './skills-preview-model';

export function privateSkillRowHint(t: TranslateFn): string {
  return t('skills.cell.privateSource');
}

export function skillCellTip(
  t: TranslateFn,
  agentName: string,
  state: SkillSyncState,
  mapStatus: SkillMapStatus,
  _linkKind?: string,
  reason?: string,
): string {
  switch (mapStatus) {
    case 'agent_unsupported':
      return reason
        ? t('skills.cell.unsupportedReason', { agentName, reason })
        : t('skills.cell.unsupported', { agentName });
    case 'agent_not_installed':
      return t('skills.cell.notInstalled', { agentName });
    case 'target_unavailable':
      return t('skills.cell.targetUnavailable', { agentName });
    case 'private_source':
      return t('skills.cell.privateSource');
    case 'conflict':
      if (state === 'foreign') return t('skills.cell.conflictForeign');
      if (state === 'conflict') return t('skills.cell.conflictUnknown');
      return t('skills.cell.conflictForeign');
    case 'available':
      break;
  }
  switch (state) {
    case 'linked':
      return t('skills.cell.linked');
    case 'copied':
      return t('skills.cell.copied');
    case 'absent':
      return t('skills.cell.absent');
    case 'unsupported':
      return t('skills.cell.unsupported', { agentName });
    default:
      return state;
  }
}

export function catalogFilters(t: TranslateFn): { id: LocalFilter; label: string }[] {
  return [
    { id: 'all', label: t('skills.filters.enableAll') },
    { id: 'private', label: t('skills.filters.enablePrivate') },
    { id: 'mapped', label: t('skills.filters.enableMapped') },
    { id: 'unmapped', label: t('skills.filters.enableUnmapped') },
    { id: 'conflict', label: t('skills.filters.enableConflict') },
  ];
}

export function sharedRootPresence(t: TranslateFn, inLibrary: boolean, label: string): string {
  return inLibrary
    ? t('skills.matrix.sharedRootIn', { label })
    : t('skills.matrix.sharedRootOut', { label });
}

export function marketSuffix(t: TranslateFn, isAuto: boolean): string {
  return isAuto ? t('skills.market.suffixAuto') : t('skills.market.suffixManual');
}

export function emptyPrivateDesc(t: TranslateFn, inLibrary: number): string {
  return inLibrary > 0
    ? t('skills.workspace.emptyPrivateDescSome', { inLibrary })
    : t('skills.workspace.emptyPrivateDescNone');
}

export function batchEnableToast(
  t: TranslateFn,
  enabled: number,
  failed: number,
  skipParts: string[],
): { title: string; description?: string } {
  return {
    title:
      enabled === 0 && failed === 0
        ? t('skills.toast.batchEnableNone')
        : t('skills.toast.batchEnableOk', { enabled }),
    description:
      skipParts.length > 0
        ? t('skills.toast.batchEnableSkip', { parts: skipParts.join(' · ') })
        : undefined,
  };
}

export function batchAdoptToast(
  t: TranslateFn,
  ok: number,
  conflict: number,
  failed: number,
): { title: string; description?: string } {
  return {
    title: t('skills.toast.batchAdoptTitle', { ok }),
    description:
      conflict > 0 || failed > 0
        ? t('skills.toast.batchAdoptPartial', { conflict, failed })
        : ok > 0
          ? t('skills.toast.adoptOkDesc')
          : undefined,
  };
}

export function enableFailedToast(
  t: TranslateFn,
  reason: string,
): { title: string; description?: string } {
  return { title: t('skills.toast.enableFailed'), description: reason };
}

export function disableFailedToast(
  t: TranslateFn,
  reason: string,
): { title: string; description?: string } {
  return { title: t('skills.toast.disableFailed'), description: reason };
}

export function enableOkToast(
  t: TranslateFn,
  agentName: string,
  skillName: string,
): { title: string; description?: string } {
  return { title: t('skills.toast.enableOk', { agentName }), description: skillName };
}

export function disableOkToast(
  t: TranslateFn,
  agentName: string,
  skillName: string,
): { title: string; description?: string } {
  return { title: t('skills.toast.disableOk', { agentName }), description: skillName };
}

export function conflictPromptToast(
  t: TranslateFn,
  agentName: string,
  skillName: string,
): { title: string; description?: string; actionLabel?: string } {
  return {
    title: t('skills.toast.conflictPromptTitle', { agentName }),
    description: t('skills.toast.conflictPromptDesc', { skillName }),
    actionLabel: t('skills.toast.conflictAction'),
  };
}

export function overwriteOkToast(
  t: TranslateFn,
  agentName: string,
  skillName: string,
): { title: string; description?: string } {
  return enableOkToast(t, agentName, skillName);
}

export function installNeedSourceToast(t: TranslateFn): { title: string; description?: string } {
  return { title: t('skills.toast.installNeedSource') };
}

export function installOkToast(t: TranslateFn): { title: string; description?: string } {
  return {
    title: t('skills.toast.installOk'),
    description: t('skills.toast.installOkDesc'),
  };
}

export function installFailedToast(
  t: TranslateFn,
  reason: string,
): { title: string; description?: string } {
  return { title: t('skills.toast.installFailed'), description: reason };
}

export function openPathMissingToast(t: TranslateFn): { title: string; description?: string } {
  return { title: t('skills.toast.openPathMissing') };
}

export function openPathFailedToast(
  t: TranslateFn,
  reason: string,
): { title: string; description?: string } {
  return { title: t('skills.toast.openPathFailed'), description: reason };
}

export function noAgentsToast(t: TranslateFn): { title: string; description?: string } {
  return {
    title: t('skills.toast.noAgents'),
    description: t('skills.toast.noAgentsDesc'),
  };
}

export function adoptOkToast(
  t: TranslateFn,
  overwrite: boolean,
): { title: string; description?: string } {
  return {
    title: overwrite ? t('skills.toast.adoptOkOverwrite') : t('skills.toast.adoptOk'),
    description: t('skills.toast.adoptOkDesc'),
  };
}

export function adoptFailedToast(
  t: TranslateFn,
  reason: string,
): { title: string; description?: string } {
  return { title: t('skills.toast.adoptFailed'), description: reason };
}

export function removeOkToast(
  t: TranslateFn,
  agentName: string,
  skillName: string,
): { title: string; description?: string } {
  return {
    title: t('skills.toast.removeOk', { agentName }),
    description: t('skills.toast.removeOkDesc', { skillName }),
  };
}

export function removeFailedToast(
  t: TranslateFn,
  reason: string,
): { title: string; description?: string } {
  return { title: t('skills.toast.removeFailed'), description: reason };
}

export function deleteSharedOkToast(
  t: TranslateFn,
  skillName: string,
): { title: string; description?: string } {
  return {
    title: t('skills.toast.deleteSharedOk'),
    description: t('skills.toast.deleteSharedOkDesc', { skillName }),
  };
}

export function deleteSharedFailedToast(
  t: TranslateFn,
  reason: string,
): { title: string; description?: string } {
  return { title: t('skills.toast.deleteSharedFailed'), description: reason };
}

export function marketInstallOkToast(
  t: TranslateFn,
  name: string,
): { title: string; description?: string; actionLabel?: string } {
  return {
    title: t('skills.toast.marketInstallOk'),
    description: t('skills.toast.marketInstallOkDesc', { name }),
    actionLabel: t('skills.toast.marketInstallAction'),
  };
}

export function marketExistsToast(
  t: TranslateFn,
  reason: string,
): { title: string; description?: string } {
  return { title: t('skills.toast.marketExists'), description: reason };
}

export function openDetailFailedToast(
  t: TranslateFn,
  reason: string,
): { title: string; description?: string } {
  return { title: t('skills.toast.openDetailFailed'), description: reason };
}
