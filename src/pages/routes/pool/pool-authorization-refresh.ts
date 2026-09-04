import {
  importCurrentLogin,
  probeLiveAuth,
  refreshQuota,
  refreshToken,
  type LiveAuthProbe,
} from '@/lib/api/account';
import {
  oauthListAction,
  oauthListActionProbesQuota,
  type AccountAction,
} from '@/lib/backend/contracts/account-actions';
import type { TranslateFn } from '@/lib/i18n';
import type { Account } from '@/lib/types';
import {
  liveAuthCoexistenceNotice,
  liveAuthImportGate,
} from '@/pages/connections/connection-model';
import { oauthActionHoverTip } from '@/pages/connections/ticket-card-detail';

export type PoolAuthorizationRefreshToast = {
  title: string;
  description?: string;
  variant: 'success' | 'danger';
};

export type PoolAuthorizationRefreshResult = {
  toast: PoolAuthorizationRefreshToast;
  reload: boolean;
};

export type PoolAuthorizationRefreshDeps = {
  probeLiveAuth: (agentId: Account['agentId'], opts: { force: boolean }) => Promise<LiveAuthProbe>;
  importCurrentLogin: (agentId: Account['agentId']) => Promise<Account>;
  refreshToken: (agentId: Account['agentId'], accountId: string) => Promise<void>;
  refreshQuota: (agentId: Account['agentId'], accountId: string) => Promise<Account | undefined>;
};

const defaultDeps: PoolAuthorizationRefreshDeps = {
  probeLiveAuth,
  importCurrentLogin,
  refreshToken,
  refreshQuota,
};

export function poolAuthorizationRefreshAction(
  account?: Pick<
    Account,
    'agentId' | 'kind' | 'provider' | 'refreshable' | 'source' | 'isCurrent'
  > | null,
): AccountAction | undefined {
  if (!account) return undefined;
  return oauthListAction(account);
}

export function poolAuthorizationRefreshLabels(
  action: AccountAction,
  t: TranslateFn,
): { idle: string; busy: string; tip: string } {
  const tip = oauthActionHoverTip(action, t) ?? t('connections.list.refreshTip');
  if (action.kind === 'sync-current-login') {
    return {
      idle: t('connections.list.syncCurrentLogin'),
      busy: t('connections.list.syncing'),
      tip,
    };
  }
  return {
    idle: t('connections.list.refresh'),
    busy: t('connections.list.refreshing'),
    tip,
  };
}

function failResult(
  title: string,
  description?: string,
  reload = false,
): PoolAuthorizationRefreshResult {
  return {
    toast: description ? { title, description, variant: 'danger' } : { title, variant: 'danger' },
    reload,
  };
}

export async function runPoolAuthorizationRefresh(
  account: Account,
  t: TranslateFn,
  deps: PoolAuthorizationRefreshDeps = defaultDeps,
): Promise<PoolAuthorizationRefreshResult> {
  const action = oauthListAction(account);
  if (!action) {
    return failResult(t('connections.list.refreshFail'));
  }

  try {
    if (action.kind === 'sync-current-login') {
      let probe: LiveAuthProbe;
      try {
        probe = await deps.probeLiveAuth(account.agentId, { force: true });
      } catch {
        return failResult(
          t('connections.import.toastFail'),
          t('connections.list.cannotConfirmLogin'),
        );
      }
      const gate = liveAuthImportGate(probe, false, account.agentId, t);
      if (!gate.enabled) {
        return failResult(t('connections.import.toastFail'), gate.reason);
      }
      const imported = await deps.importCurrentLogin(account.agentId);
      if (oauthListActionProbesQuota(action.kind)) {
        await deps.refreshQuota(account.agentId, imported.id).catch(() => undefined);
      }
      const coexistenceNotice = liveAuthCoexistenceNotice(probe, account.agentId, t);
      return {
        toast: {
          title: t('connections.import.toastOk'),
          description: coexistenceNotice
            ? t('connections.import.toastOkCoexist', { label: imported.label })
            : t('connections.import.toastOkDesc', { label: imported.label }),
          variant: 'success',
        },
        reload: true,
      };
    }

    if (action.kind === 'refresh-credentials') {
      await deps.refreshToken(account.agentId, account.id);
      await deps.refreshQuota(account.agentId, account.id).catch(() => undefined);
      return {
        toast: { title: t('connections.list.refreshOk'), variant: 'success' },
        reload: true,
      };
    }

    await deps.refreshQuota(account.agentId, account.id);
    return {
      toast: { title: t('connections.list.refreshOk'), variant: 'success' },
      reload: true,
    };
  } catch (error) {
    if (error instanceof Error && error.name === 'OauthFileSyncPending') {
      return failResult(t('connections.list.refreshPartial'), error.message, true);
    }
    return failResult(
      action.kind === 'sync-current-login'
        ? t('connections.import.toastFail')
        : t('connections.list.refreshFail'),
      error instanceof Error ? error.message : String(error),
    );
  }
}
