import * as React from 'react';
import { useNavigate } from 'react-router-dom';
import {
  DownloadCloud,
  Sparkles,
  Wrench,
} from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { EnvStatusBar } from '@/components/shared/EnvStatusBar';
import { Notice } from '@/components/shared/Notice';
import { StatusDot } from '@/components/shared/StatusDot';
import { Skeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { listAgents } from '@/lib/api/agent';
import { tryRefreshDoctor } from '@/lib/api/doctor';
import { listRuntimes } from '@/lib/api/env';
import { importCurrentLogin } from '@/lib/api/account';
import { AGENTS } from '@/config/agents';
import { RUNTIME_MAP } from '@/config/runtimes';
import { isCapabilityBlocked } from '@/lib/capability';
import { hasEnvIssues } from '@/lib/env';
import { loadBool, saveBool, StorageKey } from '@/lib/storage';
import type { AgentStatus, RuntimeDetect } from '@/lib/types';
import { cn } from '@/lib/utils';

type Step = 'env' | 'detect' | 'import' | 'done';

/**
 * 首次启动引导(docs/ui-design.md §7):
 * Step A 检测共享环境 → Step B 检测 agent → 导入登录态 → Dashboard。
 */
export function OnboardingDialog() {
  const navigate = useNavigate();
  const { toast } = useToast();
  const [open, setOpen] = React.useState(() => !loadBool(StorageKey.onboardingDone));
  const [step, setStep] = React.useState<Step>('env');
  const [runtimes, setRuntimes] = React.useState<RuntimeDetect[] | null>(null);
  const [agents, setAgents] = React.useState<AgentStatus[] | null>(null);
  const [importing, setImporting] = React.useState(false);
  const [imported, setImported] = React.useState(0);
  const [envLoading, setEnvLoading] = React.useState(false);

  // Step A: 环境检测
  React.useEffect(() => {
    if (!open || step !== 'env') return;
    let cancelled = false;
    setEnvLoading(true);
    listRuntimes()
      .then((list) => {
        if (!cancelled) setRuntimes(list);
      })
      .catch(() => {
        if (!cancelled) setRuntimes([]);
      })
      .finally(() => {
        if (!cancelled) setEnvLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, step]);

  // Step B: agent 检测(进入 detect 时)
  React.useEffect(() => {
    if (!open || step !== 'detect') return;
    let cancelled = false;
    listAgents()
      .then((list) => {
        if (!cancelled) {
          setAgents(list);
          setStep('import');
        }
      })
      .catch(() => {
        if (!cancelled) {
          setAgents([]);
          setStep('import');
        }
      });
    return () => {
      cancelled = true;
    };
  }, [open, step]);

  const finish = (go?: string) => {
    saveBool(StorageKey.onboardingDone, true);
    setOpen(false);
    if (go) navigate(go);
  };

  const refreshEnv = async () => {
    setEnvLoading(true);
    try {
      const forced = await tryRefreshDoctor();
      setRuntimes(forced?.runtimes ?? (await listRuntimes()));
    } finally {
      setEnvLoading(false);
    }
  };

  const handleImport = async () => {
    if (!agents) return;
    setImporting(true);
    let count = 0;
    try {
      const targets = agents.filter(
        (a) =>
          a.installed &&
          !a.hidden &&
          !isCapabilityBlocked(a.capabilities?.accountSwitch),
      );
      for (const a of targets) {
        try {
          await importCurrentLogin(a.agentId);
          count += 1;
        } catch {
          // 无 live 凭据时跳过
        }
      }
      setImported(count);
      setStep('done');
      toast({
        title: count > 0 ? `已导入 ${count} 个账号` : '未发现可导入的登录态',
        description: count > 0 ? '可在 Connections → 账号与密钥 查看并切换' : '可稍后手动添加账号',
        variant: count > 0 ? 'success' : 'default',
      });
    } finally {
      setImporting(false);
    }
  };

  const installed = agents?.filter((a) => a.installed && !a.hidden) ?? [];
  const visibleMetas = AGENTS.filter(
    (m) => !agents?.some((a) => a.agentId === m.id && a.hidden),
  );
  const envIssues = runtimes ? hasEnvIssues(runtimes) : false;
  const noAgents = agents != null && installed.length === 0;

  return (
    <Dialog open={open} onOpenChange={(v) => !v && finish()}>
      <DialogContent className="max-w-lg" onPointerDownOutside={(e) => e.preventDefault()}>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles className="h-5 w-5 text-accent" />
            欢迎使用 AgentHub
          </DialogTitle>
          <DialogDescription>
            统一管理 Claude Code、Codex、Kimi、Grok。安装 Agent 前会先检查本机运行环境。
          </DialogDescription>
        </DialogHeader>

        {/* Step A: 环境 */}
        {step === 'env' && (
          <div className="space-y-3 py-1">
            <p className="text-sm text-secondary">
              Step 1/2 · 检测共享运行环境(Node / npm 等)。缺失时仍可进入应用,稍后在 Agents 页修复。
            </p>
            {runtimes == null || envLoading ? (
              <div className="space-y-2">
                <Skeleton className="h-10 w-full rounded-card" />
                <Skeleton className="h-4 w-2/3" />
              </div>
            ) : (
              <>
                <EnvStatusBar
                  runtimes={runtimes}
                  loading={envLoading}
                  onRefresh={() => void refreshEnv()}
                />
                {envIssues ? (
                  <Notice tone="warning">
                    <p className="font-medium text-warning">部分环境未就绪</p>
                    <ul className="mt-1 list-inside list-disc text-muted">
                      {runtimes
                        .filter((r) => r.status !== 'ok')
                        .map((r) => (
                          <li key={r.id}>
                            {RUNTIME_MAP[r.id].name}
                            {r.minRequired ? ` (需要 ≥ ${r.minRequired})` : ''}
                          </li>
                        ))}
                    </ul>
                    <p className="mt-1.5 text-muted">
                      可先继续；需要装 Codex 等 npm 渠道时，请到 Agents 页安装 Node.js。
                    </p>
                  </Notice>
                ) : (
                  <Notice tone="success">运行环境就绪</Notice>
                )}
              </>
            )}
          </div>
        )}

        {step === 'detect' && (
          <div className="space-y-3 py-2">
            <p className="text-sm text-secondary">Step 2/2 · 正在检测本机已安装的 agent…</p>
            <div className="space-y-2">
              {AGENTS.map((m) => (
                <div key={m.id} className="flex items-center gap-3">
                  <Skeleton className="h-8 w-8 rounded-full" />
                  <Skeleton className="h-4 w-32" />
                </div>
              ))}
            </div>
          </div>
        )}

        {(step === 'import' || step === 'done') && agents && (
          <div className="space-y-4 py-1">
            {runtimes && hasEnvIssues(runtimes) && (
              <Notice tone="warning">
                运行环境仍有问题。未安装的 Agent 可能需要先修复环境。
                {noAgents && ' 建议引导结束后前往 Agents 页。'}
              </Notice>
            )}
            <div>
              <p className="mb-2 text-sm text-secondary">
                检测到 {installed.length}/{visibleMetas.length} 个已安装
              </p>
              <ul className="divide-y divide-border rounded-card border border-border">
                {visibleMetas.map((agentMeta) => {
                  const status = agents.find((a) => a.agentId === agentMeta.id);
                  const missing = !status?.installed;
                  const envBad = missing && status && status.envReady === false;
                  return (
                    <li
                      key={agentMeta.id}
                      className={cn('flex items-center gap-3 px-3 py-2.5', missing && 'opacity-70')}
                    >
                      <AgentLogo agentId={agentMeta.id} size="sm" />
                      <div className="min-w-0 flex-1">
                        <p className="text-sm font-medium">{agentMeta.name}</p>
                        <p className="text-xs text-muted">
                          {missing
                            ? envBad
                              ? '未安装 · 环境未就绪'
                              : '未安装'
                            : `v${status?.version ?? '—'}`}
                        </p>
                      </div>
                      {missing && envBad && <Badge variant="warning">环境</Badge>}
                      {!missing && status && <StatusDot status={status.authStatus} withLabel />}
                    </li>
                  );
                })}
              </ul>
            </div>

            {step === 'import' && (
              <p className="text-xs text-muted">
                可一键导入已安装 agent 的当前登录态到账号池;也可跳过,稍后在 Connections → 账号与密钥
                手动添加。
              </p>
            )}

            {step === 'done' && (
              <Notice tone="success" className="text-sm">
                {imported > 0
                  ? `已导入 ${imported} 个账号，可直接使用。`
                  : '引导完成。可从侧栏进入各功能页。'}
              </Notice>
            )}
          </div>
        )}

        <DialogFooter>
          {step === 'env' && (
            <>
              <Button variant="ghost" onClick={() => finish()} disabled={envLoading}>
                跳过引导
              </Button>
              {envIssues ? (
                <>
                  <Button variant="outline" onClick={() => setStep('detect')} disabled={runtimes == null}>
                    稍后修复环境
                  </Button>
                  <Button
                    onClick={() => {
                      finish('/agents');
                    }}
                    disabled={runtimes == null}
                  >
                    <Wrench className="h-4 w-4" />
                    去 Agents 修复
                  </Button>
                </>
              ) : (
                <Button onClick={() => setStep('detect')} disabled={runtimes == null || envLoading}>
                  继续检测 Agent
                </Button>
              )}
            </>
          )}
          {step === 'detect' && (
            <Button variant="ghost" disabled>
              检测中…
            </Button>
          )}
          {step === 'import' && (
            <>
              <Button variant="ghost" onClick={() => finish(noAgents && envIssues ? '/agents' : undefined)}>
                跳过
              </Button>
              {noAgents && envIssues ? (
                <Button onClick={() => finish('/agents')}>
                  <Wrench className="h-4 w-4" />
                  去安装环境与 Agent
                </Button>
              ) : (
                <Button
                  disabled={importing || installed.length === 0}
                  onClick={() => void handleImport()}
                >
                  <DownloadCloud className="h-4 w-4" />
                  {importing ? '导入中…' : '导入现有登录态'}
                </Button>
              )}
            </>
          )}
          {step === 'done' && (
            <>
              <Button variant="outline" onClick={() => finish('/connections')}>
                查看账号
              </Button>
              <Button onClick={() => finish(noAgents ? '/agents' : '/')}>
                {noAgents ? '去 Agents' : '进入 Dashboard'}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
