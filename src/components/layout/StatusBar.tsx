import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { AgentStatusStrip } from '@/components/layout/AgentStatusStrip';
import { pageRhythm } from '@/components/layout/page-rhythm';
import {
  statusBarForwardDotClass,
  statusBarForwardKind,
  statusBarForwardMessageKey,
} from '@/components/layout/status-bar-model';
import { useI18n } from '@/components/shared/LanguageProvider';
import { getLocalGatewayStatus } from '@/lib/api/adapter';
import { onLocalForwardLifecycle } from '@/lib/backend/tauri/local-forward-events';
import { ROUTES_BOARD_PATH } from '@/lib/routes-path';
import { cn } from '@/lib/utils';

/** 窗口底栏：左边已安装 Agent，右边本机转发。 */
export function StatusBar() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [available, setAvailable] = useState(false);
  const [running, setRunning] = useState(false);
  const [restarting, setRestarting] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;

    const apply = (status: { running: boolean; restarting: boolean }) => {
      setAvailable(true);
      setRunning(status.running);
      setRestarting(status.restarting);
    };

    void getLocalGatewayStatus()
      .then((status) => {
        if (!cancelled) apply(status);
      })
      .catch(() => {
        if (!cancelled) setAvailable(false);
      });

    void onLocalForwardLifecycle((payload) => {
      if (payload.phase === 'ready') {
        setRestarting(false);
        void getLocalGatewayStatus()
          .then((status) => {
            if (!cancelled) apply(status);
          })
          .catch(() => undefined);
      } else if (payload.phase === 'restarting') {
        setRestarting(true);
      }
    }).then((fn) => {
      if (cancelled) {
        fn();
        return;
      }
      unsub = fn;
    }).catch(() => {
      // Browser/mock: fail closed instead of installing a fake listener.
    });

    return () => {
      cancelled = true;
      unsub?.();
    };
  }, []);

  const kind = statusBarForwardKind({ available, running, restarting });
  const stateLabel = t(statusBarForwardMessageKey(kind));
  const name = t('chrome.localForward');
  const aria = `${name} ${stateLabel}`;

  return (
    <footer className={pageRhythm.statusBar} data-status-bar="">
      <AgentStatusStrip />
      <button
        type="button"
        className={cn(pageRhythm.statusBarItem, 'ml-auto')}
        aria-label={aria}
        title={aria}
        onClick={() => navigate(ROUTES_BOARD_PATH)}
      >
        <span
          aria-hidden
          className={cn('h-1.5 w-1.5 shrink-0 rounded-full', statusBarForwardDotClass(kind))}
        />
        <span className="truncate">{name}</span>
        <span className="truncate">{stateLabel}</span>
      </button>
    </footer>
  );
}
