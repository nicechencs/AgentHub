import * as React from 'react';
import { AlertCircle, CheckCircle2, Copy, ExternalLink, Loader2 } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { useToast } from '@/components/ui/toast';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Notice } from '@/components/shared/Notice';
import {
  listOAuthOptions,
  oauthSupported,
  type OAuthLoginOption,
} from '@/lib/api/account';
import {
  cancelOfficialLogin,
  finishOfficialLogin,
  pollOfficialLogin,
  startOfficialLogin,
} from '@/lib/api/official-login';
import {
  officialLoginCopyId,
  officialLoginFooter,
  officialLoginOptionDescriptionKey,
  officialLoginOptionLabelKey,
  officialLoginRetryStep,
  officialLoginShouldFinish,
  officialLoginShouldKeepPolling,
  officialLoginSuccessView,
  presentOfficialLoginOptions,
  validateManualCallbackUrl,
  type OfficialLoginDialogStep,
  type OfficialLoginPoll,
  type OfficialLoginSession,
} from '@/lib/backend/contracts/official-login-session';
import { OAUTH_WAIT_TIMEOUT_SECS } from '@/lib/backend/contracts/oauth-constants';
import { AGENT_MAP } from '@/config/agents';
import { openExternalLink } from '@/lib/open-external';
import type { Account, AgentId } from '@/lib/types';
import type { MessageKey, TranslateFn } from '@/lib/i18n';
import { cn } from '@/lib/utils';

/** Identity for one mounted official-login attempt. */
export interface OAuthFlowToken {
  generation: number;
  cancelled: boolean;
}

export function createOAuthFlowToken(generation: number): OAuthFlowToken {
  return { generation, cancelled: false };
}

export function isOAuthFlowTokenCurrent(
  current: OAuthFlowToken | null,
  token: OAuthFlowToken,
): boolean {
  return current === token && !token.cancelled;
}

export async function openManualCallbackFallbackIfCurrent(
  url: string,
  isCurrent: () => boolean,
  openLink: (target: string) => Promise<void> = openExternalLink,
): Promise<void> {
  if (!isCurrent()) return;
  await openLink(url);
}

function optionCopy(opt: OAuthLoginOption, t: TranslateFn): { label: string; description: string } {
  const copyId = officialLoginCopyId(opt.agentId, opt.id);
  if (!copyId) return { label: opt.label, description: '' };
  return {
    label: t(officialLoginOptionLabelKey(copyId)),
    description: t(officialLoginOptionDescriptionKey(copyId)),
  };
}

async function waitForOfficialLogin(
  session: OfficialLoginSession,
  isCurrent: () => boolean,
): Promise<OfficialLoginPoll> {
  if (session.flow === 'pkce') {
    return pollOfficialLogin(session, OAUTH_WAIT_TIMEOUT_SECS);
  }
  const intervalMs = Math.max(2, session.intervalSecs || 5) * 1000;
  const deadline = Date.now() + (session.expiresInSecs || 900) * 1000;
  while (isCurrent() && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
    if (!isCurrent()) return { phase: 'cancelled' };
    const poll = await pollOfficialLogin(session);
    if (!isCurrent()) return { phase: 'cancelled' };
    if (!officialLoginShouldKeepPolling(poll.phase)) return poll;
  }
  return { phase: 'expired' };
}

/**
 * Official-login wait page.
 * One session (start / poll / finish / cancel); PKCE vs device-code stay adapters.
 */
export function OAuthFlowDialog({
  agentId,
  open,
  onOpenChange,
  onCompleted,
}: {
  agentId: AgentId;
  open: boolean;
  onOpenChange: (v: boolean) => void;
  onCompleted: (acc: Account) => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [step, setStep] = React.useState<OfficialLoginDialogStep>('check');
  const [countdown, setCountdown] = React.useState(OAUTH_WAIT_TIMEOUT_SECS);
  const [account, setAccount] = React.useState<Account | null>(null);
  const [errorMsg, setErrorMsg] = React.useState<string | null>(null);
  const [session, setSession] = React.useState<OfficialLoginSession | null>(null);
  const [manualUrl, setManualUrl] = React.useState('');
  const [submittingManual, setSubmittingManual] = React.useState(false);
  const [options, setOptions] = React.useState<OAuthLoginOption[]>([]);
  const [selected, setSelected] = React.useState<OAuthLoginOption | null>(null);
  const meta = AGENT_MAP[agentId];
  const flowGenerationRef = React.useRef(0);
  const flowTokenRef = React.useRef<OAuthFlowToken | null>(null);
  const sessionRef = React.useRef<OfficialLoginSession | null>(null);

  const adoptSession = (next: OfficialLoginSession | null) => {
    sessionRef.current = next;
    setSession(next);
  };

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) {
      if (flowTokenRef.current) flowTokenRef.current.cancelled = true;
      flowTokenRef.current = null;
      flowGenerationRef.current += 1;
      const current = sessionRef.current;
      adoptSession(null);
      if (current) void cancelOfficialLogin(current).catch(() => {});
    }
    onOpenChange(nextOpen);
  };

  React.useEffect(() => {
    if (!open) {
      if (flowTokenRef.current) flowTokenRef.current.cancelled = true;
      flowTokenRef.current = null;
      flowGenerationRef.current += 1;
      const current = sessionRef.current;
      adoptSession(null);
      if (current) void cancelOfficialLogin(current).catch(() => {});
      return;
    }
    const token = createOAuthFlowToken(++flowGenerationRef.current);
    flowTokenRef.current = token;
    const isCurrent = () => isOAuthFlowTokenCurrent(flowTokenRef.current, token);

    setStep('check');
    setCountdown(OAUTH_WAIT_TIMEOUT_SECS);
    setAccount(null);
    setErrorMsg(null);
    adoptSession(null);
    setManualUrl('');
    setSubmittingManual(false);
    setOptions([]);
    setSelected(null);

    void (async () => {
      try {
        const ok = await oauthSupported(agentId);
        if (!isCurrent()) return;
        if (!ok) {
          setStep('unavailable');
          return;
        }
        const opts = presentOfficialLoginOptions(await listOAuthOptions(agentId));
        if (!isCurrent()) return;
        setOptions(opts);
        if (opts.length === 0) {
          setStep('unavailable');
        } else if (opts.length === 1) {
          setSelected(opts[0]!);
          setStep('start');
        } else {
          setStep('pick');
        }
      } catch (e) {
        if (!isCurrent()) return;
        setErrorMsg(e instanceof Error ? e.message : String(e));
        setStep('error');
      }
    })();

    return () => {
      token.cancelled = true;
      if (flowTokenRef.current === token) flowTokenRef.current = null;
    };
  }, [open, agentId]);

  React.useEffect(() => {
    if (!open || step !== 'waiting') return;
    const tick = window.setInterval(() => setCountdown((c) => Math.max(0, c - 1)), 1000);
    return () => window.clearInterval(tick);
  }, [open, step]);

  const chooseOption = (opt: OAuthLoginOption) => {
    setSelected(opt);
    setErrorMsg(null);
    setStep('start');
  };

  const startSelectedFlow = async () => {
    if (!selected) return;
    const token = flowTokenRef.current;
    if (!token) return;
    const isCurrent = () => isOAuthFlowTokenCurrent(flowTokenRef.current, token);
    setErrorMsg(null);
    setStep('waiting');
    setManualUrl('');
    setCountdown(selected.flow === 'deviceCode' ? 900 : OAUTH_WAIT_TIMEOUT_SECS);
    try {
      const started = await startOfficialLogin(agentId, selected, true);
      if (!isCurrent()) {
        void cancelOfficialLogin(started).catch(() => {});
        return;
      }
      adoptSession(started);
      setCountdown(started.expiresInSecs || OAUTH_WAIT_TIMEOUT_SECS);
      if (started.flow === 'deviceCode') {
        const url = started.verificationUriComplete || started.verificationUri;
        if (url) void openExternalLink(url).catch(() => {});
      } else if (!started.browserOpened && started.authorizeUrl) {
        toast({ title: t('connect.oauth.openAuthPage') });
        void openExternalLink(started.authorizeUrl).catch(() => {});
      }
      const poll = await waitForOfficialLogin(started, isCurrent);
      if (!isCurrent()) return;
      if (!officialLoginShouldFinish(poll.phase)) {
        const fallback: MessageKey =
          poll.phase === 'expired'
            ? 'connect.oauth.deviceTimeout'
            : started.flow === 'deviceCode'
              ? 'connect.oauth.deviceFailed'
              : 'connect.oauth.authFailed';
        setErrorMsg(poll.error ?? t(fallback));
        setStep('error');
        return;
      }
      const acc = await finishOfficialLogin(started);
      if (!isCurrent()) return;
      setAccount(acc);
      setStep('done');
    } catch (e) {
      if (!isCurrent()) return;
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setStep('error');
    }
  };

  const submitManualCallback = async () => {
    const url = manualUrl.trim();
    if (!url || !session || session.flow !== 'pkce') return;
    const token = flowTokenRef.current;
    if (!token) return;
    const isCurrent = () => isOAuthFlowTokenCurrent(flowTokenRef.current, token);
    const parsed = validateManualCallbackUrl(url, session.redirectUri ?? '', session.sessionId);
    if (!parsed.ok) {
      toast({
        title: t('connect.oauth.invalidCallback'),
        variant: 'danger',
      });
      return;
    }
    setSubmittingManual(true);
    try {
      try {
        await fetch(parsed.href, { mode: 'no-cors', credentials: 'omit' });
      } catch {
        await openManualCallbackFallbackIfCurrent(parsed.href, isCurrent).catch(() => {});
      }
      if (!isCurrent()) return;
      toast({ title: t('connect.oauth.submittedCallback') });
    } finally {
      if (isCurrent()) setSubmittingManual(false);
    }
  };

  const copyAuthorizeUrl = () => {
    if (!session?.authorizeUrl) return;
    navigator.clipboard.writeText(session.authorizeUrl).catch(() => {});
    toast({ title: t('connect.oauth.copiedLink') });
  };

  const copyUserCode = () => {
    if (!session?.userCode) return;
    navigator.clipboard.writeText(session.userCode).catch(() => {});
    toast({ title: t('connect.oauth.copiedCode') });
  };

  const retry = () => {
    const current = sessionRef.current;
    adoptSession(null);
    if (current) void cancelOfficialLogin(current).catch(() => {});
    setErrorMsg(null);
    setManualUrl('');
    setStep(officialLoginRetryStep(options.length));
  };

  const selectedCopy = selected ? optionCopy(selected, t) : null;
  const successView = account ? officialLoginSuccessView(account) : null;
  const mm = String(Math.floor(countdown / 60));
  const ss = String(countdown % 60).padStart(2, '0');
  const title = selectedCopy
    ? t('connect.oauth.titleWithProvider', { name: meta.name, provider: selectedCopy.label })
    : t('connect.oauth.title', { name: meta.name });
  const footer = officialLoginFooter(step, step === 'waiting');
  const startIsDevice = selected?.flow === 'deviceCode';
  const waitingFlow = session?.flow ?? selected?.flow;

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>

        {step === 'check' && (
          <div className="flex flex-col items-center gap-3 py-8 text-center">
            <Loader2 className="h-8 w-8 animate-spin text-accent" />
            <p className="text-sm text-secondary">{t('connect.oauth.checking')}</p>
          </div>
        )}

        {step === 'unavailable' && (
          <div className="flex flex-col items-center gap-3 py-6 text-center">
            <AlertCircle className="h-10 w-10 text-warning" />
            <p className="text-sm font-medium text-primary">{t('connect.oauth.unavailableTitle')}</p>
            <p className="text-xs text-secondary">{t('connect.oauth.unavailableDesc')}</p>
          </div>
        )}

        {step === 'pick' && (
          <div className="flex flex-col gap-3 py-2">
            <p className="text-sm text-secondary">{t('connect.oauth.pickHint')}</p>
            <div className="flex flex-col gap-2">
              {options.map((opt) => {
                const copy = optionCopy(opt, t);
                return (
                  <button
                    key={opt.id}
                    type="button"
                    className={cn(
                      'rounded-card border border-border bg-canvas px-3 py-2.5 text-left transition-colors',
                      'hover:border-accent/50 hover:bg-subtle',
                    )}
                    onClick={() => chooseOption(opt)}
                  >
                    <div className="text-sm font-medium text-primary">{copy.label}</div>
                    {copy.description ? (
                      <div className="mt-0.5 text-xs text-muted">{copy.description}</div>
                    ) : null}
                    <div className="mt-1 text-meta text-muted">
                      {opt.flow === 'deviceCode'
                        ? t('connect.oauth.flowDevice')
                        : t('connect.oauth.flowBrowser')}
                    </div>
                  </button>
                );
              })}
            </div>
          </div>
        )}

        {step === 'start' && (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            <ExternalLink className="h-8 w-8 text-accent" />
            <p className="text-sm text-secondary">
              {startIsDevice
                ? t('connect.oauth.deviceHint', { name: selectedCopy?.label ?? meta.name })
                : t('connect.oauth.browserHint', { name: selectedCopy?.label ?? meta.name })}
            </p>
            {options.length > 1 ? (
              <Button variant="ghost" size="sm" onClick={() => setStep('pick')}>
                {t('connect.oauth.reselect')}
              </Button>
            ) : null}
            <Button onClick={() => void startSelectedFlow()}>
              {startIsDevice ? (
                t('connect.oauth.startDevice')
              ) : (
                <>
                  <ExternalLink className="h-4 w-4" /> {t('connect.oauth.openBrowser')}
                </>
              )}
            </Button>
          </div>
        )}

        {step === 'waiting' && waitingFlow === 'deviceCode' && (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            <Loader2 className="h-8 w-8 animate-spin text-accent" />
            <p className="text-sm text-secondary">{t('connect.oauth.waitingDevice')}</p>
            {session?.userCode ? (
              <Card variant="plain" className="w-full bg-canvas px-4 py-3">
                <p className="text-xs text-muted">{t('connect.oauth.deviceCode')}</p>
                <p className="font-mono text-title tracking-widest text-primary">{session.userCode}</p>
              </Card>
            ) : null}
            <p className="font-mono text-title tabular-nums text-primary">
              {mm}:{ss}
            </p>
            <div className="flex flex-wrap justify-center gap-2">
              <Button size="sm" variant="outline" onClick={copyUserCode} disabled={!session?.userCode}>
                <Copy className="h-3.5 w-3.5" /> {t('connect.oauth.copyDeviceCode')}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                disabled={!session?.verificationUriComplete && !session?.verificationUri}
                onClick={() => {
                  const url = session?.verificationUriComplete || session?.verificationUri;
                  if (url) void openExternalLink(url).catch(() => {});
                }}
              >
                <ExternalLink className="h-3.5 w-3.5" /> {t('connect.oauth.openVerify')}
              </Button>
            </div>
          </div>
        )}

        {step === 'waiting' && waitingFlow !== 'deviceCode' && (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            <Loader2 className="h-8 w-8 animate-spin text-accent" />
            <p className="text-sm text-secondary">{t('connect.oauth.waitingCallback')}</p>
            <p className="font-mono text-title tabular-nums text-primary">
              {mm}:{ss}
            </p>
            <div className="w-full space-y-2 text-left">
              <Notice tone="info">{t('connect.oauth.waitingNotice')}</Notice>
              <div className="flex flex-wrap gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  disabled={!session?.authorizeUrl}
                  onClick={copyAuthorizeUrl}
                >
                  <Copy className="h-3.5 w-3.5" /> {t('connect.oauth.copyAuthLink')}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={!session?.authorizeUrl}
                  onClick={() => {
                    const token = flowTokenRef.current;
                    if (!session?.authorizeUrl || !token) return;
                    void openExternalLink(session.authorizeUrl).catch((e) => {
                      if (!isOAuthFlowTokenCurrent(flowTokenRef.current, token)) return;
                      toast({
                        title: t('connect.oauth.cannotOpen'),
                        description: e instanceof Error ? e.message : String(e),
                        variant: 'danger',
                      });
                    });
                  }}
                >
                  <ExternalLink className="h-3.5 w-3.5" /> {t('connect.oauth.reopenAuth')}
                </Button>
              </div>
              <Card variant="plain" className="bg-canvas p-3">
                <p className="mb-2 text-xs text-muted">{t('connect.oauth.pasteCallback')}</p>
                <div className="flex gap-2">
                  <Input
                    value={manualUrl}
                    onChange={(e) => setManualUrl(e.target.value)}
                    placeholder={t('connect.oauth.pastePlaceholder')}
                    className="font-mono text-xs"
                  />
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={submittingManual || !manualUrl.trim()}
                    onClick={() => void submitManualCallback()}
                  >
                    {submittingManual ? t('connect.oauth.submitting') : t('connect.oauth.submit')}
                  </Button>
                </div>
              </Card>
            </div>
          </div>
        )}

        {step === 'error' && (
          <div className="flex flex-col items-center gap-3 py-6 text-center">
            <AlertCircle className="h-10 w-10 text-danger" />
            <p className="text-sm font-medium text-primary">{t('connect.oauth.failedTitle')}</p>
            <p className="text-xs text-secondary">{errorMsg ?? t('connect.oauth.unknownError')}</p>
          </div>
        )}

        {step === 'done' && account && (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            <CheckCircle2 className="h-10 w-10 text-success" />
            <p className="text-sm font-medium">{t('connect.oauth.success')}</p>
            {successView?.title || successView?.subscription ? (
              <Card variant="plain" className="bg-canvas px-6 py-3">
                {successView.title ? <p className="text-sm">{successView.title}</p> : null}
                {successView.subscription ? (
                  <p className="mt-0.5 text-xs text-secondary">{successView.subscription}</p>
                ) : null}
                {successView.identity ? (
                  <p className="mt-0.5 text-xs text-muted">{successView.identity}</p>
                ) : null}
              </Card>
            ) : null}
            <p className="text-xs text-muted">
              {agentId === 'pi' ? t('connect.oauth.writtenPi') : t('connect.oauth.writtenPool')}
            </p>
          </div>
        )}

        <DialogFooter>
          {footer === 'success' && account ? (
            <>
              <Button variant="outline" onClick={() => handleOpenChange(false)}>
                {t('connect.oauth.later')}
              </Button>
              <Button
                onClick={() => {
                  onCompleted(account);
                  handleOpenChange(false);
                }}
              >
                {t('connect.oauth.switchNow')}
              </Button>
            </>
          ) : footer === 'retry' ? (
            <>
              <Button variant="outline" onClick={() => handleOpenChange(false)}>
                {t('connect.oauth.close')}
              </Button>
              <Button onClick={retry}>{t('chrome.error.retry')}</Button>
            </>
          ) : (
            <Button variant="outline" onClick={() => handleOpenChange(false)}>
              {footer === 'cancelWait' ? t('connect.oauth.cancelWait') : t('connect.oauth.close')}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
