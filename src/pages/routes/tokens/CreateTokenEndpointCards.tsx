import { AgentLogo } from '@/components/shared/AgentLogo';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Card } from '@/components/ui/card';
import {
  localEndpointBrandAgentId,
} from '@/lib/route-endpoints';
import { cn } from '@/lib/utils';
import { agentCssVar } from '@/styles/tokens';
import { localEndpointKindLabel } from '@/pages/routes/shared/route-pool-view-model';
import type { CreateTokenEndpointCard } from './tokens-model';

export function CreateTokenEndpointCards({
  cards,
  value,
  onChange,
  disabled,
  unavailableReason,
}: {
  cards: readonly CreateTokenEndpointCard[];
  value: string;
  onChange: (poolId: string) => void;
  disabled?: boolean;
  unavailableReason: string;
}) {
  const { t } = useI18n();

  return (
    <div
      className="flex flex-col gap-2"
      role="radiogroup"
      aria-label={t('routes.tokens.fieldEndpoint')}
    >
      {cards.map((card) => {
        const label = localEndpointKindLabel(card.kind, t);
        const selectable = Boolean(card.poolId) && !disabled;
        const selected = Boolean(card.poolId) && card.poolId === value;
        const color = agentCssVar(localEndpointBrandAgentId(card.kind));
        return (
          <Card
            key={card.kind}
            role="radio"
            tabIndex={selectable ? 0 : -1}
            aria-checked={selected}
            aria-disabled={!selectable}
            aria-label={`${card.path} ${label}`}
            data-create-endpoint={card.kind}
            title={card.poolId ? undefined : unavailableReason}
            onClick={() => {
              if (!selectable || !card.poolId) return;
              onChange(card.poolId);
            }}
            onKeyDown={(event) => {
              if (!selectable || !card.poolId) return;
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                onChange(card.poolId);
              }
            }}
            className={cn(
              'flex w-full flex-col gap-1.5 p-3 text-left transition-colors',
              selectable && 'cursor-pointer hover:border-accent/40 hover:bg-hover/40',
              selected && 'border-accent bg-hover/40',
              !selectable && 'cursor-not-allowed opacity-60',
            )}
          >
            <div className="flex min-w-0 items-baseline justify-between gap-2">
              <span
                className="min-w-0 truncate font-mono text-sm font-medium"
                style={{ color }}
              >
                {card.path}
              </span>
              <span className="shrink-0 text-xs text-secondary">{label}</span>
            </div>
            <div className="flex flex-wrap items-center gap-1">
              {card.agentIds.map((agentId) => (
                <AgentLogo key={agentId} agentId={agentId} size="sm" />
              ))}
            </div>
          </Card>
        );
      })}
    </div>
  );
}
