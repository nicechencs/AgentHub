import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTicketWallet } from '@/app/runtime';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { switchAccount } from '@/lib/api/account';
import { listProviders, switchProvider } from '@/lib/api/provider';
import { bindTicket, isActiveBindingForAgent } from '@/lib/api/tickets';
import {
  describeProviderSwitchError,
  SWITCH_WROTE_LIVE,
} from '@/pages/connections/use-connection-page-actions';
import type { TicketWallet } from '@/lib/backend/contracts/ticket';
import type { AgentId, AgentStatus, Conversation, Provider } from '@/lib/types';
import { extractModel } from './chat-format';
import {
  chatConnectionOptions,
  chatConnectionPickerView,
  connectionPickerCaption,
  leftoverProviderIsCurrent,
} from './chat-model';

/**
 * Chat 连接切换与票夹订阅。
 * switch-account → switch-provider → bind 顺序不变；refreshAgents 由会话 Hook 传入。
 * 不含发送、取消、切会话、世代。
 */
const EMPTY_WALLET: TicketWallet = { tickets: [], bindings: [], surfaceGroups: [] };

export function useChatPageConnection(input: {
  primaryAgent: AgentId | null;
  active: Conversation | null;
  hiddenIds: Set<AgentId>;
  agentStatus: AgentStatus[];
  refreshAgents: (opts?: { force?: boolean }) => Promise<AgentStatus[]>;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const { primaryAgent, active, hiddenIds, agentStatus, refreshAgents } = input;
  const ticketWallet = useTicketWallet();
  const [providers, setProviders] = useState<Provider[]>([]);
  const wallet = ticketWallet.wallet;
  const walletReady = ticketWallet.state === 'ready' || ticketWallet.state === 'error';
  const providersGenRef = useRef(0);
  const [switchingProvider, setSwitchingProvider] = useState(false);

  const currentProvider = useMemo(
    () => providers.find((p) => p.isCurrent) ?? null,
    [providers],
  );

  const loadProviders = useCallback(async (agentId: AgentId) => {
    const gen = ++providersGenRef.current;
    try {
      const list = await listProviders(agentId);
      if (gen !== providersGenRef.current) return;
      setProviders(list);
    } catch {
      if (gen !== providersGenRef.current) return;
      setProviders([]);
    }
  }, []);

  useEffect(() => {
    void ticketWallet.ensureLoaded();
  }, [ticketWallet.ensureLoaded]);

  useEffect(() => {
    if (!primaryAgent) {
      providersGenRef.current += 1;
      setProviders([]);
      return;
    }
    setProviders([]);
    void loadProviders(primaryAgent);
  }, [primaryAgent, loadProviders]);

  const primaryStatus = useMemo(
    () => (primaryAgent ? agentStatus.find((a) => a.agentId === primaryAgent) : undefined),
    [agentStatus, primaryAgent],
  );

  const leftoverCurrent = leftoverProviderIsCurrent(providers);

  const connectionOptions = useMemo(
    () =>
      chatConnectionOptions(t, {
        wallet: wallet ?? (walletReady ? EMPTY_WALLET : null),
        agentId: primaryAgent,
      }),
    [wallet, walletReady, primaryAgent, t],
  );

  const activeLogin = connectionOptions.find((option) => option.isCurrent) ?? null;

  const connectionView = useMemo(
    () =>
      chatConnectionPickerView(t, {
        primaryAgent,
        switching: switchingProvider,
        status: primaryStatus,
        currentProviderName: leftoverCurrent ? null : currentProvider?.name ?? null,
        currentProviderModel: leftoverCurrent
          ? null
          : currentProvider
            ? extractModel(currentProvider.configText)
            : null,
        activeLogin: activeLogin
          ? { title: activeLogin.title, subtitle: activeLogin.subtitle }
          : null,
        leftoverCurrent,
        walletReady,
      }),
    [
      primaryAgent,
      switchingProvider,
      primaryStatus,
      leftoverCurrent,
      currentProvider,
      activeLogin,
      walletReady,
      t,
    ],
  );

  const connectionCaption = useMemo(
    () =>
      active
        ? connectionPickerCaption(t, {
            agentIds: active.agentIds,
            primaryAgent,
          })
        : null,
    [active, primaryAgent, t],
  );

  async function handleSwitchConnection(ticketId: string) {
    if (!primaryAgent || switchingProvider || hiddenIds.has(primaryAgent)) return;
    const option = connectionOptions.find((row) => row.ticketId === ticketId);
    if (!option || option.isCurrent) return;
    setSwitchingProvider(true);
    try {
      const wroteLocal =
        option.action.type === 'switch-account' ||
        option.action.type === 'switch-provider';
      if (option.action.type === 'switch-account') {
        await switchAccount(primaryAgent, option.action.accountId);
      } else if (option.action.type === 'switch-provider') {
        await switchProvider(primaryAgent, option.action.providerId);
      } else {
        const { binding } = await bindTicket(option.action.ticketId, primaryAgent);
        if (!isActiveBindingForAgent(binding, primaryAgent)) {
          throw new Error(t('chat.connection.bindNotCurrent'));
        }
      }
      await Promise.all([
        ticketWallet.reload(),
        loadProviders(primaryAgent),
        refreshAgents({ force: true }).catch(() => []),
      ]);
      toast({
        title: wroteLocal ? SWITCH_WROTE_LIVE : t('chat.connection.switched'),
        variant: 'success',
      });
    } catch (e) {
      toast({
        title: t('connections.list.switchFail'),
        description: describeProviderSwitchError(primaryAgent, e),
        variant: 'danger',
      });
    } finally {
      setSwitchingProvider(false);
    }
  }

  return {
    providers,
    switchingProvider,
    connectionView,
    connectionOptions,
    connectionCaption,
    walletError: ticketWallet.error,
    reloadWallet: ticketWallet.reload,
    handleSwitchConnection,
  };
}
