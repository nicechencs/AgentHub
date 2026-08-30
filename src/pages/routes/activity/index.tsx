import { useMemo, useState } from 'react';
import { Activity, Boxes, Loader2 } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { InboundRequestList } from '@/components/shared/InboundRequestList';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { BRIDGES_PATH } from '@/lib/bridges-path';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import {
  buildInboundFeed,
  type InboundFeedFilter,
} from '@/pages/routes/activity/inbound-feed-model';

export default function RoutesActivityPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [filter, setFilter] = useState<InboundFeedFilter>('all');
  const {
    profiles,
    bridgeStatuses,
    profileState,
    errors,
    loading,
    reload,
  } = useAdapterResources();

  const bridges = useMemo(
    () => profiles.filter((profile) => profile.route === 'local_bridge'),
    [profiles],
  );
  const feed = useMemo(
    () => buildInboundFeed(profiles, bridgeStatuses, filter, 20),
    [profiles, bridgeStatuses, filter],
  );
  const allCount = useMemo(
    () => buildInboundFeed(profiles, bridgeStatuses, 'all', 100).length,
    [profiles, bridgeStatuses],
  );
  const failedCount = useMemo(
    () => buildInboundFeed(profiles, bridgeStatuses, 'failed', 100).length,
    [profiles, bridgeStatuses],
  );

  return (
    <RoutesPane>
      <PageHeader
        title={t('routes.activity.title')}
        description={t('routes.activity.description')}
      />
      <div className={pageRhythm.chromeRow}>
        <SegmentedControl
          aria-label={t('routes.activity.filterAria')}
          value={filter}
          onChange={setFilter}
          options={[
            { value: 'all', label: t('routes.activity.filterAll'), count: allCount },
            { value: 'failed', label: t('routes.activity.filterFailed'), count: failedCount },
          ]}
        />
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

      <p className="mb-3 text-meta text-muted">{t('routes.activity.scopeNote')}</p>

      {profileState === 'error' ? (
        <ErrorState
          title={t('routes.loadError')}
          error={errors.profiles ?? t('routes.loadError')}
          onRetry={() => void reload()}
        />
      ) : bridges.length === 0 && !loading ? (
        <EmptyState
          icon={Boxes}
          title={t('routes.board.emptyTitle')}
          description={t('routes.board.emptyDescription')}
          action={
            <Button
              size="sm"
              variant="outline"
              className="mt-2"
              onClick={() => navigate(BRIDGES_PATH)}
            >
              {t('routes.nav.goToList')}
            </Button>
          }
        />
      ) : feed.length === 0 ? (
        <EmptyState
          icon={Activity}
          title={t('routes.activity.emptyTitle')}
          description={t('routes.activity.emptyDescription')}
        />
      ) : (
        <InboundRequestList rows={feed} />
      )}
    </RoutesPane>
  );
}
