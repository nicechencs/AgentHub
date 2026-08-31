/**
 * 授权池页右上角的接入入口：先在浮动页面中选择接入方式，再打开对应的配置页面。
 * OAuth 只提供官方登录支持的三个 Agent；API 接入按下游接口类型提供固定选项。
 */
import { useMemo, useState, type ReactNode } from 'react';
import { AgentDot } from '@/components/shared/AgentDot';
import { useI18n } from '@/components/shared/LanguageProvider';
import { OAuthFlowDialog } from '@/components/connect/OAuthFlowDialog';
import { ProviderEditDialog } from '@/components/connections/ProviderEditDialog';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import type { AgentId } from '@/lib/types';

export type PoolAccessAgent = 'claude' | 'codex' | 'grok';

export type PoolOAuthChoice = {
  agentId: PoolAccessAgent;
  available: boolean;
};

export type PoolApiChoice = {
  agentId: PoolAccessAgent;
  endpoint: '/v1/messages' | '/v1/responses';
  available: boolean;
};

const OAUTH_AGENTS = ['claude', 'codex', 'grok'] as const satisfies readonly PoolAccessAgent[];

const API_CHOICES = [
  { agentId: 'claude', endpoint: '/v1/messages' },
  { agentId: 'codex', endpoint: '/v1/responses' },
  { agentId: 'grok', endpoint: '/v1/responses' },
] as const satisfies readonly {
  agentId: PoolAccessAgent;
  endpoint: PoolApiChoice['endpoint'];
}[];

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

/** Maps the fixed API endpoint choices to their current installed state. */
export function poolApiChoices(agents: readonly AgentId[]): PoolApiChoice[] {
  return API_CHOICES.map((choice) => ({
    ...choice,
    available: agents.includes(choice.agentId),
  }));
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
        <div className="flex flex-col gap-2">{children}</div>
      </DialogContent>
    </Dialog>
  );
}

function ChoiceButton({
  label,
  detail,
  unavailableLabel,
  agentId,
  available,
  onClick,
}: {
  label: string;
  detail?: string;
  unavailableLabel: string;
  agentId: PoolAccessAgent;
  available: boolean;
  onClick: () => void;
}) {
  return (
    <Button
      type="button"
      variant="outline"
      size="lg"
      className="h-auto w-full justify-start px-3 py-2.5 text-left"
      disabled={!available}
      onClick={onClick}
    >
      <AgentDot agentId={agentId} size="sm" title={null} />
      <span className="min-w-0">
        <span className="block text-body font-medium text-primary">{label}</span>
        {detail ? <span className="block text-meta text-secondary">{detail}</span> : null}
        {!available ? <span className="block text-meta text-muted">{unavailableLabel}</span> : null}
      </span>
    </Button>
  );
}

const oauthLabelKeys = {
  claude: 'routes.pool.page.oauthClaude',
  codex: 'routes.pool.page.oauthCodex',
  grok: 'routes.pool.page.oauthGrok',
} as const;

const apiLabelKeys = {
  claude: 'routes.pool.page.apiClaude',
  codex: 'routes.pool.page.apiCodex',
  grok: 'routes.pool.page.apiGrok',
} as const;

export function PoolAddButtons({
  agents,
  oauthAgents,
  onChanged,
}: {
  agents: readonly AgentId[];
  oauthAgents: readonly AgentId[];
  /** Called after an OAuth flow or API provider is saved. */
  onChanged?: () => void;
}) {
  const { t } = useI18n();
  const [picker, setPicker] = useState<'oauth' | 'api' | null>(null);
  const [oauthAgentId, setOauthAgentId] = useState<PoolAccessAgent | null>(null);
  const [apiAgentId, setApiAgentId] = useState<PoolAccessAgent | null>(null);
  const oauthChoices = useMemo(
    () => poolOAuthChoices(agents, oauthAgents),
    [agents, oauthAgents],
  );
  const apiChoices = useMemo(() => poolApiChoices(agents), [agents]);

  const selectOAuthAgent = (agentId: PoolAccessAgent) => {
    setPicker(null);
    setOauthAgentId(agentId);
  };
  const selectApiAgent = (agentId: PoolAccessAgent) => {
    setPicker(null);
    setApiAgentId(agentId);
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
        onClick={() => setPicker('api')}
      >
        {t('routes.pool.page.addApiKey')}
      </Button>

      <ChoiceDialog
        open={picker === 'oauth'}
        onOpenChange={(open) => setPicker(open ? 'oauth' : null)}
        title={t('routes.pool.page.oauthDialogTitle')}
        description={t('routes.pool.page.oauthDialogDescription')}
      >
        {oauthChoices.map((choice) => (
          <ChoiceButton
            key={choice.agentId}
            agentId={choice.agentId}
            available={choice.available}
            label={t(oauthLabelKeys[choice.agentId])}
            unavailableLabel={t('routes.pool.page.choiceUnavailable')}
            onClick={() => selectOAuthAgent(choice.agentId)}
          />
        ))}
      </ChoiceDialog>

      <ChoiceDialog
        open={picker === 'api'}
        onOpenChange={(open) => setPicker(open ? 'api' : null)}
        title={t('routes.pool.page.apiDialogTitle')}
        description={t('routes.pool.page.apiDialogDescription')}
      >
        {apiChoices.map((choice) => (
          <ChoiceButton
            key={`${choice.agentId}-${choice.endpoint}`}
            agentId={choice.agentId}
            available={choice.available}
            label={t(apiLabelKeys[choice.agentId])}
            detail={choice.endpoint}
            unavailableLabel={t('routes.pool.page.choiceUnavailable')}
            onClick={() => selectApiAgent(choice.agentId)}
          />
        ))}
      </ChoiceDialog>

      {oauthAgentId ? (
        <OAuthFlowDialog
          agentId={oauthAgentId}
          open
          onOpenChange={(open) => {
            if (open) return;
            setOauthAgentId(null);
            onChanged?.();
          }}
          onCompleted={() => {}}
        />
      ) : null}

      {apiAgentId ? (
        <ProviderEditDialog
          agentId={apiAgentId}
          open
          mode="add"
          onOpenChange={(open) => {
            if (!open) setApiAgentId(null);
          }}
          onSaved={() => {
            setApiAgentId(null);
            onChanged?.();
          }}
        />
      ) : null}
    </>
  );
}
