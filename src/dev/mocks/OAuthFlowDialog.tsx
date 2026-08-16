import * as React from 'react';
import { CheckCircle2, Copy, ExternalLink, Loader2 } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { useToast } from '@/components/ui/toast';
import { completeOAuth } from '@/lib/api/account';
import { resolveAgentMeta } from '@/config/agents';
import type { Account, AgentId } from '@/lib/types';

type Step = 'browser' | 'waiting' | 'done';

/**
 * dev:mock 专用 OAuth 演示流程。
 * 含定时器、模拟回调 URL；不得进入生产 module graph。
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
  const [step, setStep] = React.useState<Step>('browser');
  const [countdown, setCountdown] = React.useState(120);
  const [account, setAccount] = React.useState<Account | null>(null);
  const [manualUrl, setManualUrl] = React.useState('');
  const meta = resolveAgentMeta(agentId);

  React.useEffect(() => {
    if (open) {
      setStep('browser');
      setCountdown(120);
      setAccount(null);
      setManualUrl('');
    }
  }, [open]);

  React.useEffect(() => {
    if (!open || step !== 'waiting') return;
    const tick = window.setInterval(() => setCountdown((c) => Math.max(0, c - 1)), 1000);
    const fakeCallback = window.setTimeout(async () => {
      const acc = await completeOAuth(agentId);
      setAccount(acc);
      setStep('done');
    }, 4000 + Math.random() * 3000);
    return () => {
      window.clearInterval(tick);
      window.clearTimeout(fakeCallback);
    };
  }, [open, step, agentId]);

  const mm = String(Math.floor(countdown / 60));
  const ss = String(countdown % 60).padStart(2, '0');

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>OAuth 登录 — {meta.name}</DialogTitle>
        </DialogHeader>

        <p className="mb-2 rounded-btn border border-warning/30 bg-warning/5 px-2.5 py-1.5 text-xs text-warning">
          开发演示（dev:mock）：不会打开真实系统浏览器，回调与账号均为模拟数据。
        </p>

        <div className="mb-4 flex items-center gap-2 text-xs text-muted">
          {['打开浏览器', '等待回调', '完成'].map((s, i) => {
            const idx = step === 'browser' ? 0 : step === 'waiting' ? 1 : 2;
            return (
              <React.Fragment key={s}>
                {i > 0 && <span className="h-px w-6 bg-border" />}
                <span className={i <= idx ? 'text-accent' : ''}>
                  {i + 1}. {s}
                </span>
              </React.Fragment>
            );
          })}
        </div>

        {step === 'browser' && (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            <ExternalLink className="h-8 w-8 text-accent" />
            <p className="text-sm text-secondary">
              演示：模拟在浏览器中打开 {meta.name} 授权页面（不会真实打开系统浏览器）。
            </p>
            <Button onClick={() => setStep('waiting')}>
              <ExternalLink className="h-4 w-4" /> 开始演示授权
            </Button>
          </div>
        )}

        {step === 'waiting' && (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            <Loader2 className="h-8 w-8 animate-spin text-accent" />
            <p className="text-sm text-secondary">等待模拟回调（loopback）…</p>
            <p className="font-mono text-title tabular-nums text-primary">
              {mm}:{ss}
            </p>
            <div className="w-full rounded-card border border-border bg-canvas p-3">
              <p className="mb-2 text-xs text-muted">演示：复制模拟回调链接手动粘贴</p>
              <div className="flex gap-2">
                <Input
                  value={manualUrl}
                  onChange={(e) => setManualUrl(e.target.value)}
                  placeholder="http://127.0.0.1:34567/callback?code=…"
                  className="font-mono text-xs"
                />
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={!manualUrl.includes('code=')}
                  onClick={async () => {
                    const acc = await completeOAuth(agentId);
                    setAccount(acc);
                    setStep('done');
                  }}
                >
                  提交
                </Button>
              </div>
              <Button
                size="sm"
                variant="ghost"
                className="mt-2 h-6 text-xs"
                onClick={() => {
                  navigator.clipboard
                    .writeText('http://127.0.0.1:34567/callback?code=mock-code&state=mock')
                    .catch(() => {});
                  toast({ title: '模拟回调链接已复制（开发演示）' });
                }}
              >
                <Copy className="h-3 w-3" /> 复制模拟回调链接
              </Button>
            </div>
          </div>
        )}

        {step === 'done' && account && (
          <div className="flex flex-col items-center gap-3 py-4 text-center">
            <CheckCircle2 className="h-10 w-10 text-success" />
            <p className="text-sm font-medium">演示授权成功</p>
            <div className="rounded-card border border-border bg-canvas px-6 py-3">
              <p className="text-sm">{account.email}</p>
              <p className="mt-0.5 text-xs text-secondary">{account.subscription}</p>
            </div>
            <p className="text-xs text-muted">账号已写入 mock 内存池（非真实凭据存储）</p>
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
              取消
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
