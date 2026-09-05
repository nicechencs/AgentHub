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
import { agentDisplayName } from '@/config/agents';
import type { ConnectApiKeyDraft } from '@/lib/connect-flow/connect-intent';
import type { Sub2ApiGroup, Sub2ApiKey } from '@/lib/sub2api';
import type { AgentKey } from '@/lib/types';
import type { TokenImportAgentRef } from '@/pages/routes/tokens/token-import-model';
import {
  sub2apiImportDraft,
  sub2apiImportGate,
  sub2apiImportKindForKey,
} from './sub2api-import-model';

export function Sub2ApiImportToAgentButton({
  keyRow,
  groups,
  gatewayBaseUrl,
  installedAgents,
  onImport,
  busy = false,
}: {
  keyRow: Sub2ApiKey;
  groups: readonly Sub2ApiGroup[];
  gatewayBaseUrl: string;
  installedAgents: readonly TokenImportAgentRef[];
  onImport: (agentId: AgentKey, draft: ConnectApiKeyDraft) => void;
  busy?: boolean;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const kind = sub2apiImportKindForKey(keyRow, groups);
  const gate = sub2apiImportGate(keyRow, kind, installedAgents, t);
  const label = t('routes.tokens.importToAgent');
  const blockedReason = !gate.enabled ? (gate.reason ?? label) : null;

  const runImport = (agentId: AgentKey) => {
    const choice = gate.agents.find((agent) => agent.id === agentId);
    if (!choice?.enabled) return;
    const draft = sub2apiImportDraft(gatewayBaseUrl, keyRow, agentId, kind);
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

  if (blockedReason) {
    return (
      <Hint label={blockedReason}>
        <span>
          <Button type="button" variant="outline" size="sm" disabled aria-label={label}>
            {label}
          </Button>
        </span>
      </Hint>
    );
  }

  return (
    <DropdownMenu modal={false}>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size="sm"
          aria-label={label}
          disabled={busy}
          data-sub2api-import=""
          data-help="sub2api-import"
        >
          {label}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
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
