/**
 * 授权池页右上角的接入入口：先选 Agent，再深链到连接页打开对应弹窗。
 * OAuth 只列支持官方登录的 Agent；API Key 列全部可用 Agent。
 */
import { useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { ChevronDown } from 'lucide-react';
import { agentDisplayName } from '@/config/agents';
import { useI18n } from '@/components/shared/LanguageProvider';
import { AgentDot } from '@/components/shared/AgentDot';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  buildConnectionsGuideUrl,
  type ConnectGuideIntent,
} from '@/lib/connect-flow/connect-intent';
import type { AgentId } from '@/lib/types';

export type PoolAddTarget = { agentId: AgentId; intent: ConnectGuideIntent; url: string };

export function poolAddTargets(
  agents: readonly AgentId[],
  oauthAgents: readonly AgentId[],
  intent: Extract<ConnectGuideIntent, 'oauth' | 'add-key'>,
): PoolAddTarget[] {
  const pool =
    intent === 'oauth' ? agents.filter((id) => oauthAgents.includes(id)) : agents;
  return pool.map((agentId) => ({
    agentId,
    intent,
    url: buildConnectionsGuideUrl({ agentId, intent }),
  }));
}

function PoolAddMenu({
  label,
  targets,
  open,
  onOpenChange,
}: {
  label: string;
  targets: PoolAddTarget[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useI18n();
  const navigate = useNavigate();
  return (
    <DropdownMenu modal={false} open={open} onOpenChange={onOpenChange}>
      <DropdownMenuTrigger asChild>
        <Button type="button" size="sm" variant="secondary">
          {label}
          <ChevronDown className="h-3.5 w-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="end"
        className="min-w-[12rem]"
        onCloseAutoFocus={(event) => event.preventDefault()}
      >
        <DropdownMenuLabel>{t('connections.list.addAgent')}</DropdownMenuLabel>
        {targets.length === 0 ? (
          <DropdownMenuItem disabled>{t('connections.list.noAddAgent')}</DropdownMenuItem>
        ) : (
          targets.map((target) => (
            <DropdownMenuItem
              key={target.agentId}
              onSelect={() => {
                onOpenChange(false);
                navigate(target.url);
              }}
            >
              <span className="flex min-w-0 items-center gap-2">
                <AgentDot agentId={target.agentId} size="sm" title={null} />
                <span className="truncate">{agentDisplayName(target.agentId)}</span>
              </span>
            </DropdownMenuItem>
          ))
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function PoolAddButtons({
  agents,
  oauthAgents,
}: {
  agents: readonly AgentId[];
  oauthAgents: readonly AgentId[];
}) {
  const { t } = useI18n();
  const [openMenu, setOpenMenu] = useState<ConnectGuideIntent | null>(null);
  const oauthTargets = useMemo(
    () => poolAddTargets(agents, oauthAgents, 'oauth'),
    [agents, oauthAgents],
  );
  const apiKeyTargets = useMemo(
    () => poolAddTargets(agents, oauthAgents, 'add-key'),
    [agents, oauthAgents],
  );
  const openFor = (intent: ConnectGuideIntent) => openMenu === intent;
  const changeFor = (intent: ConnectGuideIntent) => (open: boolean) =>
    setOpenMenu(open ? intent : null);

  return (
    <>
      <PoolAddMenu
        label={t('routes.pool.page.addOauth')}
        targets={oauthTargets}
        open={openFor('oauth')}
        onOpenChange={changeFor('oauth')}
      />
      <PoolAddMenu
        label={t('routes.pool.page.addApiKey')}
        targets={apiKeyTargets}
        open={openFor('add-key')}
        onOpenChange={changeFor('add-key')}
      />
    </>
  );
}
