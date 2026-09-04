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
import type { ConnectApiKeyDraft } from '@/lib/connect-flow/connect-intent';
import type { AgentKey } from '@/lib/types';
import { agentDisplayName } from '@/config/agents';
import {
  tokenImportAgentChoice,
  tokenImportApiKeyDraft,
  tokenImportGate,
  type TokenImportAgentRef,
} from './token-import-model';
import type { LocalTokenRow } from './tokens-model';

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
                runImport(agent.id);
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
    </DropdownMenu>
  );
}
