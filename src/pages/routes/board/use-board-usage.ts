/**
 * Board usage hook: loads local-gateway request rows for the selected window.
 */
import { useEffect, useState } from 'react';
import { gatewayUsageQuery } from '@/lib/api/usage';
import type { GatewayUsageRow } from '@/lib/backend/contracts/usage-types';

export type BoardUsageState =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'ready'; rows: GatewayUsageRow[]; refreshing: boolean }
  | { status: 'unavailable'; reason: string | null }
  | { status: 'error' };

export function useBoardUsageStats(input: {
  enabled: boolean;
  since: string;
  refreshKey?: number;
}): BoardUsageState {
  const { enabled, since, refreshKey = 0 } = input;
  const [state, setState] = useState<BoardUsageState>({ status: 'idle' });

  useEffect(() => {
    let cancelled = false;
    if (!enabled) {
      setState({ status: 'idle' });
      return;
    }
    setState((prev) =>
      prev.status === 'ready' ? { ...prev, refreshing: true } : { status: 'loading' },
    );
    void (async () => {
      try {
        const rows = await gatewayUsageQuery({ since, limit: 100_000 });
        if (cancelled) return;
        setState({ status: 'ready', rows, refreshing: false });
      } catch {
        if (!cancelled) setState({ status: 'error' });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [enabled, since, refreshKey]);

  return state;
}
