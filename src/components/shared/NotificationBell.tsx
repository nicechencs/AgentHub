import * as React from 'react';
import { useNavigate } from 'react-router-dom';
import { AlertTriangle, Bell, CheckCircle2, Info } from 'lucide-react';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Hint } from '@/components/ui/tooltip';
import { listAlerts, dismissAlert } from '@/lib/api/dashboard';
import type { DashboardAlert } from '@/lib/types';
import { cn } from '@/lib/utils';

function LevelIcon({ level }: { level: DashboardAlert['level'] }) {
  if (level === 'danger') return <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-danger" />;
  if (level === 'warning') return <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-warning" />;
  return <Info className="h-3.5 w-3.5 shrink-0 text-info" />;
}

/** 顶栏通知铃:拉取 Dashboard 告警,点击跳转对应页面 */
export function NotificationBell() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [alerts, setAlerts] = React.useState<DashboardAlert[]>([]);
  const [open, setOpen] = React.useState(false);

  const refresh = React.useCallback(() => {
    listAlerts()
      .then(setAlerts)
      .catch(() => setAlerts([]));
  }, []);

  React.useEffect(() => {
    refresh();
    const t = window.setInterval(refresh, 30_000);
    return () => window.clearInterval(t);
  }, [refresh]);

  React.useEffect(() => {
    if (open) refresh();
  }, [open, refresh]);

  const handleClick = async (alert: DashboardAlert) => {
    setOpen(false);
    switch (alert.actionKind) {
      case 'refresh-token':
        navigate('/connections');
        break;
      case 'upgrade':
        navigate('/agents');
        break;
    }
  };

  const handleDismissAll = async () => {
    await Promise.all(alerts.map((a) => dismissAlert(a.id).catch(() => {})));
    setAlerts([]);
  };

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <Hint label={t('chrome.bell.label')}>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            className="relative rounded-btn p-1.5 text-secondary hover:bg-hover hover:text-primary"
            aria-label={t('chrome.bell.label')}
          >
            <Bell className="h-4 w-4" />
            {alerts.length > 0 && (
              <span className="absolute right-0.5 top-0.5 flex h-3.5 min-w-3.5 items-center justify-center rounded-full bg-danger px-0.5 text-meta font-medium text-white">
                {alerts.length > 9 ? '9+' : alerts.length}
              </span>
            )}
          </button>
        </DropdownMenuTrigger>
      </Hint>
      <DropdownMenuContent align="end" className="w-80">
        <DropdownMenuLabel className="flex items-center justify-between">
          <span>{t('chrome.bell.title')}</span>
          {alerts.length > 0 && (
            <button
              type="button"
              className="text-xs font-normal text-muted hover:text-primary"
              onClick={(e) => {
                e.preventDefault();
                void handleDismissAll();
              }}
            >
              {t('chrome.bell.dismissAll')}
            </button>
          )}
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        {alerts.length === 0 ? (
          <div className="flex items-center gap-2 px-2 py-4 text-sm text-secondary">
            <CheckCircle2 className="h-4 w-4 text-success" />
            {t('chrome.bell.empty')}
          </div>
        ) : (
          alerts.map((alert) => (
            <DropdownMenuItem
              key={alert.id}
              className={cn('cursor-pointer items-start gap-2 py-2.5')}
              onSelect={() => void handleClick(alert)}
            >
              <LevelIcon level={alert.level} />
              <div className="min-w-0 flex-1">
                <p className="whitespace-normal text-sm leading-snug">{alert.message}</p>
                <p className="mt-0.5 text-xs text-muted">{alert.actionLabel}</p>
              </div>
            </DropdownMenuItem>
          ))
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
