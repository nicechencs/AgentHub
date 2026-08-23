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
import { Tip } from '@/components/ui/tooltip';
import {
  finishDeviceOAuth,
  finishOAuth,
  listOAuthOptions,
  oauthSupported,
  pollDeviceOAuth,
  startDeviceOAuth,
  startOAuth,
  waitOAuth,
  type DeviceOAuthStartInfo,
  type OAuthLoginOption,
} from '@/lib/api/account';
import { AGENT_MAP } from '@/config/agents';
import { openExternalLink } from '@/lib/open-external';
import type { Account, AgentId } from '@/lib/types';
import { cn } from '@/lib/utils';

type Step =
  | 'check'
  | 'pick'
  | 'browser'
  | 'waiting'
  | 'device'
  | 'done'
  | 'unavailable'
  | 'error';

/** Identity for one mounted OAuth attempt. */
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

/**
 * OAuth 对话框：
 * - 单 provider：直接 PKCE 浏览器流
 * - Pi 多 provider：先选 anthropic / openai-codex / xai
 * - xai：设备码流
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
  const [step, setStep] = React.useState<Step>('check');
  const [countdown, setCountdown] = React.useState(120);
  const [account, setAccount] = React.useState<Account | null>(null);
  const [errorMsg, setErrorMsg] = React.useState<string | null>(null);
  const [oauthState, setOauthState] = React.useState<string | null>(null);
  const [authorizeUrl, setAuthorizeUrl] = React.useState<string | null>(null);
  const [redirectUri, setRedirectUri] = React.useState<string | null>(null);
  const [manualUrl, setManualUrl] = React.useState('');
  const [submittingManual, setSubmittingManual] = React.useState(false);
  const [options, setOptions] = React.useState<OAuthLoginOption[]>([]);
  const [selected, setSelected] = React.useState<OAuthLoginOption | null>(null);
  const [deviceInfo, setDeviceInfo] = React.useState<DeviceOAuthStartInfo | null>(null);
  const meta = AGENT_MAP[agentId];
  const flowGenerationRef = React.useRef(0);
  const flowTokenRef = React.useRef<OAuthFlowToken | null>(null);

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) {
      if (flowTokenRef.current) flowTokenRef.current.cancelled = true;
      flowTokenRef.current = null;
      flowGenerationRef.current += 1;
    }
    onOpenChange(nextOpen);
  };

  React.useEffect(() => {
    if (!open) {
      if (flowTokenRef.current) flowTokenRef.current.cancelled = true;
      flowTokenRef.current = null;
      flowGenerationRef.current += 1;
      return;
    }
    const token = createOAuthFlowToken(++flowGenerationRef.current);
    flowTokenRef.current = token;
    const isCurrent = () => isOAuthFlowTokenCurrent(flowTokenRef.current, token);

    setStep('check');
    setCountdown(120);
    setAccount(null);
    setErrorMsg(null);
    setOauthState(null);
    setAuthorizeUrl(null);
    setRedirectUri(null);
    setManualUrl('');
    setSubmittingManual(false);
    setOptions([]);
    setSelected(null);
    setDeviceInfo(null);

    void (async () => {
      try {
        const ok = await oauthSupported(agentId);
        if (!isCurrent()) return;
        if (!ok) {
          setStep('unavailable');
          return;
        }
        const opts = await listOAuthOptions(agentId);
        if (!isCurrent()) return;
        setOptions(opts);
        if (opts.length === 0) {
          setStep('unavailable');
        } else if (opts.length === 1) {
          setSelected(opts[0]!);
          setStep(opts[0]!.flow === 'deviceCode' ? 'device' : 'browser');
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
    if (!open || (step !== 'waiting' && step !== 'device')) return;
    const tick = window.setInterval(() => setCountdown((c) => Math.max(0, c - 1)), 1000);
    return () => window.clearInterval(tick);
  }, [open, step]);

  const chooseOption = (opt: OAuthLoginOption) => {
    setSelected(opt);
    setErrorMsg(null);
    setStep(opt.flow === 'deviceCode' ? 'device' : 'browser');
  };

  const startPkceFlow = async () => {
    const token = flowTokenRef.current;
    if (!token) return;
    const isCurrent = () => isOAuthFlowTokenCurrent(flowTokenRef.current, token);
    setErrorMsg(null);
    setStep('waiting');
    setCountdown(120);
    setManualUrl('');
    try {
      const start = await startOAuth(agentId, true, selected?.id ?? null);
      if (!isCurrent()) return;
      setOauthState(start.state);
      setAuthorizeUrl(start.authorizeUrl);
      setRedirectUri(start.redirectUri);
      if (!start.browserOpened) {
        toast({
          title: t('connect.oauth.openAuthPage'),
          description: start.authorizeUrl,
        });
        void openExternalLink(start.authorizeUrl).catch(() => {});
      }
      const wait = await waitOAuth(start.state, 120);
      if (!isCurrent()) return;
      if (wait.status === 'failed') {
        setErrorMsg(wait.error ?? t('connect.oauth.authFailed'));
        setStep('error');
        return;
      }
      const acc = await finishOAuth(start.state);
      if (!isCurrent()) return;
      setAccount(acc);
      setStep('done');
    } catch (e) {
      if (!isCurrent()) return;
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setStep('error');
    }
  };

  const startDeviceFlow = async () => {
    if (!selected) return;
    const token = flowTokenRef.current;
    if (!token) return;
    const isCurrent = () => isOAuthFlowTokenCurrent(flowTokenRef.current, token);
    setErrorMsg(null);
    setStep('device');
    try {
      const start = await startDeviceOAuth(agentId, selected.id);
      if (!isCurrent()) return;
      setDeviceInfo(start);
      setOauthState(start.state);
      setCountdown(start.expiresInSecs || 900);
      if (start.verificationUriComplete || start.verificationUri) {
        void openExternalLink(start.verificationUriComplete || start.verificationUri).catch(
          () => {},
        );
      }
      const intervalMs = Math.max(2, start.intervalSecs || 5) * 1000;
      const deadline = Date.now() + (start.expiresInSecs || 900) * 1000;
      while (isCurrent() && Date.now() < deadline) {
        await new Promise((r) => setTimeout(r, intervalMs));
        if (!isCurrent()) return;
        const poll = await pollDeviceOAuth(start.state);
        if (!isCurrent()) return;
        if (poll.status === 'complete') {
          const acc = await finishDeviceOAuth(start.state);
          if (!isCurrent()) return;
          setAccount(acc);
          setStep('done');
          return;
        }
        if (poll.status === 'failed' || poll.status === 'expired') {
          setErrorMsg(poll.error ?? t('connect.oauth.deviceFailed'));
          setStep('error');
          return;
        }
        // A concurrent completion holds the session in Completing; keep
        // polling until the consumer either finishes or the session expires.
        if (poll.status === 'completing') continue;
      }
      if (isCurrent()) {
        setErrorMsg(t('connect.oauth.deviceTimeout'));
        setStep('error');
      }
    } catch (e) {
      if (!isCurrent()) return;
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setStep('error');
    }
  };

  /** 用户把浏览器最终跳转的 loopback 回调 URL 粘贴回来时，fetch 触发本机 listener */
  const submitManualCallback = async () => {
    const url = manualUrl.trim();
    if (!url) return;
    const token = flowTokenRef.current;
    if (!token) return;
    const isCurrent = () => isOAuthFlowTokenCurrent(flowTokenRef.current, token);
    setSubmittingManual(true);
    try {
      try {
        await fetch(url, { mode: 'no-cors', credentials: 'omit' });
      } catch {
        await openManualCallbackFallbackIfCurrent(url, isCurrent).catch(() => {});
      }
      if (!isCurrent()) return;
      toast({ title: t('connect.oauth.submittedCallback') });
    } finally {
      if (isCurrent()) setSubmittingManual(false);
    }
  };

  const copyAuthorizeUrl = () => {
    if (!authorizeUrl) return;
    navigator.clipboard.writeText(authorizeUrl).catch(() => {});
    toast({ title: t('connect.oauth.copiedLink') });
  };

  const copyUserCode = () => {
    if (!deviceInfo?.userCode) return;
    navigator.clipboard.writeText(deviceInfo.userCode).catch(() => {});
    toast({ title: t('connect.oauth.copiedCode') });
  };

  const mm = String(Math.floor(countdown / 60));
  const ss = String(countdown % 60).padStart(2, '0');
  const title = selected
    ? t('connect.oauth.titleWithProvider', { name: meta.name, provider: selected.label })
    : t('connect.oauth.title', { name: meta.name });

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
            <p className="text-xs text-secondary">
              {t('connect.oauth.unavailableDesc')}
            </p>
          </div>
        )}

        {step === 'pick' && (
          <div className="flex flex-col gap-3 py-2">
            <p className="text-sm text-secondary">
              {t('connect.oauth.pickHint', { path: '~/.pi/agent/auth.json' })}
            </p>
            <div className="flex flex-col gap-2">
              {options.map((opt) => (
                <button
                  key={opt.id}
                  type="button"
                  className={cn(
                    'rounded-card border border-border bg-canvas px-3 py-2.5 text-left transition-colors',
                    'hover:border-accent/50 hover:bg-subtle',
                  )}
                  onClick={() => chooseOption(opt)}
                >
                  <div className="text-sm font-medium text-primary">{opt.label}</div>
                  <div className="mt-0.5 text-xs text-muted">{opt.description}</div>
                  <div className="mt-1 font-mono text-meta text-muted">
                    {opt.flow === 'deviceCode' ? t('connect.oauth.flowDevice') : t('connect.oauth.flowBrowser')}
                    {opt.authJsonKey ? ` · ${opt.authJsonKey}` : ''}
                  </div>
                </button>
              ))}
            </div>
          </div>
        )}

        {step === 'browser' && (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            <ExternalLink className="h-8 w-8 text-accent" />
            <p className="text-sm text-secondary">
              {t('connect.oauth.browserHint', { name: selected?.label ?? meta.name })}
            </p>
            {options.length > 1 ? (
              <Button variant="ghost" size="sm" onClick={() => setStep('pick')}>
                {t('connect.oauth.reselect')}
              </Button>
            ) : null}
            <Button onClick={() => void startPkceFlow()}>
              <ExternalLink className="h-4 w-4" /> {t('connect.oauth.openBrowser')}
            </Button>
          </div>
        )}

        {step === 'device' && (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            {!deviceInfo ? (
              <>
                <ExternalLink className="h-8 w-8 text-accent" />
                <p className="text-sm text-secondary">
                  {t('connect.oauth.deviceHint', { name: selected?.label ?? 'xAI' })}
                </p>
                {options.length > 1 ? (
                  <Button variant="ghost" size="sm" onClick={() => setStep('pick')}>
                    {t('connect.oauth.reselect')}
                  </Button>
                ) : null}
                <Button onClick={() => void startDeviceFlow()}>{t('connect.oauth.startDevice')}</Button>
              </>
            ) : (
              <>
                <Loader2 className="h-8 w-8 animate-spin text-accent" />
                <p className="text-sm text-secondary">{t('connect.oauth.waitingDevice')}</p>
                <Card variant="plain" className="w-full bg-canvas px-4 py-3 text-left">
                  <p className="text-xs text-muted">{t('connect.oauth.deviceCode')}</p>
                  <p className="font-mono text-title tracking-widest text-primary">
                    {deviceInfo.userCode}
                  </p>
                  <p className="mt-2 break-all font-mono text-meta text-muted">
                    {deviceInfo.verificationUri}
                  </p>
                </Card>
                <p className="font-mono text-title tabular-nums text-primary">
                  {mm}:{ss}
                </p>
                <div className="flex flex-wrap justify-center gap-2">
                  <Button size="sm" variant="outline" onClick={copyUserCode}>
                    <Copy className="h-3.5 w-3.5" /> {t('connect.oauth.copyDeviceCode')}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      const url =
                        deviceInfo.verificationUriComplete || deviceInfo.verificationUri;
                      void openExternalLink(url).catch(() => {});
                    }}
                  >
                    <ExternalLink className="h-3.5 w-3.5" /> {t('connect.oauth.openVerify')}
                  </Button>
                </div>
              </>
            )}
          </div>
        )}

        {step === 'waiting' && (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            <Loader2 className="h-8 w-8 animate-spin text-accent" />
            <p className="text-sm text-secondary">{t('connect.oauth.waitingCallback')}</p>
            <p className="font-mono text-title tabular-nums text-primary">
              {mm}:{ss}
            </p>
            {oauthState && (
              <Tip
                className="max-w-full truncate font-mono text-meta text-muted"
                label={oauthState}
              >
                state: {oauthState}
              </Tip>
            )}

            <div className="w-full space-y-2 text-left">
              <Notice tone="info">
                {t('connect.oauth.waitingNotice')}
                {redirectUri ? (
                  <span className="mt-1 block font-mono text-meta text-muted">
                    {t('connect.oauth.expectedPrefix', { uri: redirectUri })}
                  </span>
                ) : null}
              </Notice>
              <div className="flex flex-wrap gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  disabled={!authorizeUrl}
                  onClick={copyAuthorizeUrl}
                >
                  <Copy className="h-3.5 w-3.5" /> {t('connect.oauth.copyAuthLink')}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={!authorizeUrl}
                  onClick={() => {
                    const token = flowTokenRef.current;
                    if (!authorizeUrl || !token) return;
                    void openExternalLink(authorizeUrl).catch((e) => {
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
                    placeholder="http://127.0.0.1:…/callback?code=…&state=…"
                    className="font-mono text-xs"
                  />
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={submittingManual || !manualUrl.includes('code=')}
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
            <Button
              variant="secondary"
              onClick={() => {
                if (options.length > 1) setStep('pick');
                else if (selected?.flow === 'deviceCode') setStep('device');
                else setStep('browser');
              }}
            >
              {t('chrome.error.retry')}
            </Button>
          </div>
        )}

        {step === 'done' && account && (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            <CheckCircle2 className="h-10 w-10 text-success" />
            <p className="text-sm font-medium">{t('connect.oauth.success')}</p>
            <Card variant="plain" className="bg-canvas px-6 py-3">
              <p className="text-sm">{account.email ?? account.label}</p>
              {account.subscription && (
                <p className="mt-0.5 text-xs text-secondary">{account.subscription}</p>
              )}
              {account.identityLabel && account.identityLabel !== account.email ? (
                <p className="mt-0.5 text-xs text-muted">{account.identityLabel}</p>
              ) : null}
            </Card>
            <p className="text-xs text-muted">
              {agentId === 'pi'
                ? t('connect.oauth.writtenPi')
                : t('connect.oauth.writtenPool')}
            </p>
          </div>
        )}

        <DialogFooter>
          {step === 'done' && account ? (
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
          ) : (
            <Button variant="outline" onClick={() => handleOpenChange(false)}>
              {step === 'waiting' || (step === 'device' && deviceInfo)
                ? t('connect.oauth.cancelWait')
                : t('connect.oauth.close')}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
