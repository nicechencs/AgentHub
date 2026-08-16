import * as React from 'react';
import { Download, RefreshCw } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { useToast } from '@/components/ui/toast';
import { setAppUpdateAvailable } from '@/app/runtime';
import {
  checkForUpdate,
  downloadAndInstallUpdate,
  isUpdateAvailable,
  type UpdateInfo,
} from '@/lib/api/update';
import { loadBool, loadString, saveString, StorageKey } from '@/lib/ui-preferences';
import { logger } from '@/lib/logger';

const log = logger.scope('ui:update');

/** Wait for splash / onboarding before silent auto-check. */
const AUTO_CHECK_DELAY_MS = 4_000;

type InstallPhase = 'idle' | 'downloading' | 'installing';

export interface UpdatePromptHandle {
  /** Manual check (settings / toast action). Opens dialog when an update exists. */
  checkNow: (opts?: { quietWhenCurrent?: boolean }) => Promise<UpdateInfo | null>;
}

interface UpdatePromptProps {
  /** Called when the component mounts so parents can trigger manual checks. */
  onReady?: (handle: UpdatePromptHandle) => void;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

function wasDismissed(version: string): boolean {
  const dismissed = loadString(StorageKey.updateDismissedVersion, '');
  return dismissed === version;
}

/**
 * Startup update prompt + one-click install.
 * Silent auto-check after boot; settings can call `checkNow` via `onReady`.
 */
export function UpdatePrompt({ onReady }: UpdatePromptProps) {
  const { toast } = useToast();
  const [open, setOpen] = React.useState(false);
  const [info, setInfo] = React.useState<UpdateInfo | null>(null);
  const [phase, setPhase] = React.useState<InstallPhase>('idle');
  const [percent, setPercent] = React.useState<number | null>(null);
  const [downloadedLabel, setDownloadedLabel] = React.useState<string>('');
  const [error, setError] = React.useState<string | null>(null);
  const installingRef = React.useRef(false);

  const presentUpdate = React.useCallback((next: UpdateInfo) => {
    setAppUpdateAvailable(next);
    setInfo(next);
    setError(null);
    setPhase('idle');
    setPercent(null);
    setDownloadedLabel('');
    setOpen(true);
  }, []);

  const checkNow = React.useCallback(
    async (opts?: { quietWhenCurrent?: boolean }) => {
      const quiet = opts?.quietWhenCurrent ?? false;
      try {
        const available = await isUpdateAvailable();
        if (!available) {
          if (!quiet) {
            toast({
              title: '无法检查更新',
              description: '仅桌面端支持自动更新',
              variant: 'danger',
            });
          }
          return null;
        }
        const next = await checkForUpdate();
        if (!next) {
          setAppUpdateAvailable(null);
          if (!quiet) {
            toast({ title: '已是最新版本', variant: 'success' });
          }
          return null;
        }
        presentUpdate(next);
        return next;
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        log.error('checkNow failed', e);
        // quietWhenCurrent only suppresses the “already latest” toast (Settings owns that).
        // Errors must still surface — either here or via rethrow to the caller.
        if (!quiet) {
          toast({
            title: '检查更新失败',
            description: msg,
            variant: 'danger',
          });
        }
        throw e instanceof Error ? e : new Error(msg);
      }
    },
    [presentUpdate, toast],
  );

  React.useEffect(() => {
    onReady?.({ checkNow });
  }, [checkNow, onReady]);

  // Auto-check once after boot (skip when onboarding still open).
  React.useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void (async () => {
        if (cancelled) return;
        // Avoid fighting the first-run wizard.
        if (!loadBool(StorageKey.onboardingDone, false)) return;
        try {
          if (!(await isUpdateAvailable())) return;
          const next = await checkForUpdate();
          if (cancelled || !next) return;
          // Always publish for nav/about badge; only skip dialog when user dismissed this version.
          setAppUpdateAvailable(next);
          if (wasDismissed(next.version)) return;
          presentUpdate(next);
        } catch (e) {
          // Silent on auto-check network / feed errors.
          log.warn('auto check skipped', e);
        }
      })();
    }, AUTO_CHECK_DELAY_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [presentUpdate]);

  const onDismiss = () => {
    if (installingRef.current) return;
    if (info?.version) {
      saveString(StorageKey.updateDismissedVersion, info.version);
    }
    setOpen(false);
  };

  const onInstall = () => {
    if (installingRef.current) return;
    installingRef.current = true;
    setPhase('downloading');
    setError(null);
    setPercent(0);
    void (async () => {
      try {
        await downloadAndInstallUpdate((p) => {
          if (p.percent != null) {
            setPercent(p.percent);
          } else {
            setPercent(null);
          }
          if (p.total != null) {
            setDownloadedLabel(`${formatBytes(p.downloaded)} / ${formatBytes(p.total)}`);
          } else if (p.downloaded > 0) {
            setDownloadedLabel(formatBytes(p.downloaded));
          }
          if (p.percent != null && p.percent >= 100) {
            setPhase('installing');
          }
        });
        setPhase('installing');
        setAppUpdateAvailable(null);
        // relaunch() should terminate; if we are still here, show a soft message.
        toast({
          title: '更新已安装',
          description: '请手动重启应用以完成更新',
          variant: 'success',
        });
        setOpen(false);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        log.error('install failed', e);
        setError(msg);
        setPhase('idle');
        toast({
          title: '更新失败',
          description: msg,
          variant: 'danger',
        });
      } finally {
        installingRef.current = false;
      }
    })();
  };

  const busy = phase === 'downloading' || phase === 'installing';

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onDismiss();
        else setOpen(true);
      }}
    >
      <DialogContent
        className="max-w-md"
        hideClose={busy}
        onPointerDownOutside={(e) => {
          if (busy) e.preventDefault();
        }}
        onEscapeKeyDown={(e) => {
          if (busy) e.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <RefreshCw className="h-4 w-4 text-accent" />
            发现新版本
          </DialogTitle>
          <DialogDescription>
            {info ? (
              <>
                当前 <span className="font-mono">v{info.currentVersion}</span>
                {' → '}
                <span className="font-mono text-primary">v{info.version}</span>
              </>
            ) : (
              '有可用更新'
            )}
          </DialogDescription>
        </DialogHeader>

        {info?.notes ? (
          <div className="max-h-40 overflow-y-auto rounded-btn border border-border bg-canvas/60 p-3 text-xs leading-relaxed text-secondary whitespace-pre-wrap">
            {info.notes}
          </div>
        ) : (
          <p className="text-xs text-muted">建议更新以获得修复与新功能。安装过程中应用将重启。</p>
        )}

        {busy && (
          <div className="space-y-2">
            <div className="flex items-center justify-between text-xs text-muted">
              <span>{phase === 'installing' ? '正在安装并重启…' : '正在下载更新…'}</span>
              <span className="font-mono">
                {percent != null ? `${percent}%` : downloadedLabel || '…'}
              </span>
            </div>
            <Progress value={percent ?? (phase === 'installing' ? 100 : 15)} />
            {downloadedLabel && percent != null && (
              <p className="text-2xs text-muted">{downloadedLabel}</p>
            )}
          </div>
        )}

        {error && (
          <p className="text-xs text-danger" role="alert">
            {error}
          </p>
        )}

        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={onDismiss}>
            稍后
          </Button>
          <Button disabled={busy} onClick={onInstall}>
            <Download className="mr-1.5 h-3.5 w-3.5" />
            {phase === 'downloading'
              ? '下载中…'
              : phase === 'installing'
                ? '安装中…'
                : '一键更新'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
