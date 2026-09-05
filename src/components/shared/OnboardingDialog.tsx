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
import { useI18n } from '@/components/shared/LanguageProvider';
import { Notice } from '@/components/shared/Notice';
import { StatusDot } from '@/components/shared/StatusDot';
import { useSidebar } from '@/components/layout/SidebarContext';
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
import { loadBool, saveBool, StorageKey } from '@/lib/ui-preferences';
import type { AgentStatus, RuntimeDetect } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  EMPTY_ONBOARDING_USAGE,
  hasOnboardingUsageChoice,
  navVisibilityForUsage,
  toggleOnboardingUsage,
  type OnboardingUsageSelection,
} from './onboarding-model';
import { notifyOnboardingFinished } from './chrome-hint-model';
import { OnboardingUsageStep } from './OnboardingUsageStep';

type Step = 'usage' | 'env' | 'detect' | 'import' | 'done';

/**
 * 首次启动引导:
 * 选择本地路由 / Sub2API → 检测共享环境 → 检测 agent → 导入登录态。
 */
export function OnboardingDialog() {
  const navigate = useNavigate();
  const { t } = useI18n();
  const { toast } = useToast();
  const { setRoutesNavVisible, setSub2apiNavVisible } = useSidebar();
  const [open, setOpen] = React.useState(() => !loadBool(StorageKey.onboardingDone));
  const [step, setStep] = React.useState<Step>('usage');
  const [usage, setUsage] = React.useState<OnboardingUsageSelection>(EMPTY_ONBOARDING_USAGE);
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

  const applyUsage = (selection: OnboardingUsageSelection = usage) => {
    if (!hasOnboardingUsageChoice(selection)) return;
    const visibility = navVisibilityForUsage(selection);
    setRoutesNavVisible(visibility.routesNavVisible);
    setSub2apiNavVisible(visibility.sub2apiNavVisible);
  };

  const finish = (go?: string) => {
    applyUsage();
    saveBool(StorageKey.onboardingDone, true);
    notifyOnboardingFinished();
    setOpen(false);
    if (go) navigate(go);
  };

  const continueFromUsage = () => {
    applyUsage();
    setStep('env');
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
        title: count > 0 ? t('chrome.onboarding.toastImported', { n: count }) : t('chrome.onboarding.toastNone'),
        description: count > 0 ? t('chrome.onboarding.toastImportedDesc') : t('chrome.onboarding.toastNoneDesc'),
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
            {t('chrome.onboarding.title')}
          </DialogTitle>
          <DialogDescription>
            {step === 'usage'
              ? t('chrome.onboarding.usageDescription')
              : t('chrome.onboarding.description')}
          </DialogDescription>
        </DialogHeader>

        {step === 'usage' && (
          <OnboardingUsageStep
            selection={usage}
            onToggle={(id) => setUsage((current) => toggleOnboardingUsage(current, id))}
          />
        )}

        {/* Step A: 环境 */}
        {step === 'env' && (
          <div className="space-y-3 py-1">
            <p className="text-sm text-secondary">
              {t('chrome.onboarding.stepEnv')}
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
                    <p className="font-medium text-warning">{t('chrome.onboarding.envPartialTitle')}</p>
                    <ul className="mt-1 list-inside list-disc text-muted">
                      {runtimes
                        .filter((r) => r.status !== 'ok')
                        .map((r) => (
                          <li key={r.id}>
                            {RUNTIME_MAP[r.id].name}
                            {r.minRequired ? t('chrome.onboarding.envNeedMin', { min: r.minRequired }) : ''}
                          </li>
                        ))}
                    </ul>
                    <p className="mt-1.5 text-muted">
                      {t('chrome.onboarding.envPartialHint')}
                    </p>
                  </Notice>
                ) : (
                  <Notice tone="success">{t('chrome.onboarding.envReady')}</Notice>
                )}
              </>
            )}
          </div>
        )}

        {step === 'detect' && (
          <div className="space-y-3 py-2">
            <p className="text-sm text-secondary">{t('chrome.onboarding.stepDetect')}</p>
            <div className="space-y-2">
              {AGENTS.map((m) => (
                <div key={m.id} className="flex items-center gap-3">
                  <Skeleton className="h-8 w-8 rounded-mark" />
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
                {t('chrome.onboarding.envStillIssues')}
                {noAgents ? t('chrome.onboarding.envStillIssuesGoAgents') : ''}
              </Notice>
            )}
            <div>
              <p className="mb-2 text-sm text-secondary">
                {t('chrome.onboarding.detectedCount', {
                  installed: installed.length,
                  total: visibleMetas.length,
                })}
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
                              ? t('chrome.onboarding.notInstalledEnv')
                              : t('chrome.onboarding.notInstalled')
                            : `v${status?.version ?? '—'}`}
                        </p>
                      </div>
                      {missing && envBad && <Badge variant="warning">{t('chrome.onboarding.envBadge')}</Badge>}
                      {!missing && status && <StatusDot status={status.authStatus} withLabel />}
                    </li>
                  );
                })}
              </ul>
            </div>

            {step === 'import' && (
              <p className="text-xs text-muted">
                {t('chrome.onboarding.importHint')}
              </p>
            )}

            {step === 'done' && (
              <Notice tone="success" className="text-sm">
                {imported > 0
                  ? t('chrome.onboarding.importedDone', { n: imported })
                  : t('chrome.onboarding.doneNoImport')}
              </Notice>
            )}
          </div>
        )}

        <DialogFooter>
          {step === 'usage' && (
            <>
              {!hasOnboardingUsageChoice(usage) ? (
                <p className="mr-auto self-center text-meta text-muted">
                  {t('chrome.onboarding.usageNeedOne')}
                </p>
              ) : null}
              <Button variant="ghost" onClick={() => finish()}>
                {t('chrome.onboarding.skipGuide')}
              </Button>
              <Button onClick={continueFromUsage} disabled={!hasOnboardingUsageChoice(usage)}>
                {t('chrome.onboarding.usageContinue')}
              </Button>
            </>
          )}
          {step === 'env' && (
            <>
              <Button variant="ghost" onClick={() => finish()} disabled={envLoading}>
                {t('chrome.onboarding.skipGuide')}
              </Button>
              {envIssues ? (
                <>
                  <Button variant="outline" onClick={() => setStep('detect')} disabled={runtimes == null}>
                    {t('chrome.onboarding.fixLater')}
                  </Button>
                  <Button
                    onClick={() => {
                      finish('/agents');
                    }}
                    disabled={runtimes == null}
                  >
                    <Wrench className="h-4 w-4" />
                    {t('chrome.onboarding.goAgentsFix')}
                  </Button>
                </>
              ) : (
                <Button onClick={() => setStep('detect')} disabled={runtimes == null || envLoading}>
                  {t('chrome.onboarding.continueDetect')}
                </Button>
              )}
            </>
          )}
          {step === 'detect' && (
            <>
              <Button variant="ghost" onClick={() => finish()}>
                {t('chrome.onboarding.skipGuide')}
              </Button>
              <Button variant="ghost" disabled>
                {t('chrome.onboarding.detecting')}
              </Button>
            </>
          )}
          {step === 'import' && (
            <>
              <Button variant="ghost" onClick={() => finish(noAgents && envIssues ? '/agents' : undefined)}>
                {t('chrome.onboarding.skip')}
              </Button>
              {noAgents && envIssues ? (
                <Button onClick={() => finish('/agents')}>
                  <Wrench className="h-4 w-4" />
                  {t('chrome.onboarding.goInstall')}
                </Button>
              ) : (
                <Button
                  disabled={importing || installed.length === 0}
                  onClick={() => void handleImport()}
                >
                  <DownloadCloud className="h-4 w-4" />
                  {importing ? t('chrome.onboarding.importing') : t('chrome.onboarding.importLogin')}
                </Button>
              )}
            </>
          )}
          {step === 'done' && (
            <>
              <Button variant="outline" onClick={() => finish('/connections')}>
                {t('chrome.onboarding.viewAccounts')}
              </Button>
              <Button onClick={() => finish(noAgents ? '/agents' : '/')}>
                {noAgents ? t('chrome.onboarding.goAgents') : t('chrome.onboarding.enterDashboard')}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
