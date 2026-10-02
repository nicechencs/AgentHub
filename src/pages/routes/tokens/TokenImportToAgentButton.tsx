import { useEffect, useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import { AgentDot } from '@/components/shared/AgentDot';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Hint } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import { getLocalGatewayStatus } from '@/lib/api/adapter';
import type { LocalGatewayStatus } from '@/lib/backend/contracts/adapter';
import type { ConnectApiKeyDraft } from '@/lib/connect-flow/connect-intent';
import type { AgentKey } from '@/lib/types';
import { ROUTES_BOARD_PATH } from '@/lib/routes-path';
import { agentDisplayName } from '@/config/agents';
import {
  tokenImportAgentChoice,
  tokenImportApiKeyDraft,
  tokenImportGate,
  type TokenImportAgentRef,
} from './token-import-model';
import type { LocalTokenRow } from './tokens-model';

type GatewayCheckState = 'idle' | 'pending' | 'closed' | 'restarting' | 'error';
type GatewayImportStatus = Exclude<GatewayCheckState, 'idle' | 'pending' | 'error'> | 'ready';

function tokenImportGatewayStatus(
  status: Pick<LocalGatewayStatus, 'running' | 'restarting'>,
): GatewayImportStatus {
  if (status.restarting) return 'restarting';
  if (!status.running) return 'closed';
  return 'ready';
}

async function checkLocalGatewayForTokenImport(
  readStatus: () => Promise<Pick<LocalGatewayStatus, 'running' | 'restarting'>>,
  onReady: () => void,
  onBlocked: (state: Exclude<GatewayCheckState, 'idle' | 'pending'>) => void,
): Promise<void> {
  let status: GatewayImportStatus;
  try {
    status = tokenImportGatewayStatus(await readStatus());
  } catch {
    onBlocked('error');
    return;
  }
  if (status === 'ready') {
    onReady();
    return;
  }
  onBlocked(status);
}

export function TokenImportToAgentButton({
  row,
  installedAgents,
  onImport,
  size = 'sm',
  className,
}: {
  row: LocalTokenRow;
  installedAgents: readonly TokenImportAgentRef[];
  onImport: (agentId: AgentKey, draft: ConnectApiKeyDraft) => void;
  size?: 'sm' | 'default';
  className?: string;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const gate = tokenImportGate(row, installedAgents, t);
  const [confirmAgentId, setConfirmAgentId] = useState<AgentKey | null>(null);
  const [gatewayCheck, setGatewayCheck] = useState<GatewayCheckState>('idle');
  const gatewayCheckPendingRef = useRef(false);
  const gatewayCheckRequestRef = useRef(0);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      gatewayCheckRequestRef.current += 1;
      gatewayCheckPendingRef.current = false;
    };
  }, []);

  const runImport = (agentId: AgentKey) => {
    const choice = tokenImportAgentChoice(row.kind, { id: agentId, name: agentId }, t);
    if (!choice.enabled) return;
    const draft = tokenImportApiKeyDraft(row, agentId);
    if (!draft) {
      toast({
        title: t('routes.tokens.importFailed'),
        description: t('routes.tokens.importNeedKey'),
        variant: 'danger',
      });
      return;
    }
    onImport(agentId, draft);
  };

  const checkGatewayAndImport = async (agentId: AgentKey) => {
    if (gatewayCheck === 'pending' || gatewayCheckPendingRef.current) return;
    const requestId = gatewayCheckRequestRef.current + 1;
    gatewayCheckRequestRef.current = requestId;
    gatewayCheckPendingRef.current = true;
    setGatewayCheck('pending');
    await checkLocalGatewayForTokenImport(
      getLocalGatewayStatus,
      () => {
        if (
          !mountedRef.current
          || gatewayCheckRequestRef.current !== requestId
          || !gatewayCheckPendingRef.current
        ) return;
        gatewayCheckPendingRef.current = false;
        setGatewayCheck('idle');
        setConfirmAgentId(null);
        runImport(agentId);
      },
      (state) => {
        if (
          !mountedRef.current
          || gatewayCheckRequestRef.current !== requestId
          || !gatewayCheckPendingRef.current
        ) return;
        gatewayCheckPendingRef.current = false;
        setGatewayCheck(state);
      },
    );
  };

  const closeConfirm = () => {
    gatewayCheckRequestRef.current += 1;
    gatewayCheckPendingRef.current = false;
    if (!mountedRef.current) return;
    setConfirmAgentId(null);
    setGatewayCheck('idle');
  };

  const gatewayStatusCopy = gatewayCheck === 'closed'
    ? {
      title: t('routes.tokens.importGatewayClosedTitle'),
      description: t('routes.tokens.importGatewayClosedDescription'),
    }
    : gatewayCheck === 'restarting'
      ? {
        title: t('routes.tokens.importGatewayRestartingTitle'),
        description: t('routes.tokens.importGatewayRestartingDescription'),
      }
      : gatewayCheck === 'error'
        ? {
          title: t('routes.tokens.importGatewayStatusFailedTitle'),
          description: t('routes.tokens.importGatewayStatusFailedDescription'),
        }
        : null;

  const confirmName = confirmAgentId
    ? (installedAgents.find((agent) => agent.id === confirmAgentId)?.name
      || agentDisplayName(confirmAgentId))
    : '';

  const label = t('routes.tokens.importToAgent');
  const blockedReason = !gate.enabled ? (gate.reason ?? label) : null;

  if (blockedReason) {
    return (
      <Hint label={blockedReason}>
        <span
          className={className}
          onClick={(event) => event.stopPropagation()}
        >
          <Button
            type="button"
            variant="outline"
            size={size}
            disabled
            aria-label={label}
          >
            {label}
          </Button>
        </span>
      </Hint>
    );
  }

  const stopRow = (event: { stopPropagation: () => void }) => {
    event.stopPropagation();
  };

  return (
    <DropdownMenu modal={false}>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size={size}
          className={className}
          aria-label={label}
          onClick={stopRow}
          onPointerDown={stopRow}
        >
          {label}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="end"
        onClick={stopRow}
        onCloseAutoFocus={(event) => event.preventDefault()}
      >
        {gate.agents.map((agent) => {
          const name = agent.name || agentDisplayName(agent.id);
          return (
            <DropdownMenuItem
              key={agent.id}
              disabled={!agent.enabled}
              onSelect={() => {
                if (!agent.enabled) return;
                gatewayCheckRequestRef.current += 1;
                gatewayCheckPendingRef.current = false;
                setGatewayCheck('idle');
                setConfirmAgentId(agent.id);
              }}
            >
              <AgentDot agentId={agent.id} size="md" title={null} />
              <span>{name}</span>
              {agent.reason ? (
                <span className="ml-auto text-meta text-muted">{agent.reason}</span>
              ) : null}
            </DropdownMenuItem>
          );
        })}
      </DropdownMenuContent>
      <Dialog open={confirmAgentId != null} onOpenChange={(open) => { if (!open) closeConfirm(); }}>
        <DialogContent className="max-w-sm" hideClose={gatewayCheck === 'pending'}>
          <DialogHeader>
            <DialogTitle>
              {gatewayStatusCopy?.title ?? t('routes.tokens.importConfirmTitle', { name: confirmName })}
            </DialogTitle>
            <DialogDescription>
              {gatewayStatusCopy?.description
                ?? t('routes.tokens.importConfirmDescription', { name: confirmName })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            {gatewayCheck === 'closed' ? (
              <Link
                to={ROUTES_BOARD_PATH}
                className="text-meta text-accent underline-offset-2 hover:underline"
              >
                {t('routes.tokens.importOpenRoutes')}
              </Link>
            ) : null}
            <Button variant="secondary" disabled={gatewayCheck === 'pending'} onClick={closeConfirm}>
              {t('common.cancel')}
            </Button>
            <Button
              disabled={gatewayCheck === 'pending'}
              onClick={() => {
                if (!confirmAgentId) return;
                void checkGatewayAndImport(confirmAgentId);
              }}
            >
              {gatewayCheck === 'pending'
                ? t('routes.tokens.importCheckingGateway')
                : t('routes.tokens.importToAgent')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </DropdownMenu>
  );
}
