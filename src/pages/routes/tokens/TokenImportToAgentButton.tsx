import { useState } from 'react';
import { Loader2 } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Hint } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { AgentId } from '@/lib/types';
import { agentDisplayName } from '@/config/agents';
import {
  tokenImportGate,
  type TokenImportAgentRef,
} from './token-import-model';
import { importLocalTokenToAgent } from './token-import-action';
import type { LocalTokenRow } from './tokens-model';

export function TokenImportToAgentButton({
  row,
  profile,
  siblingProfiles,
  installedAgents,
  onImported,
  size = 'sm',
  className,
}: {
  row: LocalTokenRow;
  profile: AdapterProfile | null | undefined;
  siblingProfiles?: readonly AdapterProfile[];
  installedAgents: readonly TokenImportAgentRef[];
  onImported?: () => void;
  size?: 'sm' | 'default';
  className?: string;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [busyId, setBusyId] = useState<AgentId | null>(null);
  const gate = tokenImportGate(row, installedAgents, t);
  const busy = busyId != null;

  const runImport = async (agentId: AgentId) => {
    if (busy || !profile) return;
    setBusyId(agentId);
    try {
      await importLocalTokenToAgent({
        profile,
        agentId,
        localToken: row.token,
        siblingProfiles,
      });
      toast({
        title: t('routes.tokens.importSuccess', { name: agentDisplayName(agentId) }),
        variant: 'success',
      });
      onImported?.();
    } catch (error) {
      toast({
        title: t('routes.tokens.importFailed'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      setBusyId(null);
    }
  };

  const label = t('routes.tokens.importToAgent');
  const profileReady = Boolean(profile);
  const blockedReason = !gate.enabled
    ? (gate.reason ?? label)
    : (!profileReady ? t('routes.tokens.importNeedEntry') : null);
  const disabled = blockedReason != null || busy;

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

  return (
    <DropdownMenu>
      <Hint label={label}>
        <DropdownMenuTrigger asChild>
          <Button
            type="button"
            variant="outline"
            size={size}
            className={className}
            disabled={disabled}
            aria-label={label}
            onClick={(event) => event.stopPropagation()}
          >
            {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden /> : null}
            {busy ? t('routes.tokens.importing') : label}
          </Button>
        </DropdownMenuTrigger>
      </Hint>
      <DropdownMenuContent align="end" onClick={(event) => event.stopPropagation()}>
        {gate.agents.map((agent) => (
          <DropdownMenuItem
            key={agent.id}
            disabled={busy}
            onSelect={() => { void runImport(agent.id); }}
          >
            <AgentDot agentId={agent.id} size="md" title={null} />
            <span>{agent.name || agentDisplayName(agent.id)}</span>
            {busyId === agent.id ? (
              <Loader2 className="ml-auto h-3.5 w-3.5 animate-spin" aria-hidden />
            ) : null}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
