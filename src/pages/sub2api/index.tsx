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
  isTotp2FARequired,
  loadSub2ApiKeys,
  loadSub2ApiSession,
  logoutSub2Api,
  nativeSub2ApiLogin,
  nativeSub2ApiLogin2FA,
  probeSub2ApiPublicSettings,
  saveSub2ApiSession,
  SUB2API_DEFAULT_SITE_URL,
  syncSub2ApiKeyToConnections,
  type Sub2ApiCaptchaProof,
  type Sub2ApiKey,
  type Sub2ApiPublicSettings,
  type Sub2ApiSession,
} from '@/lib/api/sub2api';
import { openExternalLink } from '@/lib/open-external';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import {
  mapSub2ApiLoginError,
  resolveCaptchaKind,
  selectableSub2ApiKeys,
} from '@/lib/sub2api/client';
import { maskApiKey } from '@/lib/sub2api/url';
import type { AgentKey } from '@/lib/types';
import { cn } from '@/lib/utils';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import { Sub2ApiCaptcha, type Sub2ApiCaptchaHandle } from './Sub2ApiCaptcha';
import {
  initialSiteUrlDraft,
  normalizeTotpCode,
  prepareSiteUrlForLogin,
  sortSub2ApiKeys,
  sub2apiDisplayName,
  sub2apiKeyStatusLabel,
  sub2apiPagePhase,
} from './sub2api-page-model';

export default function Sub2ApiPage() {
  const { t, lang } = useI18n();
  const { toast } = useToast();
  const { installedIds } = useInstalledAgents();

  const [session, setSession] = React.useState<Sub2ApiSession | null>(() => loadSub2ApiSession());
  const [siteUrlDraft, setSiteUrlDraft] = React.useState(() =>
    initialSiteUrlDraft(loadSub2ApiSession()),
  );
  const [email, setEmail] = React.useState('');
  const [password, setPassword] = React.useState('');
  const [totpCode, setTotpCode] = React.useState('');
  const [tempToken, setTempToken] = React.useState<string | null>(null);
  const [maskedEmail, setMaskedEmail] = React.useState<string | null>(null);
  const [publicSettings, setPublicSettings] = React.useState<Sub2ApiPublicSettings | null>(null);
  const [captchaProof, setCaptchaProof] = React.useState<Sub2ApiCaptchaProof | null>(null);
  const [submitting, setSubmitting] = React.useState(false);
  const [pasteToken, setPasteToken] = React.useState('');
  const [advancedOpen, setAdvancedOpen] = React.useState(false);
  const [keys, setKeys] = React.useState<Sub2ApiKey[]>([]);
  const [loadingKeys, setLoadingKeys] = React.useState(false);
  const [creating, setCreating] = React.useState(false);
  const [newKeyName, setNewKeyName] = React.useState('AgentHub');
  const [createOpen, setCreateOpen] = React.useState(false);
  const [syncingId, setSyncingId] = React.useState<number | null>(null);
  const captchaRef = React.useRef<Sub2ApiCaptchaHandle>(null);

  const awaiting2fa = Boolean(tempToken);
  const phase = sub2apiPagePhase(session, awaiting2fa);
  const sortedKeys = React.useMemo(() => sortSub2ApiKeys(selectableSub2ApiKeys(keys)), [keys]);
  const syncAgents = React.useMemo(() => [...installedIds], [installedIds]);
  const langZh = lang === 'zh';

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
    async (input: {
      siteUrl: string;
      accessToken: string;
      refreshToken?: string;
      expiresAt?: number;
      expiresIn?: number;
      user?: Sub2ApiSession['user'];
    }) => {
      try {
        const next = await establishSessionFromTokens(input);
        applySession(next);
        setPassword('');
        setTotpCode('');
        setTempToken(null);
        setMaskedEmail(null);
        setPasteToken('');
        setCaptchaProof(null);
        captchaRef.current?.reset();
        await refreshKeys(next);
      } catch {
        toast({ title: t('routes.sub2api.sessionExpired'), variant: 'danger' });
      }
    },
    [applySession, refreshKeys, t, toast],
  );

  const probeSite = React.useCallback(
    async (siteUrl: string) => {
      try {
        const pub = await probeSub2ApiPublicSettings(siteUrl);
        setPublicSettings(pub);
        return pub;
      } catch {
        setPublicSettings(null);
        toast({ title: t('routes.sub2api.siteProbeFailed'), variant: 'danger' });
        return null;
      }
    },
    [t, toast],
  );

  React.useEffect(() => {
    if (phase === 'logged-in') return;
    const siteUrl = prepareSiteUrlForLogin(siteUrlDraft || SUB2API_DEFAULT_SITE_URL);
    const handle = window.setTimeout(() => {
      void probeSite(siteUrl);
    }, 400);
    return () => window.clearTimeout(handle);
  }, [siteUrlDraft, phase, probeSite]);

  const onNativeLogin = async (e?: React.FormEvent) => {
    e?.preventDefault();
    const siteUrl = prepareSiteUrlForLogin(siteUrlDraft || SUB2API_DEFAULT_SITE_URL);
    setSiteUrlDraft(siteUrl);
    if (!email.trim() || !password) {
      toast({ title: t('routes.sub2api.loginNeedCredentials'), variant: 'danger' });
      return;
    }
    setSubmitting(true);
    try {
      let settings = publicSettings;
      if (!settings) {
        settings = await probeSite(siteUrl);
      }
      let proof = captchaProof;
      const captchaKind =
        captchaRef.current?.kind() ?? resolveCaptchaKind(settings);
      const ensured = await captchaRef.current?.ensureProof();
      if (captchaKind !== 'none') {
        const nextProof = ensured ?? null;
        const hasProof = Boolean(
          nextProof?.turnstile_token?.trim()
            || (
              nextProof?.tencent_captcha_ticket?.trim()
              && nextProof?.tencent_captcha_randstr?.trim()
            ),
        );
        if (!hasProof) {
          toast({ title: t('routes.sub2api.captchaRequired'), variant: 'danger' });
          return;
        }
        proof = nextProof;
      }
      const result = await nativeSub2ApiLogin({
        siteUrl,
        email: email.trim(),
        password,
        captcha: proof,
      });
      if (isTotp2FARequired(result)) {
        setTempToken(result.temp_token || null);
        setMaskedEmail(result.user_email_masked || email.trim());
        setTotpCode('');
        return;
      }
      if (!result.access_token) {
        toast({ title: t('routes.sub2api.loginFailed'), variant: 'danger' });
        return;
      }
      await finishWithTokens({
        siteUrl,
        accessToken: result.access_token,
        refreshToken: result.refresh_token,
        expiresIn: result.expires_in,
        user: result.user,
      });
    } catch (err) {
      toast({
        title: mapSub2ApiLoginError(err, {
          captchaVerificationFailed: t('routes.sub2api.captchaVerificationFailed'),
          loginBadCredentials: t('routes.sub2api.loginBadCredentials'),
          loginFailed: t('routes.sub2api.loginFailed'),
          siteUnreachable: t('routes.sub2api.siteProbeFailed'),
        }),
        variant: 'danger',
      });
      captchaRef.current?.reset();
      setCaptchaProof(null);
    } finally {
      setSubmitting(false);
    }
  };

  const onSubmit2FA = async (e?: React.FormEvent) => {
    e?.preventDefault();
    if (!tempToken) return;
    const code = normalizeTotpCode(totpCode);
    if (code.length !== 6) {
      toast({ title: t('routes.sub2api.totpInvalid'), variant: 'danger' });
      return;
    }
    const siteUrl = prepareSiteUrlForLogin(siteUrlDraft || SUB2API_DEFAULT_SITE_URL);
    setSubmitting(true);
    try {
      const result = await nativeSub2ApiLogin2FA({
        siteUrl,
        tempToken,
        totpCode: code,
      });
      await finishWithTokens({
        siteUrl,
        accessToken: result.access_token,
        refreshToken: result.refresh_token,
        expiresIn: result.expires_in,
        user: result.user,
      });
    } catch (err) {
      toast({
        title: mapSub2ApiLoginError(err, {
          captchaVerificationFailed: t('routes.sub2api.captchaVerificationFailed'),
          loginBadCredentials: t('routes.sub2api.loginBadCredentials'),
          loginFailed: t('routes.sub2api.totpFailed'),
          siteUnreachable: t('routes.sub2api.siteProbeFailed'),
        }),
        variant: 'danger',
      });
    } finally {
      setSubmitting(false);
    }
  };

  const cancel2FA = () => {
    setTempToken(null);
    setMaskedEmail(null);
    setTotpCode('');
  };

  const submitPasteToken = async () => {
    const token = pasteToken.trim();
    if (!token) return;
    setSubmitting(true);
    try {
      await finishWithTokens({
        siteUrl: prepareSiteUrlForLogin(siteUrlDraft || SUB2API_DEFAULT_SITE_URL),
        accessToken: token,
      });
    } finally {
      setSubmitting(false);
    }
  };

  const openSiteInBrowser = async () => {
    try {
      await openExternalLink(prepareSiteUrlForLogin(siteUrlDraft || SUB2API_DEFAULT_SITE_URL));
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
  const captchaLabels = {
    turnstileLoading: t('routes.sub2api.captchaLoading'),
    turnstileFailed: t('routes.sub2api.captchaLoadFailed'),
    actionReady: t('routes.sub2api.captchaActionReady'),
    actionVerified: t('routes.sub2api.captchaVerified'),
    actionFailed: t('routes.sub2api.captchaLoadFailed'),
    actionNeeded: t('routes.sub2api.captchaClickToVerify'),
  };

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

        {(phase === 'logged-out' || phase === 'awaiting-2fa') && (
          <Card className="mx-auto w-full max-w-lg space-y-4 p-5" data-sub2api-login-form="">
            <div>
              <h2 className="text-base font-medium">
                {phase === 'awaiting-2fa'
                  ? t('routes.sub2api.totpTitle')
                  : t('routes.sub2api.loggedOutTitle')}
              </h2>
              <p className="mt-1 text-sm text-secondary">
                {phase === 'awaiting-2fa'
                  ? t('routes.sub2api.totpDescription', {
                      email: maskedEmail || email || '—',
                    })
                  : t('routes.sub2api.loggedOutDescription')}
              </p>
            </div>

            {phase === 'logged-out' ? (
              <form className="space-y-3" onSubmit={(ev) => void onNativeLogin(ev)}>
                <label className="block space-y-1.5">
                  <span className="text-sm text-secondary">{t('routes.sub2api.siteUrlLabel')}</span>
                  <Input
                    value={siteUrlDraft}
                    onChange={(e) => setSiteUrlDraft(e.target.value)}
                    placeholder={t('routes.sub2api.siteUrlPlaceholder')}
                    autoComplete="url"
                    data-sub2api-site-url=""
                  />
                </label>
                <label className="block space-y-1.5">
                  <span className="text-sm text-secondary">{t('routes.sub2api.emailLabel')}</span>
                  <Input
                    type="email"
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    placeholder={t('routes.sub2api.emailPlaceholder')}
                    autoComplete="username"
                    data-sub2api-email=""
                  />
                </label>
                <label className="block space-y-1.5">
                  <span className="text-sm text-secondary">{t('routes.sub2api.passwordLabel')}</span>
                  <Input
                    type="password"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    placeholder={t('routes.sub2api.passwordPlaceholder')}
                    autoComplete="current-password"
                    data-sub2api-password=""
                  />
                </label>
                <Sub2ApiCaptcha
                  ref={captchaRef}
                  settings={publicSettings}
                  langZh={langZh}
                  labels={captchaLabels}
                  onProofChange={setCaptchaProof}
                />
                <Button
                  type="submit"
                  className="w-full"
                  disabled={submitting}
                  data-sub2api-login-submit=""
                >
                  {submitting ? t('routes.sub2api.loggingIn') : t('routes.sub2api.login')}
                </Button>
              </form>
            ) : (
              <form className="space-y-3" onSubmit={(ev) => void onSubmit2FA(ev)} data-sub2api-2fa-form="">
                <label className="block space-y-1.5">
                  <span className="text-sm text-secondary">{t('routes.sub2api.totpLabel')}</span>
                  <Input
                    inputMode="numeric"
                    pattern="[0-9]*"
                    maxLength={6}
                    value={totpCode}
                    onChange={(e) => setTotpCode(normalizeTotpCode(e.target.value))}
                    placeholder={t('routes.sub2api.totpPlaceholder')}
                    autoComplete="one-time-code"
                    data-sub2api-totp=""
                  />
                </label>
                <div className="flex gap-2">
                  <Button type="button" variant="ghost" className="flex-1" onClick={cancel2FA}>
                    {t('routes.sub2api.cancelLogin')}
                  </Button>
                  <Button
                    type="submit"
                    className="flex-1"
                    disabled={submitting || normalizeTotpCode(totpCode).length !== 6}
                    data-sub2api-2fa-submit=""
                  >
                    {submitting ? t('routes.sub2api.loggingIn') : t('routes.sub2api.totpConfirm')}
                  </Button>
                </div>
              </form>
            )}

            <div className="space-y-2 border-t border-border pt-3">
              <Button
                type="button"
                variant="outline"
                className="w-full"
                onClick={() => void openSiteInBrowser()}
                data-sub2api-open-site=""
              >
                {t('routes.sub2api.openSiteInBrowser')}
              </Button>
              <details
                className="group rounded-card border border-border bg-subtle/60"
                open={advancedOpen}
                onToggle={(e) => setAdvancedOpen((e.target as HTMLDetailsElement).open)}
                data-sub2api-advanced=""
              >
                <summary className="cursor-pointer list-none px-3 py-2 text-xs font-medium text-secondary marker:content-none [&::-webkit-details-marker]:hidden">
                  {t('routes.sub2api.advancedPasteTitle')}
                </summary>
                <div className="space-y-2 border-t border-border px-3 py-3">
                  <p className="text-xs text-secondary">{t('routes.sub2api.pasteTokenHint')}</p>
                  <label className="block space-y-1.5">
                    <span className="text-sm text-secondary">{t('routes.sub2api.pasteTokenLabel')}</span>
                    <Input
                      value={pasteToken}
                      onChange={(e) => setPasteToken(e.target.value)}
                      placeholder={t('routes.sub2api.pasteTokenPlaceholder')}
                      autoComplete="off"
                      spellCheck={false}
                      data-sub2api-paste-token=""
                    />
                  </label>
                  <Button
                    type="button"
                    size="sm"
                    onClick={() => void submitPasteToken()}
                    disabled={!pasteToken.trim() || submitting}
                  >
                    {t('routes.sub2api.pasteTokenConfirm')}
                  </Button>
                </div>
              </details>
            </div>
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
