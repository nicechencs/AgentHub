import * as React from 'react';
import { ChevronDown, Copy } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { useI18n } from '@/components/shared/LanguageProvider';
import { copyTextToClipboard } from '@/components/shared/CopyTextButton';
import { PageRefreshButton } from '@/components/shared/PageRefreshButton';
import { ApiKeyAccountDialog } from '@/components/connections/ApiKeyAccountDialog';
import { ProviderEditDialog } from '@/components/connections/ProviderEditDialog';
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Skeleton } from '@/components/ui/skeleton';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableHeaderRow,
  TableRow,
  TableShell,
} from '@/components/ui/table';
import { Hint } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import { agentDisplayName } from '@/config/agents';
import {
  clearAllRememberedAccountsAsync,
  clearAllRememberedPasswordsAsync,
  createSub2ApiKey,
  deleteSub2ApiKey,
  deleteRememberedAccountAsync,
  deleteRememberedSite,
  ensureSub2ApiSessionFresh,
  establishSessionFromTokens,
  getLastUsedRememberedAccount,
  hydrateRememberedPasswordVault,
  isSub2ApiRememberEnabled,
  isTotp2FARequired,
  listRememberedAccounts,
  listRememberedSites,
  loadRememberedCredentials,
  loadSub2ApiGroups,
  loadSub2ApiKeys,
  loadSub2ApiSession,
  logoutSub2Api,
  nativeSub2ApiLogin,
  nativeSub2ApiLogin2FA,
  probeSub2ApiPublicSettings,
  refreshSub2ApiSession,
  saveRememberedAccountAsync,
  saveRememberedSite,
  saveSub2ApiSession,
  seedRememberedSitesIfUnset,
  setSub2ApiRememberEnabled,
  SUB2API_DEFAULT_SITE_URL,
  updateSub2ApiKey,
  updateSub2ApiKeyGroup,
  type Sub2ApiCaptchaProof,
  type Sub2ApiGroup,
  type Sub2ApiKey,
  type Sub2ApiPublicSettings,
  type Sub2ApiRememberedAccountMeta,
  type Sub2ApiSession,
} from '@/lib/api/sub2api';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import {
  mapSub2ApiLoginError,
  resolveCaptchaKind,
  selectableSub2ApiKeys,
  Sub2ApiError,
} from '@/lib/sub2api/client';
import { maskEmail } from '@/lib/sub2api/url';
import { Switch } from '@/components/ui/switch';
import type { ConnectApiKeyDraft } from '@/lib/connect-flow/connect-intent';
import type { AgentKey } from '@/lib/types';
import { cn } from '@/lib/utils';
import { applyImportedLogin } from '@/pages/routes/tokens/token-import-action';
import type { TokenImportAgentRef } from '@/pages/routes/tokens/token-import-model';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import { Sub2ApiCaptcha, type Sub2ApiCaptchaHandle } from './Sub2ApiCaptcha';
import { Sub2ApiGroupCell } from './Sub2ApiGroupCell';
import { Sub2ApiKeyActions } from './Sub2ApiKeyActions';
import { Sub2ApiKeyEditDialog } from './Sub2ApiKeyEditDialog';
import { buildEditPatch, type Sub2ApiKeyForm } from './sub2api-key-form';
import {
  applyGroupToKey,
  applySiteUrlDraftInput,
  formatKeyExpires,
  formatKeyTableTimestamp,
  formatUsdAmount,
  initialSiteUrlDraft,
  keyMatchesGroupFilter,
  maskSub2ApiTableKey,
  mergeSub2ApiGroups,
  mergeUpdatedSub2ApiKey,
  nextSub2ApiKeyToggleStatus,
  normalizeTotpCode,
  parseGroupFilter,
  pickGroupId,
  pickGroupLabel,
  pickGroupRate,
  pickKeyConcurrency,
  pickKeyUsageUsd,
  prepareSiteUrlForLogin,
  sortSub2ApiKeys,
  sub2apiDisplayName,
  sub2apiKeyStatusBadgeVariant,
  sub2apiKeyStatusKind,
  sub2apiKeyStatusLabel,
  sub2apiPagePhase,
  type Sub2ApiGroupFilter,
} from './sub2api-page-model';

function CopyableEndpointChip({
  label,
  value,
  copyAria,
  onCopy,
}: {
  label: string;
  value: string;
  copyAria: string;
  onCopy: (value: string) => void;
}) {
  return (
    <Hint label={value}>
      <button
        type="button"
        className="inline-flex min-w-0 max-w-[min(28rem,36vw)] items-center gap-1.5 rounded-full border border-border bg-panel px-2.5 py-1 text-left text-xs text-secondary hover:border-accent/40 hover:bg-hover"
        aria-label={copyAria}
        onClick={() => onCopy(value)}
      >
        <span className="shrink-0">{label}</span>
        <span className="min-w-0 truncate font-mono text-primary">{value}</span>
        <Copy className="h-3 w-3 shrink-0 text-muted" aria-hidden />
      </button>
    </Hint>
  );
}

export default function Sub2ApiPage() {
  const { t, lang } = useI18n();
  const { toast } = useToast();
  const { installedAgents } = useInstalledAgents();

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
  const [keys, setKeys] = React.useState<Sub2ApiKey[]>([]);
  const [availableGroups, setAvailableGroups] = React.useState<Sub2ApiGroup[]>([]);
  const [groupFilter, setGroupFilter] = React.useState<Sub2ApiGroupFilter>('all');
  const [loadingKeys, setLoadingKeys] = React.useState(false);
  const [creating, setCreating] = React.useState(false);
  const [newKeyName, setNewKeyName] = React.useState('AgentHub');
  const [newKeyGroupId, setNewKeyGroupId] = React.useState<number | null>(null);
  const [createOpen, setCreateOpen] = React.useState(false);
  const [editingKey, setEditingKey] = React.useState<Sub2ApiKey | null>(null);
  const [pendingDeleteKey, setPendingDeleteKey] = React.useState<Sub2ApiKey | null>(null);
  const [actingKeyId, setActingKeyId] = React.useState<number | null>(null);
  const [importSession, setImportSession] = React.useState<{
    agentId: AgentKey;
    draft: ConnectApiKeyDraft;
  } | null>(null);
  const [updatingGroupId, setUpdatingGroupId] = React.useState<number | null>(null);
  const [rememberEnabled, setRememberEnabled] = React.useState(() => isSub2ApiRememberEnabled());
  const [remembered, setRemembered] = React.useState<Sub2ApiRememberedAccountMeta[]>(() =>
    listRememberedAccounts(),
  );
  const [rememberedSites, setRememberedSites] = React.useState<string[]>(() => listRememberedSites());
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
    setRememberedSites(listRememberedSites());
  }, []);

  const awaiting2fa = Boolean(tempToken);
  const phase = sub2apiPagePhase(session, awaiting2fa, restoring);
  const groups = React.useMemo(
    () => mergeSub2ApiGroups(availableGroups, keys),
    [availableGroups, keys],
  );
  const sortedKeys = React.useMemo(() => {
    const rows = sortSub2ApiKeys(selectableSub2ApiKeys(keys));
    return rows.filter((key) => keyMatchesGroupFilter(key, groupFilter));
  }, [groupFilter, keys]);
  const importAgents = React.useMemo<TokenImportAgentRef[]>(
    () => installedAgents.map((agent) => ({ id: agent.id, name: agent.name })),
    [installedAgents],
  );
  const langZh = lang === 'zh';

  const applySession = React.useCallback((next: Sub2ApiSession) => {
    saveSub2ApiSession(next);
    setSession(next);
    setSiteUrlDraft(next.siteUrl);
  }, []);

  const loadGroups = React.useCallback(async (active: Sub2ApiSession) => {
    try {
      setAvailableGroups(await loadSub2ApiGroups(active));
    } catch {
      setAvailableGroups([]);
    }
  }, []);

  const refreshKeys = React.useCallback(
    async (active: Sub2ApiSession) => {
      setLoadingKeys(true);
      try {
        const [nextKeys] = await Promise.all([loadSub2ApiKeys(active), loadGroups(active)]);
        setKeys(nextKeys);
      } catch (err) {
        const unauthorized =
          err instanceof Sub2ApiError && (err.status === 401 || err.code === 401);
        if (unauthorized && active.refreshToken) {
          try {
            const next = await refreshSub2ApiSession(active);
            applySession(next);
            const [nextKeys] = await Promise.all([loadSub2ApiKeys(next), loadGroups(next)]);
            setKeys(nextKeys);
            return;
          } catch {
            await logoutSub2Api(active);
            setSession(null);
            setKeys([]);
            setAvailableGroups([]);
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
    [applySession, loadGroups, t, toast],
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
      const existingForSeed = loadSub2ApiSession();
      seedRememberedSitesIfUnset([
        ...listRememberedAccounts().map((row) => row.siteUrl),
        existingForSeed?.siteUrl ?? '',
      ]);
      refreshRemembered();
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
          setAvailableGroups([]);
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
        saveRememberedSite(input.siteUrl);
        if (rememberEnabled && emailForSave && passwordForSave) {
          await saveRememberedAccountAsync({
            siteUrl: input.siteUrl,
            email: emailForSave,
            password: passwordForSave,
          });
        }
        refreshRemembered();
        applySession(next);
        setPassword('');
        setTotpCode('');
        setTempToken(null);
        setMaskedEmail(null);
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
      saveRememberedSite(result.url);
      refreshRemembered();
      if (result.stripped) {
        toast({ title: t('routes.sub2api.urlNormalizedHint') });
      } else if (opts?.fromPaste && draft && draft !== raw.trim()) {
        /* normalized host only — silent */
      }
    },
    [refreshRemembered, t, toast],
  );

  const fillRememberedAccount = React.useCallback((id: string) => {
    const creds = loadRememberedCredentials(id);
    if (!creds) return;
    setSiteUrlDraft(creds.siteUrl);
    setEmail(creds.email);
    setPassword(creds.password);
  }, []);

  const fillRememberedSite = React.useCallback(
    (siteUrl: string) => {
      setSiteUrlDraft(siteUrl);
      saveRememberedSite(siteUrl);
      refreshRemembered();
      const match = listRememberedAccounts().find((row) => row.siteUrl === siteUrl);
      if (match) fillRememberedAccount(match.id);
    },
    [fillRememberedAccount, refreshRemembered],
  );

  const onDeleteRememberedSite = React.useCallback(
    (siteUrl: string) => {
      deleteRememberedSite(siteUrl);
      refreshRemembered();
      toast({ title: t('routes.sub2api.rememberedSiteDeleted') });
    },
    [refreshRemembered, t, toast],
  );

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
    setAvailableGroups([]);
    toast({ title: t('routes.sub2api.logoutKeepsConnections') });
  };

  const toastSub2ApiFailure = (err: unknown, fallback: string) => {
    const message =
      err instanceof Sub2ApiError && err.message.trim() ? err.message.trim() : fallback;
    toast({ title: message, variant: 'danger' });
  };

  const applyEditedKey = (
    key: Sub2ApiKey,
    updated: Sub2ApiKey,
    groupId: number | null | undefined,
  ) => {
    const group =
      groupId == null ? null : (groups.find((row) => row.id === groupId) ?? null);
    const next = (row: Sub2ApiKey) => {
      if (row.id !== key.id) return row;
      const merged = mergeUpdatedSub2ApiKey(row, updated);
      if (groupId == null) return applyGroupToKey(merged, null);
      if (group) return applyGroupToKey(merged, group);
      return merged;
    };
    setKeys((prev) => prev.map(next));
    setEditingKey((prev) => (prev && prev.id === key.id ? next(prev) : prev));
  };

  const onCreateKey = async () => {
    if (!session) return;
    if (groups.length > 0 && newKeyGroupId == null) {
      toast({ title: t('routes.sub2api.groupRequired'), variant: 'danger' });
      return;
    }
    setCreating(true);
    try {
      const created = await createSub2ApiKey(
        session,
        newKeyName.trim() || 'AgentHub',
        newKeyGroupId,
      );
      setCreateOpen(false);
      setKeys((prev) => [...prev, created]);
    } catch (err) {
      toastSub2ApiFailure(err, t('routes.sub2api.createKeyFailed'));
    } finally {
      setCreating(false);
    }
  };

  const onSaveEditedKey = async (form: Sub2ApiKeyForm) => {
    if (!session || !editingKey) return;
    if (groups.length > 0 && form.groupId == null) {
      toast({ title: t('routes.sub2api.groupRequired'), variant: 'danger' });
      return;
    }
    setCreating(true);
    try {
      const patch = buildEditPatch(form, editingKey);
      const updated = await updateSub2ApiKey(session, editingKey.id, patch);
      applyEditedKey(editingKey, { ...updated, name: patch.name ?? editingKey.name }, patch.group_id);
      setEditingKey(null);
      toast({ title: t('routes.sub2api.keySaved'), variant: 'success' });
    } catch (err) {
      toastSub2ApiFailure(err, t('routes.sub2api.saveKeyFailed'));
    } finally {
      setCreating(false);
    }
  };

  const onResetQuota = async () => {
    if (!session || !editingKey) return;
    setCreating(true);
    try {
      const updated = await updateSub2ApiKey(session, editingKey.id, { reset_quota: true });
      applyEditedKey(editingKey, { ...updated, quota_used: 0, used_quota: 0 }, pickGroupId(editingKey));
      toast({ title: t('routes.sub2api.quotaReset'), variant: 'success' });
    } catch (err) {
      toastSub2ApiFailure(err, t('routes.sub2api.quotaResetFailed'));
    } finally {
      setCreating(false);
    }
  };

  const onResetRateLimit = async () => {
    if (!session || !editingKey) return;
    setCreating(true);
    try {
      const updated = await updateSub2ApiKey(session, editingKey.id, { reset_rate_limit_usage: true });
      applyEditedKey(
        editingKey,
        { ...updated, usage_5h: 0, usage_1d: 0, usage_7d: 0 },
        pickGroupId(editingKey),
      );
      toast({ title: t('routes.sub2api.rateLimitReset'), variant: 'success' });
    } catch (err) {
      toastSub2ApiFailure(err, t('routes.sub2api.rateLimitResetFailed'));
    } finally {
      setCreating(false);
    }
  };

  const onToggleKeyStatus = async (key: Sub2ApiKey) => {
    if (!session) return;
    const status = nextSub2ApiKeyToggleStatus(key.status);
    setActingKeyId(key.id);
    try {
      const updated = await updateSub2ApiKey(session, key.id, { status });
      setKeys((prev) =>
        prev.map((row) =>
          row.id === key.id ? { ...mergeUpdatedSub2ApiKey(row, updated), status } : row,
        ),
      );
      toast({
        title:
          status === 'active'
            ? t('routes.sub2api.keyEnabled')
            : t('routes.sub2api.keyDisabled'),
        variant: 'success',
      });
    } catch (err) {
      toastSub2ApiFailure(err, t('routes.sub2api.statusChangeFailed'));
    } finally {
      setActingKeyId(null);
    }
  };

  const openEditKey = (key: Sub2ApiKey) => {
    setCreateOpen(false);
    setEditingKey(key);
  };

  const onDeleteKey = async () => {
    if (!session || !pendingDeleteKey) return;
    const target = pendingDeleteKey;
    setActingKeyId(target.id);
    try {
      await deleteSub2ApiKey(session, target.id);
      setKeys((prev) => prev.filter((row) => row.id !== target.id));
      setPendingDeleteKey(null);
      toast({ title: t('routes.sub2api.keyDeleted'), variant: 'success' });
    } catch (err) {
      toastSub2ApiFailure(err, t('routes.sub2api.deleteKeyFailed'));
    } finally {
      setActingKeyId(null);
    }
  };

  const startImport = (agentId: AgentKey, draft: ConnectApiKeyDraft) => {
    setImportSession({ agentId, draft });
  };

  const onChangeGroup = async (key: Sub2ApiKey, groupId: number | null) => {
    if (!session) return;
    if (pickGroupId(key) === groupId) return;
    const group = groupId == null ? null : (groups.find((row) => row.id === groupId) ?? null);
    setUpdatingGroupId(key.id);
    try {
      const updated = await updateSub2ApiKeyGroup(session, key.id, groupId);
      setKeys((prev) => prev.map((row) => (
        row.id === key.id ? applyGroupToKey({ ...row, ...updated }, group) : row
      )));
      toast({ title: t('routes.sub2api.groupChanged'), variant: 'success' });
    } catch {
      toast({ title: t('routes.sub2api.groupChangeFailed'), variant: 'danger' });
    } finally {
      setUpdatingGroupId(null);
    }
  };

  const finishImportedLogin = async (
    sourceKind: 'provider' | 'account',
    sourceId: string,
    isCurrent: boolean,
  ) => {
    const agentId = importSession?.agentId;
    setImportSession(null);
    if (!agentId) return;
    try {
      await applyImportedLogin({ agentId, sourceKind, sourceId, isCurrent });
      toast({
        title: t('routes.tokens.importSuccess', { name: agentDisplayName(agentId) }),
        variant: 'success',
      });
    } catch {
      toast({ title: t('routes.tokens.importFailed'), variant: 'danger' });
    }
  };

  const copyKeySecret = (value: string) => {
    void copyTextToClipboard(value).then(
      () => toast({ title: t('common.copied'), variant: 'success' }),
      () => toast({ title: t('common.copyFailed'), variant: 'danger' }),
    );
  };

  const userLabel = sub2apiDisplayName(session?.user, session);
  const siteOrigin = React.useMemo(() => {
    const raw = session?.siteUrl?.trim() ?? '';
    if (!raw) return '';
    try {
      return new URL(raw).origin;
    } catch {
      return raw.replace(/\/+$/, '');
    }
  }, [session?.siteUrl]);
  const gatewayOrigin = React.useMemo(() => {
    const raw = session?.gatewayBaseUrl?.trim() ?? '';
    if (!raw) return '';
    try {
      return new URL(raw).origin;
    } catch {
      return raw.replace(/\/+$/, '');
    }
  }, [session?.gatewayBaseUrl]);
  const showDirectLine = Boolean(gatewayOrigin && siteOrigin && gatewayOrigin !== siteOrigin);
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
              ? [t('routes.sub2api.userLabel'), userLabel || null].filter(Boolean).join(' · ')
              : t('routes.sub2api.description')
          }
          descriptionTip={t('routes.sub2api.descriptionTip')}
        />
        {phase === 'logged-in' ? (
          <div className={cn(pageRhythm.chromeRow, 'flex-nowrap')}>
            <div className="flex min-w-0 items-center gap-2 overflow-hidden">
              <div className="flex min-w-0 items-center gap-2" data-sub2api-endpoints="">
                {session?.siteUrl ? (
                  <CopyableEndpointChip
                    label={t('routes.sub2api.apiEndpointLabel')}
                    value={session.siteUrl}
                    copyAria={t('routes.sub2api.copyEndpoint')}
                    onCopy={copyKeySecret}
                  />
                ) : null}
                {showDirectLine && session?.gatewayBaseUrl ? (
                  <CopyableEndpointChip
                    label={t('routes.sub2api.directLineLabel')}
                    value={session.gatewayBaseUrl}
                    copyAria={t('routes.sub2api.copyDirectLine')}
                    onCopy={copyKeySecret}
                  />
                ) : null}
              </div>
              <Select
                value={typeof groupFilter === 'number' ? String(groupFilter) : groupFilter}
                onValueChange={(value) => setGroupFilter(parseGroupFilter(value))}
              >
                <SelectTrigger className="w-44 shrink-0" aria-label={t('routes.sub2api.selectGroup')} data-sub2api-group-filter="">
                  <SelectValue placeholder={t('routes.sub2api.selectGroup')} />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">{t('routes.sub2api.allGroups')}</SelectItem>
                  {groups.map((group) => (
                    <SelectItem key={group.id} value={String(group.id)}>
                      {group.name}
                    </SelectItem>
                  ))}
                  <SelectItem value="none">{t('routes.sub2api.groupNone')}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className={pageRhythm.chromeActions}>
              <PageRefreshButton
                onClick={() => session && void refreshKeys(session)}
                loading={loadingKeys}
                label={t('routes.sub2api.refresh')}
              />
              <Button
                type="button"
                size="sm"
                onClick={() => {
                  setEditingKey(null);
                  setNewKeyName('AgentHub');
                  setNewKeyGroupId(
                    typeof groupFilter === 'number' ? groupFilter : (groups[0]?.id ?? null),
                  );
                  setCreateOpen(true);
                }}
              >
                {t('routes.sub2api.createKey')}
              </Button>
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
                <div className="block space-y-1.5">
                  <span className="text-sm text-secondary">{t('routes.sub2api.siteUrlLabel')}</span>
                  <div className="flex gap-1.5" data-sub2api-site-picker="">
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
                      className="min-w-0 flex-1"
                      data-sub2api-site-url=""
                    />
                    {rememberedSites.length > 0 ? (
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild>
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            className="h-7 w-7 shrink-0 px-0"
                            aria-label={t('routes.sub2api.rememberedSitesLabel')}
                            data-sub2api-site-picker-trigger=""
                          >
                            <ChevronDown className="h-3.5 w-3.5" />
                          </Button>
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end" className="w-72">
                          {rememberedSites.map((url) => (
                            <DropdownMenuItem
                              key={url}
                              className="justify-between gap-2"
                              onSelect={() => fillRememberedSite(url)}
                            >
                              <span className="min-w-0 truncate">{url}</span>
                              <button
                                type="button"
                                className="shrink-0 text-xs text-secondary hover:text-primary"
                                data-sub2api-site-delete=""
                                onPointerDown={(ev) => ev.preventDefault()}
                                onClick={(ev) => {
                                  ev.preventDefault();
                                  ev.stopPropagation();
                                  onDeleteRememberedSite(url);
                                }}
                              >
                                {t('routes.sub2api.rememberedDeleteAccount')}
                              </button>
                            </DropdownMenuItem>
                          ))}
                        </DropdownMenuContent>
                      </DropdownMenu>
                    ) : null}
                  </div>
                </div>
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

            <p className="text-sm text-secondary">{t('routes.sub2api.syncedKeysEmpty')}</p>
          </Card>
        )}

        {phase === 'logged-in' && (
          <div className="flex min-h-0 flex-1 flex-col gap-3">
            {loadingKeys ? (
              <Card className="space-y-2 p-4">
                <Skeleton className="h-10 w-full" />
                <Skeleton className="h-10 w-full" />
              </Card>
            ) : sortedKeys.length === 0 ? (
              <Card className="p-6 text-sm text-secondary">
                <div className="font-medium text-primary">{t('routes.sub2api.keysEmpty')}</div>
                <p className="mt-1">{t('routes.sub2api.keysEmptyHint')}</p>
              </Card>
            ) : (
              <TableShell className="min-h-0 flex-1">
                <Table className="min-w-[1200px]" data-sub2api-keys-table="">
                  <TableHeader>
                    <TableHeaderRow>
                      <TableHead>{t('routes.sub2api.colName')}</TableHead>
                      <TableHead>{t('routes.sub2api.colApiKey')}</TableHead>
                      <TableHead>{t('routes.sub2api.colGroup')}</TableHead>
                      <TableHead>{t('routes.sub2api.colConcurrency')}</TableHead>
                      <TableHead>{t('routes.sub2api.colUsage')}</TableHead>
                      <TableHead>{t('routes.sub2api.colExpires')}</TableHead>
                      <TableHead>{t('routes.sub2api.colStatus')}</TableHead>
                      <TableHead>{t('routes.sub2api.colCreated')}</TableHead>
                      <TableHead>{t('routes.sub2api.colActions')}</TableHead>
                    </TableHeaderRow>
                  </TableHeader>
                  <TableBody>
                    {sortedKeys.map((key) => {
                      const statusKind = sub2apiKeyStatusKind(key.status);
                      const groupLabel = pickGroupLabel(key);
                      const groupRate = pickGroupRate(key);
                      const usage = pickKeyUsageUsd(key);
                      return (
                        <TableRow key={key.id} data-sub2api-key-row={String(key.id)}>
                          <TableCell className="whitespace-nowrap font-medium">
                            {key.name || `Key #${key.id}`}
                          </TableCell>
                          <TableCell>
                            <div className="flex items-center gap-1.5">
                              <span className="rounded-btn bg-subtle px-1.5 py-0.5 font-mono text-xs text-secondary">
                                {maskSub2ApiTableKey(key.key)}
                              </span>
                              <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                className="h-7 w-7 p-0"
                                aria-label={t('routes.sub2api.copyKey')}
                                onClick={() => copyKeySecret(key.key)}
                              >
                                <Copy className="h-3.5 w-3.5" />
                              </Button>
                            </div>
                          </TableCell>
                          <TableCell>
                            <Sub2ApiGroupCell
                              label={groupLabel}
                              rate={groupRate}
                              groupId={pickGroupId(key)}
                              groups={groups}
                              disabled={updatingGroupId === key.id}
                              onSelect={(groupId) => void onChangeGroup(key, groupId)}
                            />
                          </TableCell>
                          <TableCell>
                            <span className="inline-flex min-w-[1.75rem] justify-center rounded-btn bg-subtle px-1.5 py-0.5 font-mono text-xs">
                              {pickKeyConcurrency(key)}
                            </span>
                          </TableCell>
                          <TableCell className="whitespace-nowrap text-xs leading-5 text-secondary">
                            <div>
                              {t('routes.sub2api.usageToday')}: {formatUsdAmount(usage.today)}
                            </div>
                            <div>
                              {t('routes.sub2api.usageLast30Days')}:{' '}
                              {formatUsdAmount(usage.last30Days)}
                            </div>
                          </TableCell>
                          <TableCell className="whitespace-nowrap text-xs text-secondary">
                            {formatKeyExpires(key.expires_at, t('routes.sub2api.expiresNever'))}
                          </TableCell>
                          <TableCell>
                            <Badge variant={sub2apiKeyStatusBadgeVariant(statusKind)}>
                              {sub2apiKeyStatusLabel(key.status, {
                                active: t('routes.sub2api.statusActive'),
                                disabled: t('routes.sub2api.statusDisabled'),
                                expired: t('routes.sub2api.statusExpired'),
                                quotaExhausted: t('routes.sub2api.statusQuotaExhausted'),
                                other: t('routes.sub2api.statusOther'),
                              })}
                            </Badge>
                          </TableCell>
                          <TableCell className="whitespace-nowrap text-xs text-secondary">
                            {formatKeyTableTimestamp(key.created_at) ?? '—'}
                          </TableCell>
                          <TableCell>
                            <Sub2ApiKeyActions
                              keyRow={key}
                              groups={groups}
                              gatewayBaseUrl={session?.gatewayBaseUrl ?? ''}
                              installedAgents={importAgents}
                              onImport={startImport}
                              onToggleStatus={(row) => void onToggleKeyStatus(row)}
                              onEdit={openEditKey}
                              onDelete={setPendingDeleteKey}
                              busy={actingKeyId === key.id || creating}
                            />
                          </TableCell>
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
              </TableShell>
            )}
          </div>
        )}
      </div>

      <Dialog
        open={createOpen}
        onOpenChange={(open) => {
          if (!open) setCreateOpen(false);
        }}
      >
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('routes.sub2api.createKey')}</DialogTitle>
          </DialogHeader>
          <label className="block space-y-1.5">
            <span className="text-sm text-secondary">{t('routes.sub2api.createKeyName')}</span>
            <Input value={newKeyName} onChange={(e) => setNewKeyName(e.target.value)} />
          </label>
          {groups.length > 0 ? (
            <label className="block space-y-1.5">
              <span className="text-sm text-secondary">{t('routes.sub2api.selectGroup')}</span>
              <Select
                value={newKeyGroupId == null ? undefined : String(newKeyGroupId)}
                onValueChange={(value) => {
                  const n = Number(value);
                  setNewKeyGroupId(Number.isFinite(n) ? n : null);
                }}
              >
                <SelectTrigger data-sub2api-create-group="">
                  <SelectValue placeholder={t('routes.sub2api.selectGroup')} />
                </SelectTrigger>
                <SelectContent>
                  {groups.map((group) => (
                    <SelectItem key={group.id} value={String(group.id)}>
                      {group.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </label>
          ) : null}
          <DialogFooter className="gap-2 sm:gap-2">
            <Button type="button" variant="outline" onClick={() => setCreateOpen(false)} disabled={creating}>
              {t('common.cancel')}
            </Button>
            <Button type="button" onClick={() => void onCreateKey()} disabled={creating}>
              {t('routes.sub2api.createKeyConfirm')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {editingKey ? (
        <Sub2ApiKeyEditDialog
          keyRow={editingKey}
          groups={groups}
          busy={creating}
          onClose={() => setEditingKey(null)}
          onSave={(form) => void onSaveEditedKey(form)}
          onResetQuota={() => void onResetQuota()}
          onResetRateLimit={() => void onResetRateLimit()}
        />
      ) : null}

      <Dialog
        open={pendingDeleteKey != null}
        onOpenChange={(open) => {
          if (!open) setPendingDeleteKey(null);
        }}
      >
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('routes.sub2api.deleteKey')}</DialogTitle>
          </DialogHeader>
          <p className="text-sm text-secondary">
            {t('routes.sub2api.deleteKeyConfirm', {
              name: pendingDeleteKey?.name || (pendingDeleteKey ? `Key #${pendingDeleteKey.id}` : ''),
            })}
          </p>
          <DialogFooter className="gap-2 sm:gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => setPendingDeleteKey(null)}
              disabled={actingKeyId != null}
            >
              {t('common.cancel')}
            </Button>
            <Button
              type="button"
              variant="danger"
              onClick={() => void onDeleteKey()}
              disabled={actingKeyId != null}
            >
              {t('common.delete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {importSession ? (
        importSession.agentId === 'workbuddy' ? (
          <ApiKeyAccountDialog
            open
            agentId="workbuddy"
            mode="add"
            initialApiKey={importSession.draft.apiKey}
            initialBaseUrl={importSession.draft.baseUrl}
            initialModel={importSession.draft.model}
            onOpenChange={(open) => {
              if (!open) setImportSession(null);
            }}
            onSaved={(account) => {
              void finishImportedLogin('account', account.id, account.isCurrent);
            }}
          />
        ) : (
          <ProviderEditDialog
            open
            agentId={importSession.agentId}
            mode="add"
            initialBaseUrl={importSession.draft.baseUrl}
            initialApiKey={importSession.draft.apiKey}
            initialModel={importSession.draft.model}
            compactGrokApiBackend={importSession.draft.apiBackend}
            onOpenChange={(open) => {
              if (!open) setImportSession(null);
            }}
            onSaved={(provider) => {
              void finishImportedLogin('provider', provider.id, provider.isCurrent);
            }}
          />
        )
      ) : null}

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
