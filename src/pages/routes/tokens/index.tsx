import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { KeyRound } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { PageRefreshButton } from '@/components/shared/PageRefreshButton';
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
  const {
    profiles,
    bridgeStatuses,
    errors,
    profileState,
    loading,
    reload,
  } = useAdapterResources();
  const [revealedTokenId, setRevealedTokenId] = useState<string | null>(null);
  const rows = useMemo(
    () => {
      const next = buildLocalTokenRows(profiles, bridgeStatuses, errors.bridgeStatuses);
      if (profileState !== 'error') return next;
      return next.map((row) => ({
        ...row,
        token: null,
        maskedToken: null,
        unavailable: true,
      }));
    },
    [bridgeStatuses, errors.bridgeStatuses, profileState, profiles],
  );

  useEffect(() => {
    if (!revealedTokenId) return undefined;
    const timer = window.setTimeout(() => setRevealedTokenId(null), 8_000);
    return () => window.clearTimeout(timer);
  }, [revealedTokenId]);

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
          <PageRefreshButton
            loading={loading}
            onClick={() => void reload()}
            label={t('routes.board.refresh')}
          />
        </div>
      </div>

      {profileState === 'error' && rows.length === 0 && !loading ? (
        <ErrorState
          title={t('routes.runtime.unavailable')}
          error={errors.profiles ?? new Error(t('routes.runtime.unavailable'))}
          onRetry={() => void reload()}
        />
      ) : rows.length === 0 && !loading ? (
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
            const displayedToken = token
              ? revealedTokenId === row.profileId ? token : row.maskedToken
              : null;
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
                      {row.unavailable ? (
                        <span className="text-meta text-muted">{t('routes.runtime.unavailable')}</span>
                      ) : displayedToken ? (
                          <span className="min-w-0 truncate font-mono text-meta text-secondary">
                          {displayedToken}
                        </span>
                      ) : (
                        <span className="text-meta text-muted">{t('routes.tokens.noToken')}</span>
                      )}
                    </>
                  }
                  actions={
                    token && !row.unavailable ? (
                      <div className="flex items-center gap-1.5">
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => setRevealedTokenId((current) => (
                            current === row.profileId ? null : row.profileId
                          ))}
                        >
                          {revealedTokenId === row.profileId
                            ? t('common.hideSecret')
                            : t('common.showSecret')}
                        </Button>
                        <Button variant="outline" size="sm" onClick={() => copyToken(token)}>
                          {t('routes.tokens.copy')}
                        </Button>
                      </div>
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
