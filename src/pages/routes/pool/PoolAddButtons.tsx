/**
 * 连接池页右上角的添加入口：先在浮动页面中选择添加方式，再打开对应的配置页面。
 * 官方登录只提供支持的三个 Agent；添加 API Key 时先填服务地址和 Key，再勾选接口类型。
 * 这里加入的登录只给连接池用，不会出现在连接页。
 */
import { useMemo, useState, type ReactNode } from 'react';
import { agentDisplayName } from '@/config/agents';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { useI18n } from '@/components/shared/LanguageProvider';
import { OAuthFlowDialog } from '@/components/connect/OAuthFlowDialog';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/toast';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import type { AgentId } from '@/lib/types';
import { attachPoolOwnedAuthorization, syncConnectionAuthorizations } from '@/lib/api/adapter';
import type {
  AdapterSourceKind,
  DefaultRoutePoolOverview,
  RoutePoolSurface,
  SyncConnectionSource,
} from '@/lib/backend/contracts';
import type { ConnectionEntry } from '@/lib/connection-entry';
import { sourceKindLabel } from '@/pages/routes/shared/adapter-create-flow';
import { isPoolShareableLogin } from '@/pages/connections/ticket-pool-import';
import { cn } from '@/lib/utils';
import { ApiAccessDialog } from './ApiAccessDialog';
import { type PoolAccessAgent } from './api-access-model';

export type { PoolAccessAgent, PoolApiChoice } from './api-access-model';
export { poolApiChoices, poolSurfaceForApiChoice } from './api-access-model';

export type PoolOAuthChoice = {
  agentId: PoolAccessAgent;
  available: boolean;
};

const OAUTH_AGENTS = ['claude', 'codex', 'grok'] as const satisfies readonly PoolAccessAgent[];

/** Maps the fixed OAuth choices to their current installed/supported state. */
export function poolOAuthChoices(
  agents: readonly AgentId[],
  oauthAgents: readonly AgentId[],
): PoolOAuthChoice[] {
  return OAUTH_AGENTS.map((agentId) => ({
    agentId,
    available: agents.includes(agentId) && oauthAgents.includes(agentId),
  }));
}

export function poolSurfaceForOAuth(agentId: PoolAccessAgent): RoutePoolSurface {
  return agentId === 'claude' ? 'messages' : 'responses';
}

export type PoolSyncCandidate = SyncConnectionSource & {
  key: string;
  agentId: AgentId;
  title: string;
  alreadySynced: boolean;
};

/** Build a credential-free source list from the shared Connections/read-model rows. */
export function poolSyncCandidates(
  entries: readonly ConnectionEntry[],
  pools: readonly DefaultRoutePoolOverview[],
): PoolSyncCandidate[] {
  const synced = new Set(
    pools.flatMap((pool) => pool.members.map((member) => `${member.sourceKind}:${member.sourceId}`)),
  );
  return entries
    .filter((entry) => isPoolShareableLogin({ agentId: entry.agentId, kind: entry.kind }))
    .filter((entry) => entry.account?.home !== 'route_pool' && entry.provider?.home !== 'route_pool')
    .map((entry) => {
      const sourceKind = entry.source;
      const sourceId = entry.id;
      const key = `${sourceKind}:${sourceId}`;
      return {
        key,
        sourceKind,
        sourceId,
        agentId: entry.agentId,
        title: entry.title,
        alreadySynced: synced.has(key),
      };
    });
}

type ChoiceDialogProps = {
  open: boolean;
  title: string;
  description: string;
  children: ReactNode;
  onOpenChange: (open: boolean) => void;
};

function ChoiceDialog({
  open,
  title,
  description,
  children,
  onOpenChange,
}: ChoiceDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <div className="grid grid-cols-1 gap-3">{children}</div>
      </DialogContent>
    </Dialog>
  );
}

function OAuthChoiceCard({
  label,
  unavailableLabel,
  agentId,
  available,
  onClick,
}: {
  label: string;
  unavailableLabel: string;
  agentId: PoolAccessAgent;
  available: boolean;
  onClick: () => void;
}) {
  return (
    <div
      role="button"
      tabIndex={available ? 0 : -1}
      aria-disabled={!available}
      onClick={() => {
        if (available) onClick();
      }}
      onKeyDown={(event) => {
        if (!available) return;
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onClick();
        }
      }}
      className={cn(
        'rounded-card border border-border bg-panel p-4 text-left transition-colors',
        available && 'cursor-pointer hover:bg-hover/50',
        !available && 'opacity-60',
      )}
    >
      <div className="flex items-center gap-3">
        <AgentLogo agentId={agentId} size="md" />
        <div className="min-w-0 flex-1">
          <span className="text-body font-medium">{label}</span>
          {!available ? <p className="mt-1 text-xs text-muted">{unavailableLabel}</p> : null}
        </div>
      </div>
    </div>
  );
}

const oauthLabelKeys = {
  claude: 'routes.pool.page.oauthClaude',
  codex: 'routes.pool.page.oauthCodex',
  grok: 'routes.pool.page.oauthGrok',
} as const;

export function PoolAddButtons({
  agents,
  oauthAgents,
  entries = [],
  defaultPools = [],
  onChanged,
}: {
  agents: readonly AgentId[];
  oauthAgents: readonly AgentId[];
  entries?: readonly ConnectionEntry[];
  defaultPools?: readonly DefaultRoutePoolOverview[];
  /** Called after an OAuth flow or API provider is saved. */
  onChanged?: () => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [picker, setPicker] = useState<'oauth' | null>(null);
  const [apiAccessOpen, setApiAccessOpen] = useState(false);
  const [oauthAgentId, setOauthAgentId] = useState<PoolAccessAgent | null>(null);
  const [syncOpen, setSyncOpen] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [selectedSyncKeys, setSelectedSyncKeys] = useState<Set<string>>(new Set());
  const oauthChoices = useMemo(
    () => poolOAuthChoices(agents, oauthAgents),
    [agents, oauthAgents],
  );
  const syncCandidates = useMemo(
    () => poolSyncCandidates(entries, defaultPools),
    [defaultPools, entries],
  );
  const attachAuthorization = async (
    sourceKind: AdapterSourceKind,
    sourceId: string,
    targetAgentId: PoolAccessAgent,
    surface: RoutePoolSurface,
  ) => {
    try {
      await attachPoolOwnedAuthorization({
        sourceKind,
        sourceId,
        targetAgentId,
        surface,
      });
      toast({ title: t('routes.pool.page.added'), variant: 'success' });
      onChanged?.();
    } catch (error) {
      toast({
        title: t('routes.pool.page.addFailed'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    }
  };

  const syncFromConnections = async () => {
    setSyncing(true);
    try {
      const sources: SyncConnectionSource[] = syncCandidates
        .filter((candidate) => !candidate.alreadySynced && selectedSyncKeys.has(candidate.key))
        .map(({ sourceKind, sourceId }) => ({ sourceKind, sourceId }));
      const result = await syncConnectionAuthorizations({ sources });
      setSyncOpen(false);
      toast({
        title: result.added > 0
          ? t('routes.pool.page.synced', { count: result.added })
          : t('routes.pool.page.syncNone'),
        variant: 'success',
      });
      onChanged?.();
    } catch (error) {
      toast({
        title: t('routes.pool.page.syncFailed'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      setSyncing(false);
    }
  };

  const openSyncDialog = () => {
    setSelectedSyncKeys(new Set(
      syncCandidates
        .filter((candidate) => !candidate.alreadySynced)
        .map((candidate) => candidate.key),
    ));
    setSyncOpen(true);
  };

  const selectOAuthAgent = (agentId: PoolAccessAgent) => {
    setPicker(null);
    setOauthAgentId(agentId);
  };

  return (
    <>
      <Button
        type="button"
        size="sm"
        variant="secondary"
        onClick={() => setPicker('oauth')}
      >
        {t('routes.pool.page.addOauth')}
      </Button>
      <Button
        type="button"
        size="sm"
        variant="secondary"
        onClick={() => setApiAccessOpen(true)}
      >
        {t('routes.pool.page.addApiKey')}
      </Button>
      <Button
        type="button"
        size="sm"
        variant="secondary"
        onClick={openSyncDialog}
      >
        {t('routes.pool.page.syncFromConnections')}
      </Button>

      <Dialog open={syncOpen} onOpenChange={(open) => { if (!syncing) setSyncOpen(open); }}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{t('routes.pool.page.syncTitle')}</DialogTitle>
            <DialogDescription>{t('routes.pool.page.syncDescription')}</DialogDescription>
          </DialogHeader>
          <div className="max-h-80 space-y-2 overflow-y-auto">
            {syncCandidates.length > 0 ? syncCandidates.map((candidate) => {
              const disabled = candidate.alreadySynced;
              return (
                <label
                  key={candidate.key}
                  className={cn(
                    'flex items-center gap-3 rounded-card border border-border bg-panel p-3',
                    disabled ? 'cursor-not-allowed opacity-50' : 'cursor-pointer hover:bg-hover/50',
                  )}
                >
                  <input
                    type="checkbox"
                    checked={!disabled && selectedSyncKeys.has(candidate.key)}
                    disabled={disabled}
                    onChange={(event) => {
                      setSelectedSyncKeys((current) => {
                        const next = new Set(current);
                        if (event.target.checked) next.add(candidate.key);
                        else next.delete(candidate.key);
                        return next;
                      });
                    }}
                  />
                  <AgentLogo agentId={candidate.agentId} size="sm" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-medium">{candidate.title}</span>
                    <span className="block truncate text-xs text-muted">
                      {agentDisplayName(candidate.agentId)} · {sourceKindLabel(candidate.sourceKind, t)}
                    </span>
                  </span>
                </label>
              );
            }) : (
              <p className="text-sm text-muted">{t('routes.pool.page.syncNone')}</p>
            )}
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="secondary"
              onClick={() => setSyncOpen(false)}
              disabled={syncing}
            >
              {t('common.cancel')}
            </Button>
            <Button
              type="button"
              onClick={() => void syncFromConnections()}
              disabled={syncing || selectedSyncKeys.size === 0}
            >
              {syncing ? t('routes.pool.page.syncing') : t('routes.pool.page.syncConfirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ChoiceDialog
        open={picker === 'oauth'}
        onOpenChange={(open) => setPicker(open ? 'oauth' : null)}
        title={t('routes.pool.page.oauthDialogTitle')}
        description={t('routes.pool.page.oauthDialogDescription')}
      >
        {oauthChoices.map((choice) => (
          <OAuthChoiceCard
            key={choice.agentId}
            agentId={choice.agentId}
            available={choice.available}
            label={t(oauthLabelKeys[choice.agentId])}
            unavailableLabel={t('routes.pool.page.choiceUnavailable')}
            onClick={() => selectOAuthAgent(choice.agentId)}
          />
        ))}
      </ChoiceDialog>

      <ApiAccessDialog
        open={apiAccessOpen}
        agents={agents}
        onOpenChange={setApiAccessOpen}
        onSaved={onChanged}
      />

      {oauthAgentId ? (
        <OAuthFlowDialog
          agentId={oauthAgentId}
          open
          offerSwitch={false}
          poolOwned
          successDescription={t('routes.pool.page.oauthSaved')}
          onOpenChange={(open) => {
            if (open) return;
            setOauthAgentId(null);
          }}
          onStored={(account) => {
            const agentId = oauthAgentId;
            // Grok device-code completion attaches to the authorization pool
            // inside the backend operation, so a second async attach would
            // only create a misleading failure after a successful login.
            if (agentId === 'grok') {
              onChanged?.();
              return;
            }
            void attachAuthorization(
              'account',
              account.id,
              agentId,
              poolSurfaceForOAuth(agentId),
            );
          }}
          onCompleted={() => {}}
        />
      ) : null}

    </>
  );
}
