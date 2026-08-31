import { useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { KeyRound, Loader2 } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ListRow, ListRowBody, LIST_ROW_PAD } from '@/components/shared/ListRow';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/toast';
import { ROUTES_POOL_PATH } from '@/lib/bridges-path';
import { useAdapterResources } from '@/pages/bridges/use-bridge-resources';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import { buildLocalTokenRows } from './tokens-model';

export default function RoutesTokensPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const { toast } = useToast();
  const { profiles, bridgeStatuses, loading, reload } = useAdapterResources();
  const rows = useMemo(
    () => buildLocalTokenRows(profiles, bridgeStatuses),
    [profiles, bridgeStatuses],
  );

  const copyToken = (token: string) => {
    void navigator.clipboard.writeText(token).then(
      () => toast({ title: t('routes.tokens.copied'), variant: 'success' }),
      () => toast({ title: t('routes.tokens.copyFailed'), variant: 'danger' }),
    );
  };

  return (
    <RoutesPane>
      <PageHeader
        title={t('routes.tokens.title')}
        description={t('routes.tokens.description')}
      />
      <div className={pageRhythm.chromeRow}>
        <p className="min-w-0 truncate text-meta text-muted">{t('routes.tokens.scopeNote')}</p>
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

      {rows.length === 0 && !loading ? (
        <EmptyState
          icon={KeyRound}
          title={t('routes.tokens.emptyTitle')}
          description={t('routes.tokens.emptyDescription')}
          action={
            <Button
              size="sm"
              variant="outline"
              className="mt-2"
              onClick={() => navigate(ROUTES_POOL_PATH)}
            >
              {t('routes.nav.goToList')}
            </Button>
          }
        />
      ) : (
        <div className={pageRhythm.stackDense}>
          {rows.map((row) => {
            const token = row.token;
            return (
              <ListRow key={row.profileId} className={LIST_ROW_PAD}>
                <ListRowBody
                  main={
                    <>
                      <span className="min-w-0 truncate text-sm font-medium text-primary">
                        {row.name}
                      </span>
                      <span className="font-mono text-meta text-muted">
                        {row.endpoint ?? t('routes.pendingPort')}
                      </span>
                      {token ? (
                        <span className="min-w-0 truncate font-mono text-meta text-secondary">
                          {token}
                        </span>
                      ) : (
                        <span className="text-meta text-muted">{t('routes.tokens.noToken')}</span>
                      )}
                    </>
                  }
                  actions={
                    token ? (
                      <Button variant="outline" size="sm" onClick={() => copyToken(token)}>
                        {t('routes.tokens.copy')}
                      </Button>
                    ) : null
                  }
                />
              </ListRow>
            );
          })}
        </div>
      )}
    </RoutesPane>
  );
}
