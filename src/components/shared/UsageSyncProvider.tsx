import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';

import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { getSettings } from '@/lib/api/settings';
import { collectUsage, getUsageAvailability } from '@/lib/api/usage';
import {
  buildUsageSyncStatusLine,
  computeAutoRetryAt,
  computeNextCollectAt,
  loadLastCollectAt,
  normalizeIntervalMin,
  notifyUsageCollected,
  saveLastCollectAt,
  USAGE_SYNC_SETTINGS_CHANGED,
  type UsageCollectSource,
} from '@/lib/usage-sync';

/** Grace delay when overdue so first paint / availability check can settle. */
const OVERDUE_GRACE_MS = 2_000;
const TICK_MS = 1_000;

export interface UsageSyncContextValue {
  intervalMin: number;
  lastCollectAt: number | null;
  nextCollectAt: number | null;
  collecting: boolean;
  collectPct: number;
  collectSource: UsageCollectSource | null;
  statusLine: string;
  /** Manual collect (toasts always). */
  manualCollect: () => Promise<void>;
  reloadSettings: () => Promise<void>;
}

const UsageSyncContext = createContext<UsageSyncContextValue | null>(null);

export function useUsageSync(): UsageSyncContextValue {
  const ctx = useContext(UsageSyncContext);
  if (!ctx) {
    throw new Error('useUsageSync must be used within UsageSyncProvider');
  }
  return ctx;
}

export function UsageSyncProvider({ children }: { children: ReactNode }) {
  const { toast } = useToast();
  const { t } = useI18n();
  const [intervalMin, setIntervalMin] = useState(0);
  const [lastCollectAt, setLastCollectAt] = useState<number | null>(() => loadLastCollectAt());
  const [nextCollectAt, setNextCollectAt] = useState<number | null>(null);
  const [autoRetryAt, setAutoRetryAt] = useState<number | null>(null);
  const [collecting, setCollecting] = useState(false);
  const [collectPct, setCollectPct] = useState(0);
  const [collectSource, setCollectSource] = useState<UsageCollectSource | null>(null);
  const [nowTick, setNowTick] = useState(() => Date.now());
  /** Bumped on visibility resume so the schedule effect re-arms. */
  const [scheduleGen, setScheduleGen] = useState(0);

  const collectingRef = useRef(false);
  const intervalMinRef = useRef(0);
  const lastCollectAtRef = useRef(lastCollectAt);
  const autoRetryAtRef = useRef(autoRetryAt);
  const autoLastAttemptAtRef = useRef<number | null>(null);
  const autoFailureCountRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    intervalMinRef.current = intervalMin;
  }, [intervalMin]);
  useEffect(() => {
    lastCollectAtRef.current = lastCollectAt;
  }, [lastCollectAt]);
  useEffect(() => {
    autoRetryAtRef.current = autoRetryAt;
  }, [autoRetryAt]);

  const clearTimer = useCallback(() => {
    if (timerRef.current != null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const recomputeNext = useCallback(
    (last: number | null, mins: number, now = Date.now()) => {
      const next = computeNextCollectAt(last, mins, now);
      setNextCollectAt(next);
      return next;
    },
    [],
  );

  const reloadSettings = useCallback(async () => {
    try {
      const s = await getSettings();
      const mins = normalizeIntervalMin(s.usageCollectIntervalMin);
      setIntervalMin(mins);
      autoFailureCountRef.current = 0;
      autoRetryAtRef.current = null;
      setAutoRetryAt(null);
      recomputeNext(lastCollectAtRef.current, mins);
    } catch {
      // keep previous interval
    }
  }, [recomputeNext]);

  const runCollect = useCallback(
    async (source: UsageCollectSource) => {
      if (collectingRef.current) return;
      collectingRef.current = true;
      setCollecting(true);
      setCollectSource(source);
      setCollectPct(source === 'manual' ? 0 : 5);
      clearTimer();
      const attemptAt = Date.now();
      if (source === 'auto') autoLastAttemptAtRef.current = attemptAt;

      try {
        const availability = await getUsageAvailability();
        if (availability.status === 'unavailable') {
          // An unavailable source is not a transient 2s overdue condition:
          // defer until the next normal cycle (or an explicit manual run).
          autoFailureCountRef.current = 0;
          const existingRetry = autoRetryAtRef.current;
          const retryAt =
            existingRetry != null && existingRetry > attemptAt
              ? existingRetry
              : intervalMinRef.current > 0
                ? attemptAt + intervalMinRef.current * 60_000
                : null;
          autoRetryAtRef.current = retryAt;
          setAutoRetryAt(retryAt);
          setNextCollectAt(retryAt);
          if (source === 'manual') {
            toast({
              title: t('dashboard.page.usageUnavailableTitle'),
              description: availability.reason,
              variant: 'danger',
            });
          }
          return;
        }

        const result = await collectUsage((pct) => setCollectPct(pct));
        const at = Date.now();
        saveLastCollectAt(at);
        setLastCollectAt(at);
        lastCollectAtRef.current = at;
        autoFailureCountRef.current = 0;
        autoRetryAtRef.current = null;
        setAutoRetryAt(null);
        recomputeNext(at, intervalMinRef.current, at);

        const inserted =
          result && typeof result === 'object' && 'inserted' in result
            ? Number(result.inserted) || 0
            : undefined;
        const missing =
          result && typeof result === 'object' && 'missingPricingModels' in result
            ? (result.missingPricingModels as string[] | undefined) ?? []
            : [];

        notifyUsageCollected({ source, inserted, at });

        if (source === 'manual') {
          const models = `${missing.slice(0, 4).join(', ')}${missing.length > 4 ? '…' : ''}`;
          toast({
            title: t('dashboard.sync.toastManualDone'),
            description:
              missing.length > 0
                ? t('dashboard.sync.toastManualMissing', {
                    inserted: inserted != null ? t('dashboard.sync.toastInsertedParen', { n: inserted }) : '',
                    models,
                  })
                : inserted != null
                  ? t('dashboard.sync.toastManualInserted', { n: inserted })
                  : t('dashboard.sync.toastManualGeneric'),
            variant: missing.length > 0 ? 'default' : 'success',
          });
        } else if (inserted != null && inserted > 0) {
          const models = `${missing.slice(0, 3).join(', ')}${missing.length > 3 ? '…' : ''}`;
          toast({
            title: t('dashboard.sync.toastAutoDone'),
            description:
              missing.length > 0
                ? t('dashboard.sync.toastAutoMissing', { n: inserted, models })
                : t('dashboard.sync.toastAutoInserted', { n: inserted }),
            variant: 'success',
          });
        }
      } catch (e) {
        if (source === 'manual') {
          toast({
            title: t('dashboard.sync.toastCollectFailed'),
            description: e instanceof Error ? e.message : String(e),
            variant: 'danger',
          });
        }
        if (source === 'auto') {
          const failureCount = ++autoFailureCountRef.current;
          const retryAt = computeAutoRetryAt(
            autoLastAttemptAtRef.current ?? Date.now(),
            intervalMinRef.current,
            failureCount,
          );
          autoRetryAtRef.current = retryAt;
          setAutoRetryAt(retryAt);
          setNextCollectAt(retryAt);
        } else {
          // A manual failure must not cause the overdue auto timer to fire
          // again after its 2s grace period.
          const existingRetry = autoRetryAtRef.current;
          const retryAt =
            existingRetry != null && existingRetry > Date.now()
              ? existingRetry
              : intervalMinRef.current > 0
                ? Date.now() + intervalMinRef.current * 60_000
                : null;
          autoRetryAtRef.current = retryAt;
          setAutoRetryAt(retryAt);
          setNextCollectAt(retryAt);
        }
      } finally {
        collectingRef.current = false;
        setCollecting(false);
        setCollectSource(null);
        setCollectPct(0);
      }
    },
    [clearTimer, recomputeNext, toast, t],
  );

  const manualCollect = useCallback(async () => {
    await runCollect('manual');
  }, [runCollect]);

  // Load interval on mount + settings change
  useEffect(() => {
    void reloadSettings();
    const onSettings = () => {
      void reloadSettings();
    };
    window.addEventListener(USAGE_SYNC_SETTINGS_CHANGED, onSettings);
    return () => window.removeEventListener(USAGE_SYNC_SETTINGS_CHANGED, onSettings);
  }, [reloadSettings]);

  // 1s tick for countdown labels
  useEffect(() => {
    const id = window.setInterval(() => setNowTick(Date.now()), TICK_MS);
    return () => window.clearInterval(id);
  }, []);

  // Schedule auto collect while document is visible
  useEffect(() => {
    clearTimer();
    if (collecting) return;
    if (normalizeIntervalMin(intervalMin) <= 0) {
      setNextCollectAt(null);
      return;
    }
    if (typeof document !== 'undefined' && document.visibilityState === 'hidden') {
      return;
    }

    const now = Date.now();
    const retryAt = autoRetryAtRef.current;
    let next = retryAt != null && retryAt > now
      ? retryAt
      : computeNextCollectAt(lastCollectAt, intervalMin, now);
    if (next != null && next <= now) {
      next = now + OVERDUE_GRACE_MS;
    }
    setNextCollectAt(next);
    if (next == null) return;

    const delay = Math.max(0, next - now);
    timerRef.current = setTimeout(() => {
      if (typeof document !== 'undefined' && document.visibilityState === 'hidden') {
        return;
      }
      void runCollect('auto');
    }, delay);

    return clearTimer;
  }, [intervalMin, lastCollectAt, collecting, autoRetryAt, scheduleGen, clearTimer, runCollect]);

  // Pause / resume on visibility
  useEffect(() => {
    const onVis = () => {
      if (document.visibilityState === 'hidden') {
        clearTimer();
        return;
      }
      setNowTick(Date.now());
      setScheduleGen((g) => g + 1);
    };
    document.addEventListener('visibilitychange', onVis);
    return () => document.removeEventListener('visibilitychange', onVis);
  }, [clearTimer]);

  const statusLine = useMemo(
    () =>
      buildUsageSyncStatusLine({
        lastCollectAt,
        nextCollectAt,
        intervalMin,
        collecting,
        now: nowTick,
        t,
      }),
    [lastCollectAt, nextCollectAt, intervalMin, collecting, nowTick, t],
  );

  const value = useMemo<UsageSyncContextValue>(
    () => ({
      intervalMin,
      lastCollectAt,
      nextCollectAt,
      collecting,
      collectPct,
      collectSource,
      statusLine,
      manualCollect,
      reloadSettings,
    }),
    [
      intervalMin,
      lastCollectAt,
      nextCollectAt,
      collecting,
      collectPct,
      collectSource,
      statusLine,
      manualCollect,
      reloadSettings,
    ],
  );

  return <UsageSyncContext.Provider value={value}>{children}</UsageSyncContext.Provider>;
}
