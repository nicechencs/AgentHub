import type { TranslateFn } from '@/lib/i18n';
import { useMemo } from 'react';
import { Loader2, Users } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { useRoutePoolState } from '@/pages/bridges/use-route-pool-state';
import {
  defaultPoolEntryUrl,
  routePoolMemberLabels,
  routePoolSurfaceLabel,
} from '@/pages/bridges/route-pool-view-model';
import { RoutesPane } from '@/pages/routes/RoutesPane';

function memberAvailabilityLabel(
  availability: string | undefined,
  enabled: boolean,
  t: TranslateFn,
): string {
  if (!enabled || availability === 'disabled') return t('routes.pool.availabilityDisabled');
  if (availability === 'cooling') return t('routes.pool.availabilityCooling');
  if (availability === 'isolated') return t('routes.pool.availabilityIsolated');
  return t('routes.pool.availabilityReady');
}

export default function RoutesPoolPage() {
  const { t } = useI18n();
  const { profiles, entries, loading, reload } = useAdapterResources();
  const { routePoolV2, defaultPools } = useRoutePoolState({
    profiles,
    detailTarget: null,
  });

  const pools = useMemo(() => defaultPools, [defaultPools]);

  return (
    <RoutesPane>
      <PageHeader
        title={t('routes.pool.page.title')}
        description={t('routes.pool.page.description')}
      />
      <div className={pageRhythm.chromeRow}>
        <p className="min-w-0 truncate text-meta text-muted">
          {t('routes.pool.page.chromeHint')}
        </p>
        <div className={pageRhythm.chromeActions}>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={loading}
            onClick={() => void reload()}
          >
            {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
            {t('routes.board.refresh')}
          </Button>
        </div>
      </div>

      {!routePoolV2 ? (
        <EmptyState
          icon={Users}
          title={t('routes.pool.page.disabledTitle')}
          description={t('routes.pool.page.disabledDescription')}
        />
      ) : pools.length === 0 ? (
        <EmptyState
          icon={Users}
          title={t('routes.pool.page.emptyTitle')}
          description={t('routes.pool.page.emptyDescription')}
        />
      ) : (
        <div className={pageRhythm.blocks}>
          {pools.map((pool) => {
            const entry = defaultPoolEntryUrl(pool.gatewayPort);
            const members = routePoolMemberLabels(pool.members, entries);
            return (
              <PageSection
                key={pool.id}
                title={`${pool.targetAgentId} · ${routePoolSurfaceLabel(pool.surface, t)}`}
              >
                <div className="rounded-card border border-border bg-panel p-3 space-y-3">
                  <p className="text-meta text-muted">
                    {entry.url ?? t('routes.pool.entryPending')}
                  </p>
                  <ul className={pageRhythm.stackDense}>
                    {members.length === 0 ? (
                      <li className="text-sm text-muted">{t('routes.pool.page.noMembers')}</li>
                    ) : (
                      members.map((member) => (
                        <li
                          key={`${member.sourceKind}:${member.sourceId}`}
                          className="flex min-w-0 flex-wrap items-center gap-x-2 text-sm"
                        >
                          <span className="truncate font-medium">{member.title}</span>
                          <span className="text-meta text-muted">
                            {memberAvailabilityLabel(member.availability, member.enabled, t)}
                          </span>
                        </li>
                      ))
                    )}
                  </ul>
                  {pool.listedModels && pool.listedModels.length > 0 ? (
                    <details className="text-meta">
                      <summary className="cursor-pointer text-secondary">
                        {t('routes.capabilities.models')}
                      </summary>
                      <p className="mt-1 text-muted">{pool.listedModels.join(', ')}</p>
                    </details>
                  ) : null}
                </div>
              </PageSection>
            );
          })}
        </div>
      )}
    </RoutesPane>
  );
}
