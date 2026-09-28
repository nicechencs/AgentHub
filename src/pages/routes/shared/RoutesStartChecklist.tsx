import { Check } from 'lucide-react';
import { Link } from 'react-router-dom';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { MessageKey } from '@/lib/i18n';
import { ROUTES_BOARD_PATH, ROUTES_POOL_PATH, ROUTES_TOKENS_PATH } from '@/lib/routes-path';
import { cn } from '@/lib/utils';

export type RoutesStartStep = 'pool' | 'forward' | 'key';

/** One order for board / pool / tokens: login in the pool → forwarding on → copy or import a key. */
export const ROUTES_START_STEPS: ReadonlyArray<{
  id: RoutesStartStep;
  labelKey: MessageKey;
  to: string;
}> = [
  { id: 'pool', labelKey: 'routes.start.pool', to: ROUTES_POOL_PATH },
  { id: 'forward', labelKey: 'routes.start.forward', to: ROUTES_BOARD_PATH },
  { id: 'key', labelKey: 'routes.start.key', to: ROUTES_TOKENS_PATH },
];

export function RoutesStartChecklist({
  done,
  showTitle = true,
  className,
}: {
  done?: Partial<Record<RoutesStartStep, boolean>>;
  showTitle?: boolean;
  className?: string;
}) {
  const { t } = useI18n();
  return (
    <div className={cn('text-left', className)} data-testid="routes-start-checklist">
      {showTitle ? <p className="text-meta text-secondary">{t('routes.start.title')}</p> : null}
      <ol className="mt-1 list-decimal space-y-1 pl-5 text-meta text-secondary">
        {ROUTES_START_STEPS.map((step) => {
          const isDone = done?.[step.id] === true;
          return (
            <li key={step.id} className={isDone ? 'text-muted' : undefined}>
              <Link to={step.to} className="hover:text-primary">
                {t(step.labelKey)}
              </Link>
              {isDone ? (
                <Check
                  className="ml-1 inline h-3.5 w-3.5 text-success"
                  aria-label={t('routes.start.done')}
                />
              ) : null}
            </li>
          );
        })}
      </ol>
    </div>
  );
}
