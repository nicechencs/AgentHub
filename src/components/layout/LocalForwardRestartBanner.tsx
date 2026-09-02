import { useEffect, useState } from 'react';
import { Notice } from '@/components/shared/Notice';
import { useI18n } from '@/components/shared/LanguageProvider';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { getLocalEntryStatus } from '@/lib/api/adapter';
import {
  onLocalForwardLifecycle,
  type LocalForwardLifecyclePhase,
} from '@/lib/backend/tauri/local-forward-events';
import { cn } from '@/lib/utils';
import {
  localForwardRestartBannerVisible,
  localForwardStartingComingBack,
} from './local-forward-restart';

/**
 * Non-blocking yellow bar while local forwarding is coming back after
 * restore / start. Shown under TopBar in the app shell and in RoutesLayout.
 */
export function LocalForwardRestartBanner() {
  const { t } = useI18n();
  const visible = useLocalForwardRestartBanner();
  if (!visible) return null;
  return (
    <div
      className={cn('shrink-0', pageRhythm.workbenchX, 'py-2')}
      data-local-forward-restart-banner
    >
      <Notice tone="warning">{t('routes.localForward.restarting')}</Notice>
    </div>
  );
}

export function useLocalForwardRestartBanner(): boolean {
  const [restarting, setRestarting] = useState(false);
  const [phase, setPhase] = useState<LocalForwardLifecyclePhase | null>(null);
  const [startingComingBack, setStartingComingBack] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;

    const applyStatus = (status: {
      running: boolean;
      restarting: boolean;
      statuses: readonly { state?: string }[];
    }) => {
      setRestarting(status.restarting);
      setStartingComingBack(localForwardStartingComingBack(status));
    };

    void getLocalEntryStatus()
      .then((status) => {
        if (!cancelled) applyStatus(status);
      })
      .catch(() => undefined);

    void onLocalForwardLifecycle((payload) => {
      setPhase(payload.phase);
      if (payload.phase === 'ready') {
        setRestarting(false);
        setStartingComingBack(false);
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

  useEffect(() => {
    const visible = localForwardRestartBannerVisible({
      restarting,
      phase,
      startingComingBack,
    });
    if (!visible) return;
    const timer = window.setInterval(() => {
      void getLocalEntryStatus()
        .then((status) => {
          setRestarting(status.restarting);
          setStartingComingBack(localForwardStartingComingBack(status));
          if (!status.restarting && !localForwardStartingComingBack(status)) {
            setPhase('ready');
          }
        })
        .catch(() => undefined);
    }, 2000);
    return () => window.clearInterval(timer);
  }, [restarting, phase, startingComingBack]);

  return localForwardRestartBannerVisible({
    restarting,
    phase,
    startingComingBack,
  });
}
