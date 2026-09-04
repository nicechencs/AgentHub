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
  clearAllRememberedAccountsAsync,
  clearAllRememberedPasswordsAsync,
  createSub2ApiKey,
  deleteRememberedAccountAsync,
  ensureSub2ApiSessionFresh,
  establishSessionFromTokens,
  getLastUsedRememberedAccount,
  hydrateRememberedPasswordVault,
  isSub2ApiRememberEnabled,
  isTotp2FARequired,
  listRememberedAccounts,
  loadRememberedCredentials,
  loadSub2ApiKeys,
  loadSub2ApiSession,
  logoutSub2Api,
  nativeSub2ApiLogin,
  nativeSub2ApiLogin2FA,
  probeSub2ApiPublicSettings,
  refreshSub2ApiSession,
  saveRememberedAccountAsync,
  saveSub2ApiSession,
  setSub2ApiRememberEnabled,
  SUB2API_DEFAULT_SITE_URL,
  syncSub2ApiKeyToConnections,
  type Sub2ApiCaptchaProof,
  type Sub2ApiKey,
  type Sub2ApiPublicSettings,
  type Sub2ApiRememberedAccountMeta,
  type Sub2ApiSession,
} from '@/lib/api/sub2api';
import { openExternalLink } from '@/lib/open-external';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import {
  mapSub2ApiLoginError,
  resolveCaptchaKind,
  selectableSub2ApiKeys,
  Sub2ApiError,
} from '@/lib/sub2api/client';
import { maskApiKey, maskEmail } from '@/lib/sub2api/url';
import { Switch } from '@/components/ui/switch';
import type { AgentKey } from '@/lib/types';
import { cn } from '@/lib/utils';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import { Sub2ApiCaptcha, type Sub2ApiCaptchaHandle } from './Sub2ApiCaptcha';
import {
  applySiteUrlDraftInput,
  formatKeyModelsFromKey,
  sub2apiKeyStatusBadgeVariant,
  formatKeyQuota,
  formatKeyTimestamp,
  initialSiteUrlDraft,
  normalizeTotpCode,
  pickGroupLabel,
  prepareSiteUrlForLogin,
  sortSub2ApiKeys,
  sub2apiDisplayName,
  sub2apiKeyStatusKind,
  sub2apiKeyStatusLabel,
  sub2apiPagePhase,
} from './sub2api-page-model';

export default function Sub2ApiPage() {
  const { t, lang } = useI18n();
  const { toast } = useToast();
  const { installedIds } = useInstalledAgents();

  const [session, setSession] = React.useState<Sub2ApiSession | null>(() => loadSub2ApiSession());
  const [restoring, setRestoring] = React.useState(() => Boolean(loadSub2ApiSession()?.accessToken));
  const [siteUrlDraft, setSiteUrlDraft] = React.useState(() => {
    const existing = loadSub2ApiSession();
    if (existing?.siteUrl) return initialSiteUrlDraft(existing);
    const last = getLastUsedRememberedAccount();
    return last?.siteUrl || SUB2API_DEFAULT_SITE_URL;
  });
  const [email, setEmail] = React.useState(() => getLastUsedRememberedAccount()?.email ?? '');
  const [password, setPassword] = React.useState(() => {
    const last = getLastUsedRememberedAccount();
    if (!last) return '';
    return loadRememberedCredentials(last.id)?.password ?? '';
  });
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
  const [rememberEnabled, setRememberEnabled] = React.useState(() => isSub2ApiRememberEnabled());
  const [remembered, setRemembered] = React.useState<Sub2ApiRememberedAccountMeta[]>(() =>
    listRememberedAccounts(),
  );
  const [rememberOffOpen, setRememberOffOpen] = React.useState(false);
  const [pendingDeleteId, setPendingDeleteId] = React.useState<string | null>(null);
  const [forgetAllOpen, setForgetAllOpen] = React.useState(false);
  const captchaRef = React.useRef<Sub2ApiCaptchaHandle>(null);
  const loginPasswordRef = React.useRef(password);

  React.useEffect(() => {
    loginPasswordRef.current = password;
  }, [password]);

  const refreshRemembered = React.useCallback(() => {
    setRemembered(listRememberedAccounts());
  }, []);

  const awaiting2fa = Boolean(tempToken);
  const phase = sub2apiPagePhase(session, awaiting2fa, restoring);
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
      } catch (err) {
        const unauthorized =
          err instanceof Sub2ApiError && (err.status === 401 || err.code === 401);
        if (unauthorized && active.refreshToken) {
          try {
            const next = await refreshSub2ApiSession(active);
            applySession(next);
            setKeys(await loadSub2ApiKeys(next));
            return;
          } catch {
            await logoutSub2Api(active);
            setSession(null);
            setKeys([]);
            const last = getLastUsedRememberedAccount();
            if (last) {
              const creds = loadRememberedCredentials(last.id);
              setSiteUrlDraft(last.siteUrl);
              setEmail(last.email);
              setPassword(creds?.password ?? '');
            }
            toast({ title: t('routes.sub2api.sessionExpired'), variant: 'danger' });
            return;
          }
        }
        toast({ title: t('routes.sub2api.loadKeysFailed'), variant: 'danger' });
        setKeys([]);
      } finally {
        setLoadingKeys(false);
      }
    },
    [applySession, t, toast],
  );

  React.useEffect(() => {
    if (restoring) return;
    if (session?.accessToken) void refreshKeys(session);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session?.accessToken, session?.siteUrl, restoring]);

  React.useEffect(() => {
    let cancelled = false;
    const boot = async () => {
      await hydrateRememberedPasswordVault();
      if (cancelled) return;
      const last = getLastUsedRememberedAccount();
      if (last) {
        const creds = loadRememberedCredentials(last.id);
        setSiteUrlDraft((prev) => prev || last.siteUrl);
        setEmail((prev) => prev || last.email);
        if (creds?.password) setPassword((prev) => prev || creds.password);
        refreshRemembered();
      }
      const existing = loadSub2ApiSession();
      if (!existing?.accessToken) {
        if (!cancelled) setRestoring(false);
        return;
      }
      try {
        const fresh = await ensureSub2ApiSessionFresh(existing);
        if (cancelled) return;
        if (fresh) {
          applySession(fresh);
          await refreshKeys(fresh);
        } else {
          setSession(null);
          setKeys([]);
          if (last) {
            const creds = loadRememberedCredentials(last.id);
            setSiteUrlDraft(last.siteUrl);
            setEmail(last.email);
            setPassword(creds?.password ?? '');
          }
          toast({ title: t('routes.sub2api.sessionExpired'), variant: 'danger' });
        }
      } finally {
        if (!cancelled) setRestoring(false);
      }
    };
    void boot();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const finishWithTokens = React.useCallback(
    async (input: {
      siteUrl: string;
      accessToken: string;
      refreshToken?: string;
      expiresAt?: number;
      expiresIn?: number;
      user?: Sub2ApiSession['user'];
      rememberEmail?: string;
      rememberPassword?: string;
    }) => {
      try {
        const next = await establishSessionFromTokens(input);
        const emailForSave = (input.rememberEmail || input.user?.email || email || '').trim();
        const passwordForSave = input.rememberPassword ?? loginPasswordRef.current;
        if (rememberEnabled && emailForSave && passwordForSave) {
          await saveRememberedAccountAsync({
            siteUrl: input.siteUrl,
            email: emailForSave,
            password: passwordForSave,
          });
          refreshRemembered();
        }
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
    [applySession, email, refreshKeys, refreshRemembered, rememberEnabled, t, toast],
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
        rememberEmail: email.trim(),
        rememberPassword: password,
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

  const applySiteUrlFromInput = React.useCallback(
    (raw: string, opts?: { fromPaste?: boolean }) => {
      const { draft, result } = applySiteUrlDraftInput(raw);
      if (!result) return;
      if (!result.ok) {
        toast({
          title:
            result.reason === 'empty'
              ? t('routes.sub2api.urlEmpty')
              : t('routes.sub2api.urlInvalid'),
          variant: 'danger',
        });
        return;
      }
      setSiteUrlDraft(result.url);
      if (result.stripped) {
        toast({ title: t('routes.sub2api.urlNormalizedHint') });
      } else if (opts?.fromPaste && draft && draft !== raw.trim()) {
        /* normalized host only — silent */
      }
    },
    [t, toast],
  );

  const fillRememberedAccount = React.useCallback((id: string) => {
    const creds = loadRememberedCredentials(id);
    if (!creds) return;
    setSiteUrlDraft(creds.siteUrl);
    setEmail(creds.email);
    setPassword(creds.password);
  }, []);

  const onRememberToggle = (next: boolean) => {
    if (next) {
      setSub2ApiRememberEnabled(true);
      setRememberEnabled(true);
      return;
    }
    setRememberOffOpen(true);
  };

  const confirmRememberOff = (clearPasswords: boolean) => {
    setSub2ApiRememberEnabled(false);
    setRememberEnabled(false);
    setRememberOffOpen(false);
    if (clearPasswords) {
      void clearAllRememberedPasswordsAsync().then(() => {
        toast({ title: t('routes.sub2api.passwordsCleared') });
      });
    }
  };

  const confirmDeleteRemembered = () => {
    if (!pendingDeleteId) return;
    const id = pendingDeleteId;
    setPendingDeleteId(null);
    void deleteRememberedAccountAsync(id).then(() => {
      refreshRemembered();
      toast({ title: t('routes.sub2api.rememberedDeleted') });
    });
  };

  const confirmForgetAll = () => {
    setForgetAllOpen(false);
    void clearAllRememberedAccountsAsync().then(() => {
      refreshRemembered();
      toast({ title: t('routes.sub2api.rememberedCleared') });
    });
  };

  const onLogout = async () => {
    await logoutSub2Api(session);
    setSession(null);
    setKeys([]);
    toast({ title: t('routes.sub2api.logoutKeepsConnections') });
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

        {phase === 'restoring' ? (
          <Card className="mx-auto w-full max-w-lg space-y-3 p-5" data-sub2api-restoring="">
            <div className="text-sm font-medium">{t('routes.sub2api.sessionRestoring')}</div>
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-24 w-full" />
          </Card>
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
                    onBlur={(e) => applySiteUrlFromInput(e.target.value)}
                    onPaste={(e) => {
                      const pasted = e.clipboardData.getData('text');
                      if (!pasted.trim()) return;
                      e.preventDefault();
                      applySiteUrlFromInput(pasted, { fromPaste: true });
                    }}
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
                <div
                  className="flex items-start justify-between gap-3 rounded-card border border-border bg-subtle/50 px-3 py-2.5"
                  data-sub2api-remember=""
                >
                  <div className="min-w-0 space-y-0.5">
                    <div className="text-sm font-medium">{t('routes.sub2api.rememberLabel')}</div>
                    <p className="text-xs text-secondary">{t('routes.sub2api.rememberHint')}</p>
                  </div>
                  <Switch
                    checked={rememberEnabled}
                    onCheckedChange={onRememberToggle}
                    aria-label={t('routes.sub2api.rememberLabel')}
                  />
                </div>
                {remembered.length > 0 ? (
                  <div className="space-y-2" data-sub2api-remembered-list="">
                    <div className="flex items-center justify-between gap-2">
                      <div className="text-sm font-medium">
                        {t('routes.sub2api.rememberedAccountsTitle')}
                      </div>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="h-7 px-2 text-xs"
                        onClick={() => setForgetAllOpen(true)}
                      >
                        {t('routes.sub2api.rememberedForgetAll')}
                      </Button>
                    </div>
                    <ul className="divide-y divide-border rounded-card border border-border">
                      {remembered.map((row) => (
                        <li
                          key={row.id}
                          className="flex flex-wrap items-center gap-2 px-3 py-2"
                          data-sub2api-remembered-row=""
                        >
                          <div className="min-w-0 flex-1">
                            <div className="truncate text-sm">{maskEmail(row.email)}</div>
                            <div className="truncate text-xs text-secondary">{row.siteUrl}</div>
                          </div>
                          <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => fillRememberedAccount(row.id)}
                          >
                            {t('routes.sub2api.rememberedUseAccount')}
                          </Button>
                          <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={() => setPendingDeleteId(row.id)}
                          >
                            {t('routes.sub2api.rememberedDeleteAccount')}
                          </Button>
                        </li>
                      ))}
                    </ul>
                  </div>
                ) : null}
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
                    {sortedKeys.map((key) => {
                      const statusKind = sub2apiKeyStatusKind(key.status);
                      const createdLabel = formatKeyTimestamp(key.created_at);
                      const updatedLabel = formatKeyTimestamp(key.updated_at);
                      const lastUsedLabel = formatKeyTimestamp(key.last_used_at);
                      const expiresLabel = formatKeyTimestamp(key.expires_at);
                      const groupLabel = pickGroupLabel(key);
                      const quotaLabel = formatKeyQuota(key, {
                        unlimited: t('routes.sub2api.quotaUnlimited'),
                      });
                      const modelsLabel = formatKeyModelsFromKey(key);
                      const metaItems: { label: string; value: string }[] = [];
                      if (createdLabel) {
                        metaItems.push({
                          label: t('routes.sub2api.keyCreated'),
                          value: createdLabel,
                        });
                      }
                      if (updatedLabel) {
                        metaItems.push({
                          label: t('routes.sub2api.keyUpdated'),
                          value: updatedLabel,
                        });
                      }
                      if (lastUsedLabel) {
                        metaItems.push({
                          label: t('routes.sub2api.keyLastUsed'),
                          value: lastUsedLabel,
                        });
                      }
                      if (expiresLabel) {
                        metaItems.push({
                          label: t('routes.sub2api.keyExpires'),
                          value: expiresLabel,
                        });
                      }
                      if (groupLabel) {
                        metaItems.push({
                          label: t('routes.sub2api.keyGroup'),
                          value: groupLabel,
                        });
                      }
                      if (quotaLabel) {
                        metaItems.push({
                          label: t('routes.sub2api.keyQuota'),
                          value: quotaLabel,
                        });
                      }
                      if (modelsLabel) {
                        metaItems.push({
                          label: t('routes.sub2api.keyModels'),
                          value: modelsLabel,
                        });
                      }
                      return (
                      <li key={key.id} className="flex flex-wrap items-start gap-2 px-4 py-3">
                        <div className="min-w-0 flex-1 space-y-1">
                          <div className="truncate text-sm font-medium">
                            {key.name || `Key #${key.id}`}
                          </div>
                          <div className="truncate font-mono text-xs text-secondary">
                            {maskApiKey(key.key)}
                          </div>
                          {metaItems.length > 0 ? (
                            <div className="flex flex-wrap gap-x-3 gap-y-1 pt-0.5">
                              {metaItems.map((item) => (
                                <div
                                  key={`${key.id}-${item.label}`}
                                  className="max-w-full text-xs text-secondary"
                                >
                                  <span>{item.label}</span>
                                  <span className="mx-1">·</span>
                                  <span className="break-all text-primary">{item.value}</span>
                                </div>
                              ))}
                            </div>
                          ) : null}
                        </div>
                        <Badge variant={sub2apiKeyStatusBadgeVariant(statusKind)}>
                          {sub2apiKeyStatusLabel(key.status, {
                            active: t('routes.sub2api.statusActive'),
                            disabled: t('routes.sub2api.statusDisabled'),
                            expired: t('routes.sub2api.statusExpired'),
                            quotaExhausted: t('routes.sub2api.statusQuotaExhausted'),
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
                      );
                    })}
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

      <Dialog open={rememberOffOpen} onOpenChange={setRememberOffOpen}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('routes.sub2api.rememberOffClearPasswordsTitle')}</DialogTitle>
          </DialogHeader>
          <p className="text-sm text-secondary">{t('routes.sub2api.rememberOffClearPasswordsBody')}</p>
          <DialogFooter className="gap-2 sm:gap-2">
            <Button type="button" variant="outline" onClick={() => confirmRememberOff(false)}>
              {t('routes.sub2api.rememberOffKeepPasswords')}
            </Button>
            <Button type="button" onClick={() => confirmRememberOff(true)}>
              {t('routes.sub2api.rememberOffClearPasswordsConfirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={pendingDeleteId != null}
        onOpenChange={(open) => {
          if (!open) setPendingDeleteId(null);
        }}
      >
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('routes.sub2api.rememberedDeleteConfirm')}</DialogTitle>
          </DialogHeader>
          <DialogFooter className="gap-2 sm:gap-2">
            <Button type="button" variant="outline" onClick={() => setPendingDeleteId(null)}>
              {t('routes.sub2api.cancelLogin')}
            </Button>
            <Button type="button" variant="danger" onClick={confirmDeleteRemembered}>
              {t('routes.sub2api.rememberedDeleteAccount')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={forgetAllOpen} onOpenChange={setForgetAllOpen}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('routes.sub2api.rememberedForgetAllConfirm')}</DialogTitle>
          </DialogHeader>
          <DialogFooter className="gap-2 sm:gap-2">
            <Button type="button" variant="outline" onClick={() => setForgetAllOpen(false)}>
              {t('routes.sub2api.cancelLogin')}
            </Button>
            <Button type="button" variant="danger" onClick={confirmForgetAll}>
              {t('routes.sub2api.rememberedForgetAll')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </RoutesPane>
  );
}
