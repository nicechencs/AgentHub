import { useCallback, useRef, useState } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import type { AgentTabId } from '@/components/layout/AgentTabStrip';
import { deleteAccount, switchAccount, undoSwitchAccount } from '@/lib/api/account';
import {
  deleteProvider,
  disconnectPiProvider,
  switchPreview,
  switchProvider,
  undoSwitch,
} from '@/lib/api/provider';
import { logGuiEvent, guiErrorCode } from '@/lib/api/settings';
import type { TranslateFn } from '@/lib/i18n';
import {
  bindTicket,
  isActiveBindingForAgent,
  type TicketView,
  type TicketWallet,
} from '@/lib/api/tickets';
import type { LiveOccupancyDto } from '@/lib/backend/contracts/agent-catalog-types';
import {
  isCatalogAppendOccupancy,
  isListOccupancy,
} from '@/lib/backend/contracts/agent-catalog-types';
import { resolveAgentMeta } from '@/config/agents';
import { removeTicketFromWalletSnapshot } from '@/app/runtime';
import { deleteConnectionToastDescription } from './connection-model';
import { isPiProviderTicket } from './ticket-card-detail';

/** Success toast: switch wrote the login into this Agent's local files (Chinese fallback for callers without a translator). */
export const SWITCH_WROTE_LIVE = '已写入本机配置';

/** Localized success toast for a switch that wrote to local config. */
export function switchWroteLiveLabel(
  t?: TranslateFn,
  occupancy?: LiveOccupancyDto | null,
): string {
  if (isCatalogAppendOccupancy(occupancy)) {
    return t ? t('connections.list.switchWroteCatalog') : '已写入模型列表';
  }
  return t ? t('connections.list.switchWroteLive') : SWITCH_WROTE_LIVE;
}

const FAILED_TO_WRITE_LIVE_FALLBACK = '未能写入本机配置';
const CURSOR_LIVE_WRITE_UNSUPPORTED_FALLBACK =
  '未能写入本机配置。Cursor 暂时不能把这份登录写到本机配置。请用 Cursor 自己的登录。';

function isCurrentListTicket(
  extras: { isCurrent?: boolean; inList?: boolean } | null,
): boolean {
  return Boolean(extras?.isCurrent && (extras.inList ?? extras.isCurrent === true));
}

export type ConnectionActionApi = {
  disconnectPiProvider: (providerId: string, deleteFromLibrary: boolean) => Promise<void>;
  undoSwitchAccount: (agentId: TicketView['agentId']) => Promise<boolean>;
  undoSwitch: (agentId: TicketView['agentId']) => Promise<boolean>;
  deleteAccount: (agentId: TicketView['agentId'], accountId: string) => Promise<void>;
  deleteProvider: (agentId: TicketView['agentId'], providerId: string) => Promise<void>;
};

const defaultConnectionActionApi: ConnectionActionApi = {
  disconnectPiProvider,
  undoSwitchAccount,
  undoSwitch,
  deleteAccount,
  deleteProvider,
};

/** Runs the live-list removal for one concrete record. */
export async function removeCatalogTicket(
  ticket: TicketView,
  extras: { isCurrent?: boolean; inList?: boolean } | null,
  api: ConnectionActionApi = defaultConnectionActionApi,
): Promise<boolean> {
  if (!isListOccupancy(resolveAgentMeta(ticket.agentId).occupancy)) return false;
  if (!isCurrentListTicket(extras)) return false;
  if (isPiProviderTicket(ticket)) {
    await api.disconnectPiProvider(ticket.sourceId, false);
    return true;
  }
  return ticket.sourceKind === 'account'
    ? api.undoSwitchAccount(ticket.agentId)
    : api.undoSwitch(ticket.agentId);
}

/** Deletes one connection record, removing Pi's live slot only for its current provider. */
export async function deleteConnectionTicket(
  ticket: TicketView,
  extras: { isCurrent?: boolean } | null,
  api: ConnectionActionApi = defaultConnectionActionApi,
): Promise<void> {
  if (isPiProviderTicket(ticket) && extras?.isCurrent) {
    await api.disconnectPiProvider(ticket.sourceId, true);
  } else if (ticket.sourceKind === 'account') {
    await api.deleteAccount(ticket.agentId, ticket.sourceId);
  } else {
    await api.deleteProvider(ticket.agentId, ticket.sourceId);
  }
}

export function switchErrorText(error: unknown): string {
  if (typeof error === 'string' && error.trim()) return error.trim();
  if (error instanceof Error && error.message.trim()) return error.message.trim();
  if (error && typeof error === 'object' && 'message' in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === 'string' && message.trim()) return message.trim();
  }
  return '';
}

/** Matches both the legacy Chinese wording and the English core error text for robustness. */
function isUnsupportedProviderSwitch(text: string): boolean {
  return /provider\.switch\.rollback|\bunsupported\b|\[unsupported\]/i.test(text)
    || text.includes('暂时不能把这份登录写到本机配置')
    || text.includes("can't write this login to")
    || text.includes('live config writes are not supported for cursor');
}

export function describeProviderSwitchError(
  agentId: string,
  error: unknown,
  t?: TranslateFn,
): string {
  const text = switchErrorText(error).replace(/\s+\[[^\]]+\]\s*$/, '').trim();
  if (agentId === 'cursor' && isUnsupportedProviderSwitch(text || String(error))) {
    return t
      ? t('connections.list.cursorLiveWriteUnsupportedFull')
      : CURSOR_LIVE_WRITE_UNSUPPORTED_FALLBACK;
  }
  return text || (t ? t('connections.list.failedToWriteLive') : FAILED_TO_WRITE_LIVE_FALLBACK);
}

/** Gives Pi's live-config conflict a safe, actionable explanation without exposing core details. */
export function describePiProviderActionError(error: unknown, t?: TranslateFn): string {
  const code = guiErrorCode(error)?.toLowerCase();
  if (code === 'provider.pi.live_conflict' || code === 'provider.conflict') {
    return t
      ? t('connections.list.disconnectPiProviderConflict')
      : 'Pi 的本机配置已改变，请刷新连接页面后重试。';
  }
  return t
    ? t('connections.list.disconnectPiProviderFail')
    : '无法取消接入，请重试';
}

/**
 * Connections 页切换当前登录与删除确认。
 * 世代丢弃、same-agent switch / other-agent bind、回收站删除语义未改。
 * 不含导入探测、分享 / 路由侧栏、票夹 bind 预览。
 */
export function useConnectionPageActions(input: {
  filterAgent: AgentTabId;
  wallet: TicketWallet | null;
  extrasForTicket: (ticket: TicketView) => { isCurrent?: boolean; inList?: boolean } | null;
  loadWallet: () => Promise<boolean>;
  poolReload: () => Promise<void>;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const { filterAgent, extrasForTicket, loadWallet, poolReload } = input;
  const [switchingTicketId, setSwitchingTicketId] = useState<string | null>(null);
  const switchGen = useRef(0);
  const [deleteTicket, setDeleteTicket] = useState<TicketView | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const catalogRemovalPendingRef = useRef(false);
  const deletePendingRef = useRef(false);

  const handleRemoveFromCatalog = useCallback(async (ticket: TicketView) => {
    if (catalogRemovalPendingRef.current) return;
    const extras = extrasForTicket(ticket);
    if (!isListOccupancy(resolveAgentMeta(ticket.agentId).occupancy)) return;
    if (!isCurrentListTicket(extras)) return;
    catalogRemovalPendingRef.current = true;
    const generation = ++switchGen.current;
    const isPiProvider = isPiProviderTicket(ticket);
    setSwitchingTicketId(ticket.id);
    try {
      const undone = await removeCatalogTicket(ticket, extras);
      if (switchGen.current !== generation) return;
      if (!undone) {
        toast({
          title: t('connections.list.removeFromCatalogFail'),
          variant: 'danger',
        });
        return;
      }
      void logGuiEvent('switch', { agent: ticket.agentId });
      toast({
        title: isPiProvider
          ? t('connections.list.disconnectPiProviderOk')
          : t('connections.list.removeFromCatalogOk'),
        variant: 'success',
      });
      await poolReload().catch(() => {});
      await loadWallet();
    } catch (e) {
      if (switchGen.current !== generation) return;
      void logGuiEvent('switch_fail', {
        agent: ticket.agentId,
        code: guiErrorCode(e),
      });
      toast({
        title: isPiProvider
          ? t('connections.list.disconnectPiProviderFail')
          : t('connections.list.removeFromCatalogFail'),
        description: isPiProvider
          ? describePiProviderActionError(e, t)
          : describeProviderSwitchError(ticket.agentId, e, t),
        variant: 'danger',
      });
    } finally {
      if (switchGen.current === generation) setSwitchingTicketId(null);
      catalogRemovalPendingRef.current = false;
    }
  }, [extrasForTicket, loadWallet, poolReload, t, toast]);

  const handleSwitchTicket = useCallback(async (ticket: TicketView) => {
    const targetAgent = filterAgent === 'all' ? ticket.agentId : filterAgent;
    const extras = extrasForTicket(ticket);
    // Skip only when this login is already the default. In-list siblings
    // stay listed; 切换 changes which one is default.
    if (extras?.isCurrent) return;
    const generation = ++switchGen.current;
    setSwitchingTicketId(ticket.id);
    const wroteLocal = ticket.agentId === targetAgent;
    try {
      if (wroteLocal) {
        if (ticket.sourceKind === 'account') {
          await switchAccount(ticket.agentId, ticket.sourceId);
        } else {
          await switchPreview(ticket.agentId, ticket.sourceId);
          await switchProvider(ticket.agentId, ticket.sourceId);
        }
      } else {
        const { binding } = await bindTicket(ticket.id, targetAgent);
        if (!isActiveBindingForAgent(binding, targetAgent)) {
          throw new Error(t('connections.list.switchFail'));
        }
      }
      if (switchGen.current !== generation) return;
      void logGuiEvent(wroteLocal ? 'switch' : 'bind', { agent: targetAgent });
      toast({
        title: wroteLocal
          ? extras?.inList
            ? t('connections.list.switchDefaultOk')
            : switchWroteLiveLabel(t, resolveAgentMeta(ticket.agentId).occupancy)
          : t('connections.list.switchOk'),
        variant: 'success',
      });
      await poolReload().catch(() => {});
      await loadWallet();
    } catch (e) {
      if (switchGen.current !== generation) return;
      void logGuiEvent(wroteLocal ? 'switch_fail' : 'bind_fail', {
        agent: targetAgent,
        code: guiErrorCode(e),
      });
      toast({
        title: t('connections.list.switchFail'),
        description: describeProviderSwitchError(targetAgent, e, t),
        variant: 'danger',
      });
    } finally {
      if (switchGen.current === generation) setSwitchingTicketId(null);
    }
  }, [extrasForTicket, filterAgent, loadWallet, poolReload, t, toast]);

  const confirmDeleteTicket = async () => {
    if (!deleteTicket) return;
    if (deletePendingRef.current) return;
    deletePendingRef.current = true;
    const extras = extrasForTicket(deleteTicket);
    const isCurrentPiProvider = isPiProviderTicket(deleteTicket) && extras?.isCurrent === true;
    setDeleteBusy(true);
    try {
      await deleteConnectionTicket(deleteTicket, extras);
      void logGuiEvent('delete_connection', { agent: deleteTicket.agentId });
      removeTicketFromWalletSnapshot(deleteTicket.id);
      setDeleteTicket(null);
      toast({
        title: t('connections.delete.toastOk'),
        description: isCurrentPiProvider
          ? t('connections.delete.toastPiCurrent')
          : deleteConnectionToastDescription({ isCurrent: extras?.isCurrent === true }, t),
        variant: 'success',
      });
      await loadWallet();
      await poolReload().catch(() => {});
    } catch (e) {
      void logGuiEvent('delete_connection_fail', {
        agent: deleteTicket.agentId,
        code: guiErrorCode(e),
      });
      toast({
        title: t('connections.delete.toastFail'),
        description: isCurrentPiProvider
          ? describePiProviderActionError(e, t)
          : e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setDeleteBusy(false);
      deletePendingRef.current = false;
    }
  };

  return {
    switchingTicketId,
    handleSwitchTicket,
    handleRemoveFromCatalog,
    deleteTicket,
    setDeleteTicket,
    deleteBusy,
    confirmDeleteTicket,
  };
}
