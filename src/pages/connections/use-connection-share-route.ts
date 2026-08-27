import { useCallback } from 'react';
import type { TicketView } from '@/lib/api/tickets';
import type { ConnectFlowEntry } from '@/lib/connect-flow/types';

export type ConnectionConnectInspect = {
  kind: 'connect';
  entry: Extract<ConnectFlowEntry, { mode: 'for-source' }>;
};

function isConnectInspect(
  target: { kind: string } | null,
): target is ConnectionConnectInspect {
  return target != null && target.kind === 'connect' && 'entry' in target;
}

/**
 * Connections 页分享 / 路由侧栏打开。
 * inspect 目标仍是 `{ kind: 'connect', entry: { mode: 'for-source', ... } }`。
 * 打开时不 close inspect；重复点同一入口保持侧栏打开。
 * 不含导入探测、切换 / 绑定 / 删除。
 */
export function useConnectionShareRoute(input: {
  inspectTarget: ConnectionConnectInspect | { kind: string } | null;
  inspectOpen: (next: ConnectionConnectInspect) => void;
  setLoginImportOpen: (open: boolean) => void;
}): {
  handleShareTicket: (ticket: TicketView) => void;
  handleRouteTicket: (ticket: TicketView) => void;
} {
  const { inspectTarget, inspectOpen, setLoginImportOpen } = input;

  const openConnectForTicket = useCallback((ticket: TicketView, purpose: 'share' | 'route') => {
    setLoginImportOpen(false);
    if (
      isConnectInspect(inspectTarget)
      && inspectTarget.entry.purpose === purpose
      && inspectTarget.entry.source.kind === ticket.sourceKind
      && inspectTarget.entry.source.id === ticket.sourceId
    ) {
      // Repeated activation is idempotent. The share/route action is an
      // opener, so clicking it again must not collapse the already-open pane.
      return;
    }
    inspectOpen({
      kind: 'connect',
      entry: {
        mode: 'for-source',
        source: { kind: ticket.sourceKind, id: ticket.sourceId },
        purpose,
      },
    });
  }, [inspectOpen, inspectTarget, setLoginImportOpen]);

  const handleShareTicket = useCallback((ticket: TicketView) => {
    openConnectForTicket(ticket, 'share');
  }, [openConnectForTicket]);

  const handleRouteTicket = useCallback((ticket: TicketView) => {
    openConnectForTicket(ticket, 'route');
  }, [openConnectForTicket]);

  return {
    handleShareTicket,
    handleRouteTicket,
  };
}
