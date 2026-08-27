import { useCallback, useRef, useState } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import type { AgentTabId } from '@/components/layout/AgentTabStrip';
import { deleteAccount, switchAccount } from '@/lib/api/account';
import { deleteProvider, switchPreview, switchProvider } from '@/lib/api/provider';
import {
  bindTicket,
  isActiveBindingForAgent,
  type TicketView,
  type TicketWallet,
} from '@/lib/api/tickets';
import { deleteConnectionToastDescription } from './connection-model';
import { activeBindingForAgent } from './ticket-wallet-model';

/** Success toast: switch wrote the login into this Agent's local files. */
export const SWITCH_WROTE_LIVE = '已写入本机配置';

const CURSOR_LIVE_WRITE_UNSUPPORTED =
  'Cursor 暂时不能把这份登录写到本机配置。请用 Cursor 自己的登录，或设置 CURSOR_API_KEY。';

export function switchErrorText(error: unknown): string {
  if (typeof error === 'string' && error.trim()) return error.trim();
  if (error instanceof Error && error.message.trim()) return error.message.trim();
  if (error && typeof error === 'object' && 'message' in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === 'string' && message.trim()) return message.trim();
  }
  return '';
}

function isUnsupportedProviderSwitch(text: string): boolean {
  return /provider\.switch\.rollback|\bunsupported\b|\[unsupported\]/i.test(text)
    || text.includes('暂时不能把这份登录写到本机配置')
    || text.includes('live config writes are not supported for cursor');
}

export function describeProviderSwitchError(agentId: string, error: unknown): string {
  const text = switchErrorText(error).replace(/\s+\[[^\]]+\]\s*$/, '').trim();
  if (agentId === 'cursor' && isUnsupportedProviderSwitch(text || String(error))) {
    return `未能写入本机配置。${CURSOR_LIVE_WRITE_UNSUPPORTED}`;
  }
  return text || '未能写入本机配置';
}

/**
 * Connections 页切换当前登录与删除确认。
 * 世代丢弃、same-agent switch / other-agent bind、回收站删除语义未改。
 * 不含导入探测、分享 / 路由侧栏、票夹 bind 预览。
 */
export function useConnectionPageActions(input: {
  filterAgent: AgentTabId;
  wallet: TicketWallet | null;
  extrasForTicket: (ticket: TicketView) => { isCurrent?: boolean } | null;
  loadWallet: () => Promise<boolean>;
  poolReload: () => Promise<void>;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const { filterAgent, wallet, extrasForTicket, loadWallet, poolReload } = input;
  const [switchingTicketId, setSwitchingTicketId] = useState<string | null>(null);
  const switchGen = useRef(0);
  const [deleteTicket, setDeleteTicket] = useState<TicketView | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);

  const handleSwitchTicket = useCallback(async (ticket: TicketView) => {
    const targetAgent = filterAgent === 'all' ? ticket.agentId : filterAgent;
    const tabCurrentId = wallet
      ? activeBindingForAgent(wallet, targetAgent)?.ticket.id ?? null
      : null;
    if (tabCurrentId === ticket.id) return;
    const generation = ++switchGen.current;
    setSwitchingTicketId(ticket.id);
    try {
      if (ticket.agentId === targetAgent) {
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
      toast({ title: SWITCH_WROTE_LIVE, variant: 'success' });
      await poolReload().catch(() => {});
      await loadWallet();
    } catch (e) {
      if (switchGen.current !== generation) return;
      toast({
        title: t('connections.list.switchFail'),
        description: describeProviderSwitchError(targetAgent, e),
        variant: 'danger',
      });
    } finally {
      if (switchGen.current === generation) setSwitchingTicketId(null);
    }
  }, [filterAgent, loadWallet, poolReload, t, toast, wallet]);

  const confirmDeleteTicket = async () => {
    if (!deleteTicket) return;
    const extras = extrasForTicket(deleteTicket);
    setDeleteBusy(true);
    try {
      if (deleteTicket.sourceKind === 'account') {
        await deleteAccount(deleteTicket.agentId, deleteTicket.sourceId);
      } else {
        await deleteProvider(deleteTicket.agentId, deleteTicket.sourceId);
      }
      setDeleteTicket(null);
      toast({
        title: t('connections.delete.toastOk'),
        description: deleteConnectionToastDescription({ isCurrent: extras?.isCurrent === true }, t),
        variant: 'success',
      });
      await loadWallet();
      await poolReload().catch(() => {});
    } catch (e) {
      toast({
        title: t('connections.delete.toastFail'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setDeleteBusy(false);
    }
  };

  return {
    switchingTicketId,
    handleSwitchTicket,
    deleteTicket,
    setDeleteTicket,
    deleteBusy,
    confirmDeleteTicket,
  };
}
