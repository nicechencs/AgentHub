/**
 * Connections row: import this login into the default connection pool.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useToast } from '@/components/ui/toast';
import { listDefaultRoutePools, syncConnectionAuthorizations } from '@/lib/api/adapter';
import type { DefaultRoutePoolOverview } from '@/lib/backend/contracts/adapter';
import type { TicketView } from '@/lib/backend/contracts/ticket';
import type { TranslateFn } from '@/lib/i18n';
import {
  importedSourceKeys,
  resolveTicketPoolImportAction,
  ticketPoolImportKey,
  type TicketBindAction,
} from './ticket-pool-import';

export function useTicketPoolImport(input: { t: TranslateFn }): {
  importActionForTicket: (ticket: TicketView) => TicketBindAction;
  handleImportToPool: (ticket: TicketView) => Promise<void>;
  importingTicketId: string | null;
} {
  const { t } = input;
  const { toast } = useToast();
  const [poolEnabled, setPoolEnabled] = useState(true);
  const [pools, setPools] = useState<DefaultRoutePoolOverview[]>([]);
  const [importingTicketId, setImportingTicketId] = useState<string | null>(null);
  const inFlight = useRef(false);
  const gen = useRef(0);

  const applyListed = useCallback((listed: { enabled: boolean; pools: DefaultRoutePoolOverview[] }) => {
    setPoolEnabled(listed.enabled);
    setPools(listed.pools);
  }, []);

  useEffect(() => {
    let cancelled = false;
    void listDefaultRoutePools()
      .then((listed) => {
        if (!cancelled) applyListed(listed);
      })
      .catch(() => {
        if (cancelled) return;
        setPoolEnabled(true);
        setPools([]);
      });
    return () => {
      cancelled = true;
    };
  }, [applyListed]);

  const keys = useMemo(() => importedSourceKeys(pools), [pools]);

  const importActionForTicket = useCallback((ticket: TicketView): TicketBindAction => (
    resolveTicketPoolImportAction(ticket, {
      poolEnabled,
      alreadyImported: keys.has(ticketPoolImportKey(ticket)),
    }, t)
  ), [keys, poolEnabled, t]);

  const handleImportToPool = useCallback(async (ticket: TicketView) => {
    if (inFlight.current) return;
    if (importActionForTicket(ticket).disabled) return;
    inFlight.current = true;
    const generation = ++gen.current;
    setImportingTicketId(ticket.id);
    try {
      const result = await syncConnectionAuthorizations({
        sources: [{ sourceKind: ticket.sourceKind, sourceId: ticket.sourceId }],
      });
      const listed = await listDefaultRoutePools().catch(() => null);
      if (generation !== gen.current) return;
      if (listed) applyListed(listed);
      toast({
        title: result.added > 0
          ? t('connections.list.importToPoolOk')
          : t('connections.list.importToPoolAlready'),
        variant: 'success',
      });
    } catch (error) {
      if (generation !== gen.current) return;
      toast({
        title: t('connections.list.importToPoolFail'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      inFlight.current = false;
      if (generation === gen.current) setImportingTicketId(null);
    }
  }, [applyListed, importActionForTicket, t, toast]);

  return { importActionForTicket, handleImportToPool, importingTicketId };
}
