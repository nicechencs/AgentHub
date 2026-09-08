import type { PluginAgentStatus } from '@/lib/backend/contracts/plugin-types';
import type { TranslateFn } from '@/lib/i18n';
import { canToggleListedPlugin } from './can-toggle';

export type PluginEmptyCopy = {
  title: string;
  description: string;
  showRefresh: boolean;
};

function unsupportedDescription(errorCode: string | null | undefined, t: TranslateFn): string {
  switch (errorCode) {
    case 'unsupported-cursor':
      return t('plugins.support.unsupportedCursor');
    case 'unsupported-dsh':
      return t('plugins.support.unsupportedDsh');
    case 'unsupported-zcode':
      return t('plugins.support.unsupportedZcode');
    default:
      return t('plugins.support.unsupportedNoCli');
  }
}

/** Agents whose official list command failed. */
export function pluginScanFailedAgents(
  agents: readonly PluginAgentStatus[] | undefined,
  visibleIds: ReadonlySet<string> | readonly string[],
): PluginAgentStatus[] {
  const visible = visibleIds instanceof Set ? visibleIds : new Set(visibleIds);
  if (visible.size === 0) return [];
  return (agents ?? []).filter(
    (row) => row.errorCode === 'cli-failed' && visible.has(row.agent),
  );
}

/** Empty-state copy for the plugins list: wired-empty vs planned vs no pack system. */
export function pluginEmptyCopy(
  filterAgent: string,
  agents: readonly PluginAgentStatus[] | undefined,
  agentLabel: string,
  t: TranslateFn,
  failedNames = '',
): PluginEmptyCopy {
  if (filterAgent === 'all') {
    if (failedNames) {
      return {
        title: t('plugins.empty.partialTitle'),
        description: t('plugins.empty.allPartial', { names: failedNames }),
        showRefresh: true,
      };
    }
    return {
      title: t('plugins.empty.title'),
      description: t('plugins.empty.all'),
      showRefresh: true,
    };
  }

  const status = agents?.find((row) => row.agent === filterAgent);
  const support = status?.support;
  const errorCode = status?.errorCode ?? '';

  if (support === 'planned' || errorCode === 'planned') {
    return {
      title: t('plugins.empty.plannedTitle'),
      description: t('plugins.support.planned'),
      showRefresh: false,
    };
  }

  if (support === 'unsupported') {
    return {
      title: t('plugins.empty.unsupportedTitle'),
      description: unsupportedDescription(errorCode, t),
      showRefresh: false,
    };
  }

  if (errorCode === 'cli-failed') {
    return {
      title: t('plugins.empty.title'),
      description: t('plugins.support.cliFailed'),
      showRefresh: true,
    };
  }

  return {
    title: t('plugins.empty.title'),
    description: canToggleListedPlugin(filterAgent)
      ? t('plugins.empty.agent', { name: agentLabel })
      : t('plugins.empty.agentListOnly', { name: agentLabel }),
    showRefresh: true,
  };
}
