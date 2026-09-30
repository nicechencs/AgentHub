import { AgentLogo } from '@/components/shared/AgentLogo';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Card } from '@/components/ui/card';
import {
  localEndpointBrandAgentId,
  type LocalEndpointKind,
} from '@/lib/route-endpoints';
import { cn } from '@/lib/utils';
import { agentCssVar } from '@/styles/tokens';
import { localEndpointKindLabel } from '@/pages/routes/shared/route-pool-view-model';
import { createTokenPoolLabel, type CreateTokenEndpointCard } from './tokens-model';

export function CreateTokenEndpointCards({
  cards,
  value,
  selectedKind,
  onChange,
  onSelectKind,
  disabled,
  unavailableReason,
}: {
  cards: readonly CreateTokenEndpointCard[];
  value: string;
  selectedKind?: LocalEndpointKind | '';
  onChange: (poolId: string) => void;
  onSelectKind?: (kind: LocalEndpointKind) => void;
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
        const selectable = card.pools.length > 0 && !disabled;
        const selected = selectedKind === card.kind
          || card.pools.some((pool) => pool.id === value);
        const color = agentCssVar(localEndpointBrandAgentId(card.kind));
        const pickCard = () => {
          onSelectKind?.(card.kind);
          if (card.pools.length === 1) {
            onChange(card.pools[0]!.id);
            return;
          }
          if (!card.pools.some((pool) => pool.id === value)) onChange('');
        };
        return (
          <Card
            key={card.kind}
            role="radio"
            tabIndex={selectable ? 0 : -1}
            aria-checked={selected}
            aria-disabled={!selectable}
            aria-label={`${card.path} ${label}`}
            data-create-endpoint={card.kind}
            title={card.pools.length > 0 ? undefined : unavailableReason}
            onClick={() => {
              if (!selectable) return;
              pickCard();
            }}
            onKeyDown={(event) => {
              if (!selectable) return;
              if (event.key === 'Enter' || event.key === ' ') {
                event.preventDefault();
                pickCard();
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
            {card.kind === 'messages' ? (
              <p className="text-meta text-secondary">{t('routes.tokens.messagesClaudeOnly')}</p>
            ) : null}
            {selected && card.pools.length > 1 ? (
              <div
                className="mt-1 flex flex-col gap-1"
                role="radiogroup"
                aria-label={t('routes.tokens.pickPool')}
                onClick={(event) => event.stopPropagation()}
              >
                <p className="text-meta text-muted">{t('routes.tokens.pickPool')}</p>
                {card.pools.map((pool) => {
                  const poolSelected = pool.id === value;
                  return (
                    <button
                      key={pool.id}
                      type="button"
                      role="radio"
                      aria-checked={poolSelected}
                      disabled={disabled}
                      onClick={() => onChange(pool.id)}
                      className={cn(
                        'rounded-btn border px-2 py-1 text-left text-sm',
                        poolSelected ? 'border-accent bg-hover/40' : 'border-border',
                      )}
                    >
                      {createTokenPoolLabel(pool)}
                    </button>
                  );
                })}
              </div>
            ) : null}
          </Card>
        );
      })}
    </div>
  );
}
