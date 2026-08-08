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
import { Notice } from '@/components/shared/Notice';
import { Tip } from '@/components/ui/tooltip';
import {
  finishOAuth,
  oauthSupported,
  startOAuth,
  waitOAuth,
} from '@/lib/api/account';
import { AGENT_MAP } from '@/config/agents';
import { openExternalLink } from '@/lib/open-external';
import type { Account, AgentId } from '@/lib/types';

type Step = 'check' | 'browser' | 'waiting' | 'done' | 'unavailable' | 'error';

/**
 * 生产 OAuth PKCE 对话框：
 * - 已配置 PKCE：系统浏览器 + loopback + 入库
 * - 等待回调：倒计时 + 复制授权链接 + 手动粘贴回调 URL 降级
 * - 未配置：明确 unavailable
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
  const meta = AGENT_MAP[agentId];
  const cancelRef = React.useRef({ cancelled: false });

  React.useEffect(() => {
    if (!open) return;
    cancelRef.current.cancelled = false;
    setStep('check');
    setCountdown(120);
    setAccount(null);
    setErrorMsg(null);
    setOauthState(null);
    setAuthorizeUrl(null);
    setRedirectUri(null);
    setManualUrl('');
    setSubmittingManual(false);

    void (async () => {
      try {
        const ok = await oauthSupported(agentId);
        if (cancelRef.current.cancelled) return;
        setStep(ok ? 'browser' : 'unavailable');
      } catch (e) {
        if (cancelRef.current.cancelled) return;
        setErrorMsg(e instanceof Error ? e.message : String(e));
        setStep('error');
      }
    })();

    return () => {
      cancelRef.current.cancelled = true;
    };
  }, [open, agentId]);

  React.useEffect(() => {
    if (!open || step !== 'waiting') return;
    const tick = window.setInterval(() => setCountdown((c) => Math.max(0, c - 1)), 1000);
    return () => window.clearInterval(tick);
  }, [open, step]);

  const startFlow = async () => {
    setErrorMsg(null);
    setStep('waiting');
    setCountdown(120);
    setManualUrl('');
    try {
      const start = await startOAuth(agentId, true);
      if (cancelRef.current.cancelled) return;
      setOauthState(start.state);
      setAuthorizeUrl(start.authorizeUrl);
      setRedirectUri(start.redirectUri);
      if (!start.browserOpened) {
        toast({
          title: '请手动打开授权页',
          description: start.authorizeUrl,
        });
        void openExternalLink(start.authorizeUrl).catch(() => {
          /* toast already above; copy remains available */
        });
      }
      const wait = await waitOAuth(start.state, 120);
      if (cancelRef.current.cancelled) return;
      if (wait.status === 'failed') {
        setErrorMsg(wait.error ?? '授权失败');
        setStep('error');
        return;
      }
      const acc = await finishOAuth(start.state);
      if (cancelRef.current.cancelled) return;
      setAccount(acc);
      setStep('done');
    } catch (e) {
      if (cancelRef.current.cancelled) return;
      setErrorMsg(e instanceof Error ? e.message : String(e));
      setStep('error');
    }
  };

  /** 用户把浏览器最终跳转的 loopback 回调 URL 粘贴回来时，fetch 触发本机 listener */
  const submitManualCallback = async () => {
    const url = manualUrl.trim();
    if (!url) return;
    setSubmittingManual(true);
    try {
      // 触发 core loopback listener 写入 code；waitOAuth 仍在进行时会自动完成
      await fetch(url, { mode: 'no-cors', credentials: 'omit' }).catch(() => {
        // no-cors 可能 opaque；另开窗口作为兜底
        void openExternalLink(url).catch(() => {});
      });
      toast({ title: '已提交回调，若仍等待请确认 URL 含 code 与 state' });
    } finally {
      setSubmittingManual(false);
    }
  };

  const copyAuthorizeUrl = () => {
    if (!authorizeUrl) return;
    navigator.clipboard.writeText(authorizeUrl).catch(() => {});
    toast({ title: '授权链接已复制' });
  };

  const mm = String(Math.floor(countdown / 60));
  const ss = String(countdown % 60).padStart(2, '0');

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>OAuth 登录 — {meta.name}</DialogTitle>
        </DialogHeader>

        {step === 'check' && (
          <div className="flex flex-col items-center gap-3 py-8 text-center">
            <Loader2 className="h-8 w-8 animate-spin text-accent" />
            <p className="text-sm text-secondary">检查 OAuth 支持…</p>
          </div>
        )}

        {step === 'unavailable' && (
          <div className="flex flex-col items-center gap-3 py-6 text-center">
            <AlertCircle className="h-10 w-10 text-warning" />
            <p className="text-sm font-medium text-primary">OAuth 浏览器授权尚未配置</p>
            <p className="text-xs text-secondary">
              该 Agent 暂未接入 PKCE。请改用「导入当前账号」或「添加 API Key」。
            </p>
          </div>
        )}

        {step === 'browser' && (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            <ExternalLink className="h-8 w-8 text-accent" />
            <p className="text-sm text-secondary">
              将在系统浏览器中打开 {meta.name} 授权页面，请完成登录授权。回调经本机 loopback 接收。
            </p>
            <Button onClick={() => void startFlow()}>
              <ExternalLink className="h-4 w-4" /> 打开浏览器授权
            </Button>
          </div>
        )}

        {step === 'waiting' && (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            <Loader2 className="h-8 w-8 animate-spin text-accent" />
            <p className="text-sm text-secondary">等待浏览器回调…</p>
            <p className="font-mono text-2xl tabular-nums text-primary">
              {mm}:{ss}
            </p>
            {oauthState && (
              <Tip
                className="max-w-full truncate font-mono text-2xs text-muted"
                label={oauthState}
              >
                state: {oauthState}
              </Tip>
            )}

            <div className="w-full space-y-2 text-left">
              <Notice tone="info">
                若浏览器未自动回调，可复制授权链接手动打开，或把最终跳转的本地回调 URL 粘贴到下方。
                {redirectUri ? (
                  <span className="mt-1 block font-mono text-2xs text-muted">
                    期望回调前缀：{redirectUri}
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
                  <Copy className="h-3.5 w-3.5" /> 复制授权链接
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  disabled={!authorizeUrl}
                  onClick={() => {
                    if (!authorizeUrl) return;
                    void openExternalLink(authorizeUrl).catch((e) => {
                      toast({
                        title: '无法打开授权页',
                        description: e instanceof Error ? e.message : String(e),
                        variant: 'danger',
                      });
                    });
                  }}
                >
                  <ExternalLink className="h-3.5 w-3.5" /> 重新打开授权页
                </Button>
              </div>
              <Card variant="plain" className="bg-canvas p-3">
                <p className="mb-2 text-xs text-muted">手动粘贴回调 URL</p>
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
                    {submittingManual ? '提交中…' : '提交'}
                  </Button>
                </div>
              </Card>
            </div>
          </div>
        )}

        {step === 'error' && (
          <div className="flex flex-col items-center gap-3 py-6 text-center">
            <AlertCircle className="h-10 w-10 text-danger" />
            <p className="text-sm font-medium text-primary">授权失败</p>
            <p className="text-xs text-secondary">{errorMsg ?? '未知错误'}</p>
            <Button variant="secondary" onClick={() => setStep('browser')}>
              重试
            </Button>
          </div>
        )}

        {step === 'done' && account && (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            <CheckCircle2 className="h-10 w-10 text-success" />
            <p className="text-sm font-medium">授权成功</p>
            <Card variant="plain" className="bg-canvas px-6 py-3">
              <p className="text-sm">{account.email ?? account.label}</p>
              {account.subscription && (
                <p className="mt-0.5 text-xs text-secondary">{account.subscription}</p>
              )}
            </Card>
            <p className="text-xs text-muted">账号已写入本地账号池</p>
          </div>
        )}

        <DialogFooter>
          {step === 'done' && account ? (
            <>
              <Button variant="outline" onClick={() => onOpenChange(false)}>
                稍后
              </Button>
              <Button
                onClick={() => {
                  onCompleted(account);
                  onOpenChange(false);
                }}
              >
                立即切换到此账号
              </Button>
            </>
          ) : (
            <Button variant="outline" onClick={() => onOpenChange(false)}>
              {step === 'waiting' ? '取消等待' : '关闭'}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
