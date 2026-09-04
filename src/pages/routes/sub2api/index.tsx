import * as React from 'react';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { useI18n } from '@/components/shared/LanguageProvider';
import { PageRefreshButton } from '@/components/shared/PageRefreshButton';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Skeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName } from '@/config/agents';
import {
  createSub2ApiKey,
  establishSessionFromTokens,
  loadSub2ApiKeys,
  loadSub2ApiSession,
  logoutSub2Api,
  closeSub2ApiLoginWindow,
  openSub2ApiLoginWindow,
  probeSub2ApiPublicSettings,
  saveSub2ApiSession,
  SUB2API_DEFAULT_SITE_URL,
  sub2apiLoginUrl,
  syncSub2ApiKeyToConnections,
  type Sub2ApiKey,
  type Sub2ApiSession,
} from '@/lib/api/sub2api';
import { openExternalLink } from '@/lib/open-external';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { selectableSub2ApiKeys } from '@/lib/sub2api/client';
import { maskApiKey } from '@/lib/sub2api/url';
import type { AgentKey } from '@/lib/types';
import { cn } from '@/lib/utils';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import {
  initialSiteUrlDraft,
  prepareSiteUrlForLogin,
  sortSub2ApiKeys,
  sub2apiDisplayName,
  sub2apiKeyStatusLabel,
  sub2apiPagePhase,
} from './sub2api-page-model';

export default function RoutesSub2ApiPage() {
  const { t } = useI18n();
  const { toast } = useToast();
  const { installedIds } = useInstalledAgents();

  const [session, setSession] = React.useState<Sub2ApiSession | null>(() => loadSub2ApiSession());
  const [siteUrlDraft, setSiteUrlDraft] = React.useState(() =>
    initialSiteUrlDraft(loadSub2ApiSession()),
  );
  const [loggingIn, setLoggingIn] = React.useState(false);
  const [pasteToken, setPasteToken] = React.useState('');
  const [keys, setKeys] = React.useState<Sub2ApiKey[]>([]);
  const [loadingKeys, setLoadingKeys] = React.useState(false);
  const [creating, setCreating] = React.useState(false);
  const [newKeyName, setNewKeyName] = React.useState('AgentHub');
  const [createOpen, setCreateOpen] = React.useState(false);
  const [syncingId, setSyncingId] = React.useState<number | null>(null);
  const loginAbortRef = React.useRef(false);

  const phase = sub2apiPagePhase(session, loggingIn);
  const sortedKeys = React.useMemo(() => sortSub2ApiKeys(selectableSub2ApiKeys(keys)), [keys]);
  const syncAgents = React.useMemo(() => [...installedIds], [installedIds]);

  const applySession = React.useCallback((next: Sub2ApiSession) => {
    saveSub2ApiSession(next);
    setSession(next);
    setSiteUrlDraft(next.siteUrl);
  }, []);

  const refreshKeys = React.useCallback(
    async (active: Sub2ApiSession) => {
      setLoadingKeys(true);
      try {
        setKeys(await loadSub2ApiKeys(active));
      } catch {
        toast({ title: t('routes.sub2api.loadKeysFailed'), variant: 'danger' });
        setKeys([]);
      } finally {
        setLoadingKeys(false);
      }
    },
    [t, toast],
  );

  React.useEffect(() => {
    if (session?.accessToken) void refreshKeys(session);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session?.accessToken, session?.siteUrl]);

  const finishWithTokens = React.useCallback(
    async (input: { siteUrl: string; accessToken: string; refreshToken?: string; expiresAt?: number }) => {
      try {
        const next = await establishSessionFromTokens(input);
        applySession(next);
        setLoggingIn(false);
        setPasteToken('');
        await refreshKeys(next);
      } catch {
        toast({ title: t('routes.sub2api.sessionExpired'), variant: 'danger' });
      }
    },
    [applySession, refreshKeys, t, toast],
  );

  const startLogin = async () => {
    const siteUrl = prepareSiteUrlForLogin(siteUrlDraft || SUB2API_DEFAULT_SITE_URL);
    setSiteUrlDraft(siteUrl);
    loginAbortRef.current = false;
    setLoggingIn(true);
    setPasteToken('');
    try {
      await probeSub2ApiPublicSettings(siteUrl);
    } catch {
      toast({ title: t('routes.sub2api.siteProbeFailed'), variant: 'danger' });
    }
    try {
      const tokens = await openSub2ApiLoginWindow(sub2apiLoginUrl(siteUrl));
      if (loginAbortRef.current) return;
      await finishWithTokens({
        siteUrl,
        accessToken: tokens.accessToken,
        refreshToken: tokens.refreshToken,
        expiresAt: tokens.expiresAt,
      });
    } catch (e) {
      if (loginAbortRef.current) return;
      const msg = e instanceof Error ? e.message : String(e);
      if (/cancelled/i.test(msg)) return;
      toast({ title: t('routes.sub2api.loginFailed'), variant: 'danger' });
    }
  };

  const cancelLogin = () => {
    loginAbortRef.current = true;
    setLoggingIn(false);
    setPasteToken('');
    void closeSub2ApiLoginWindow();
  };

  const submitPasteToken = async () => {
    const token = pasteToken.trim();
    if (!token) return;
    await finishWithTokens({
      siteUrl: prepareSiteUrlForLogin(siteUrlDraft || SUB2API_DEFAULT_SITE_URL),
      accessToken: token,
    });
  };

  const openLoginInBrowser = async () => {
    try {
      await openExternalLink(
        sub2apiLoginUrl(prepareSiteUrlForLogin(siteUrlDraft || SUB2API_DEFAULT_SITE_URL)),
      );
    } catch {
      toast({ title: t('routes.sub2api.loginFailed'), variant: 'danger' });
    }
  };

  const onLogout = async () => {
    await logoutSub2Api(session);
    setSession(null);
    setKeys([]);
  };

  const onCreateKey = async (alsoSync = false) => {
    if (!session) return;
    setCreating(true);
    try {
      const created = await createSub2ApiKey(session, newKeyName.trim() || 'AgentHub');
      setCreateOpen(false);
      setKeys((prev) => [...prev, created]);
      if (alsoSync) {
        const agentId = syncAgents[0];
        if (agentId) await syncKey(created, agentId);
      }
    } catch {
      toast({ title: t('routes.sub2api.createKeyFailed'), variant: 'danger' });
    } finally {
      setCreating(false);
    }
  };

  const syncKey = async (key: Sub2ApiKey, agentId: AgentKey) => {
    if (!session) return;
    setSyncingId(key.id);
    try {
      const result = await syncSub2ApiKeyToConnections({
        gatewayBaseUrl: session.gatewayBaseUrl,
        apiKey: key.key,
        name: key.name || `Sub2API #${key.id}`,
        agentId,
      });
      toast({
        title: result.ok ? t('routes.sub2api.syncDone') : t('routes.sub2api.syncFailed'),
        variant: result.ok ? 'default' : 'danger',
      });
    } catch {
      toast({ title: t('routes.sub2api.syncFailed'), variant: 'danger' });
    } finally {
      setSyncingId(null);
    }
  };

  const userLabel = sub2apiDisplayName(session?.user, session);

  return (
    <RoutesPane>
      <div className={cn(pageRhythm.stack, 'min-h-0 flex-1')}>
        <PageHeader
          title={t('routes.sub2api.title')}
          description={
            phase === 'logged-in'
              ? [t('routes.sub2api.userLabel'), userLabel || null, session?.siteUrl || null]
                  .filter(Boolean)
                  .join(' · ')
              : t('routes.sub2api.description')
          }
          descriptionTip={t('routes.sub2api.descriptionTip')}
        />
        {phase === 'logged-in' ? (
          <div className={pageRhythm.chromeRow}>
            <div className="min-w-0" />
            <div className={pageRhythm.chromeActions}>
              <PageRefreshButton
                onClick={() => session && void refreshKeys(session)}
                loading={loadingKeys}
                label={t('routes.sub2api.refresh')}
              />
              <Button type="button" variant="outline" size="sm" onClick={() => void onLogout()}>
                {t('routes.sub2api.logout')}
              </Button>
            </div>
          </div>
        ) : null}

        {phase === 'logged-out' && (
          <Card className="mx-auto w-full max-w-lg space-y-4 p-5">
            <div>
              <h2 className="text-base font-medium">{t('routes.sub2api.loggedOutTitle')}</h2>
              <p className="mt-1 text-sm text-secondary">{t('routes.sub2api.loggedOutDescription')}</p>
            </div>
            <label className="block space-y-1.5">
              <span className="text-sm text-secondary">{t('routes.sub2api.siteUrlLabel')}</span>
              <Input
                value={siteUrlDraft}
                onChange={(e) => setSiteUrlDraft(e.target.value)}
                placeholder={t('routes.sub2api.siteUrlPlaceholder')}
                autoComplete="url"
              />
            </label>
            <Button type="button" onClick={() => void startLogin()}>
              {t('routes.sub2api.login')}
            </Button>
            <p className="text-sm text-secondary">{t('routes.sub2api.syncedKeysEmpty')}</p>
          </Card>
        )}

        {phase === 'logged-in' && (
          <div className="flex min-h-0 flex-1 flex-col gap-3 lg:flex-row">
            <Card className="flex min-h-0 flex-1 flex-col overflow-hidden p-0">
              <div className="flex items-center justify-between gap-2 border-b border-border px-4 py-3">
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium">{t('routes.sub2api.keysTitle')}</div>
                  {userLabel ? (
                    <div className="truncate text-xs text-secondary">
                      {t('routes.sub2api.userLabel')} · {userLabel}
                    </div>
                  ) : null}
                </div>
                <Button type="button" size="sm" variant="outline" onClick={() => setCreateOpen(true)}>
                  {t('routes.sub2api.createKey')}
                </Button>
              </div>
              <div className="min-h-0 flex-1 overflow-auto">
                {loadingKeys ? (
                  <div className="space-y-2 p-4">
                    <Skeleton className="h-10 w-full" />
                    <Skeleton className="h-10 w-full" />
                  </div>
                ) : sortedKeys.length === 0 ? (
                  <div className="p-6 text-sm text-secondary">
                    <div className="font-medium text-primary">{t('routes.sub2api.keysEmpty')}</div>
                    <p className="mt-1">{t('routes.sub2api.keysEmptyHint')}</p>
                  </div>
                ) : (
                  <ul className="divide-y divide-border">
                    {sortedKeys.map((key) => (
                      <li key={key.id} className="flex flex-wrap items-center gap-2 px-4 py-3">
                        <div className="min-w-0 flex-1">
                          <div className="truncate text-sm font-medium">
                            {key.name || `Key #${key.id}`}
                          </div>
                          <div className="truncate font-mono text-xs text-secondary">
                            {maskApiKey(key.key)}
                          </div>
                        </div>
                        <Badge variant="default">
                          {sub2apiKeyStatusLabel(key.status, {
                            active: t('routes.sub2api.statusActive'),
                            other: t('routes.sub2api.statusOther'),
                          })}
                        </Badge>
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <Button
                              type="button"
                              size="sm"
                              variant="outline"
                              disabled={syncingId === key.id}
                            >
                              {t('routes.sub2api.syncToConnections')}
                            </Button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            {syncAgents.length === 0 ? (
                              <DropdownMenuItem disabled>
                                {t('routes.sub2api.syncPickAgent')}
                              </DropdownMenuItem>
                            ) : (
                              syncAgents.map((agentId) => (
                                <DropdownMenuItem
                                  key={agentId}
                                  onClick={() => void syncKey(key, agentId)}
                                >
                                  {agentDisplayName(agentId)}
                                </DropdownMenuItem>
                              ))
                            )}
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </Card>
            <Card className="flex w-full shrink-0 flex-col gap-3 p-4 lg:w-72">
              <div className="text-sm font-medium">{t('routes.sub2api.detailTitle')}</div>
              <div className="space-y-1">
                <div className="text-xs text-secondary">{t('routes.sub2api.siteUrlLabel')}</div>
                <div className="break-all text-xs text-secondary">{session?.siteUrl}</div>
              </div>
              <div className="space-y-1">
                <div className="text-xs text-secondary">API</div>
                <div className="break-all text-xs text-secondary">{session?.gatewayBaseUrl}</div>
              </div>
              <Button
                type="button"
                variant="outline"
                className="mt-auto w-full"
                onClick={() => setCreateOpen(true)}
              >
                {t('routes.sub2api.createAndSync')}
              </Button>
            </Card>
          </div>
        )}
      </div>

      <Dialog open={phase === 'logging-in'} onOpenChange={(open) => { if (!open) cancelLogin(); }}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{t('routes.sub2api.loggingInTitle')}</DialogTitle>
            <DialogDescription>{t('routes.sub2api.loggingInDescription')}</DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            <label className="block space-y-1.5">
              <span className="text-sm text-secondary">{t('routes.sub2api.loginUrlLabel')}</span>
              <Input
                value={sub2apiLoginUrl(prepareSiteUrlForLogin(siteUrlDraft || SUB2API_DEFAULT_SITE_URL))}
                readOnly
                className="font-mono text-xs"
              />
            </label>
            <Button type="button" variant="outline" className="w-full" onClick={() => void openLoginInBrowser()}>
              {t('routes.sub2api.openBrowser')}
            </Button>
            <p className="text-xs text-secondary">{t('routes.sub2api.pasteTokenHint')}</p>
            <label className="block space-y-1.5">
              <span className="text-sm text-secondary">{t('routes.sub2api.pasteTokenLabel')}</span>
              <Input
                value={pasteToken}
                onChange={(e) => setPasteToken(e.target.value)}
                placeholder={t('routes.sub2api.pasteTokenPlaceholder')}
                autoComplete="off"
                spellCheck={false}
              />
            </label>
          </div>
          <DialogFooter className="gap-2 sm:gap-2">
            <Button type="button" variant="ghost" onClick={cancelLogin}>
              {t('routes.sub2api.cancelLogin')}
            </Button>
            <Button type="button" onClick={() => void submitPasteToken()} disabled={!pasteToken.trim()}>
              {t('routes.sub2api.pasteTokenConfirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('routes.sub2api.createKey')}</DialogTitle>
          </DialogHeader>
          <label className="block space-y-1.5">
            <span className="text-sm text-secondary">{t('routes.sub2api.createKeyName')}</span>
            <Input value={newKeyName} onChange={(e) => setNewKeyName(e.target.value)} />
          </label>
          <DialogFooter className="gap-2 sm:gap-2">
            <Button type="button" variant="outline" onClick={() => void onCreateKey(false)} disabled={creating}>
              {t('routes.sub2api.createKeyConfirm')}
            </Button>
            <Button type="button" onClick={() => void onCreateKey(true)} disabled={creating || syncAgents.length === 0}>
              {t('routes.sub2api.createAndSync')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </RoutesPane>
  );
}
