import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTicketWallet } from '@/app/runtime';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { AGENT_IDS } from '@/config/agents';
import { listChatMessages, updateConversation } from '@/lib/api/chat';
import { switchAccount } from '@/lib/api/account';
import { listProviders, switchProvider } from '@/lib/api/provider';
import { pickDirectory } from '@/lib/api/settings';
import { bindTicket, isActiveBindingForAgent } from '@/lib/api/tickets';
import type { TicketWallet } from '@/lib/backend/contracts/ticket';
import type { AgentId, ChatMessage, Provider } from '@/lib/types';
import { extractModel, groupByTurn } from './chat-format';
import {
  agentChatEnvReady,
  agentHasConfiguredAuth,
  agentPickerLabel as agentPickerLabelOf,
  chatAgentPickerRows,
  chatConnectionOptions,
  chatConnectionPickerView,
  connectionPickerCaption,
  filterConversations,
  groupConversationsByDay,
  isChatAgentSelectable,
  leftoverProviderIsCurrent,
  selectConversationAgent,
} from './chat-model';
import { useChatPageChrome } from './use-chat-page-chrome';
import { useChatPageSend } from './use-chat-page-send';
import { useChatPageSessions } from './use-chat-page-sessions';

export {
  conversationListState,
  createSingleFlight,
  isCurrentChatRequest,
} from './chat-request';

const STICK_THRESHOLD_PX = 80;

const EMPTY_WALLET: TicketWallet = { tickets: [], bindings: [], surfaceGroups: [] };

export function useChatPage() {
  const { t } = useI18n();
  const { toast } = useToast();
  const ticketWallet = useTicketWallet();
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [providers, setProviders] = useState<Provider[]>([]);
  const wallet = ticketWallet.wallet;
  const walletReady = ticketWallet.state === 'ready' || ticketWallet.state === 'error';
  const providersGenRef = useRef(0);
  /** 当前会话消息 / provider 加载 */
  const [messagesLoading, setMessagesLoading] = useState(false);
  const [draft, setDraft] = useState('');
  const {
    railOpen,
    setRailOpen,
    settingsOpen,
    setSettingsOpen,
    dangerConfirm,
    setDangerConfirm,
    deleteConfirmId,
    setDeleteConfirmId,
    railQuery,
    setRailQuery,
  } = useChatPageChrome();
  const [switchingProvider, setSwitchingProvider] = useState(false);
  const transcriptRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const stickToBottomRef = useRef(true);
  const activeIdRef = useRef<string | null>(null);
  const activeGenerationRef = useRef(0);
  const generationActiveIdRef = useRef<string | null>(null);
  const sendRef = useRef<{
    adoptInflight: (id: string | null) => void;
    cancelIfSending: (id: string) => Promise<void>;
  }>({
    adoptInflight: () => {},
    cancelIfSending: async () => {},
  });

  const sessions = useChatPageSessions({
    setMessages,
    setDraft,
    deleteConfirmId,
    setDeleteConfirmId,
    sendRef,
  });
  const {
    conversations,
    setConversations,
    activeId,
    active,
    agentStatus,
    agentsReady,
    error,
    setError,
    listLoading,
    refreshAgents,
    handleNewChat,
    confirmDelete,
    retryLoad,
    focusConversation,
  } = sessions;

  activeIdRef.current = activeId;
  if (generationActiveIdRef.current !== activeId) {
    generationActiveIdRef.current = activeId;
    activeGenerationRef.current += 1;
  }

  const installed = useMemo(() => {
    const m = new Map<AgentId, boolean>();
    for (const a of agentStatus) m.set(a.agentId, a.installed);
    return m;
  }, [agentStatus]);

  const hiddenIds = useMemo(
    () => new Set(agentStatus.filter((a) => a.hidden).map((a) => a.agentId)),
    [agentStatus],
  );
  const unconfiguredAuthIds = useMemo(
    () =>
      new Set(
        agentStatus.filter((a) => a.installed && !agentHasConfiguredAuth(a)).map((a) => a.agentId),
      ),
    [agentStatus],
  );
  const envNotReadyIds = useMemo(
    () =>
      new Set(
        agentStatus.filter((a) => a.installed && !agentChatEnvReady(a)).map((a) => a.agentId),
      ),
    [agentStatus],
  );

  const loadMessages = useCallback(async (id: string) => {
    return listChatMessages(id);
  }, []);

  const turns = useMemo(() => groupByTurn(messages), [messages]);

  const send = useChatPageSend({
    active,
    activeId,
    messages,
    setMessages,
    setConversations,
    conversations,
    hiddenIds,
    envNotReadyIds,
    unconfiguredAuthIds,
    activeIdRef,
    activeGenerationRef,
    loadMessages,
    draft,
    setDraft,
    turns,
  });
  sendRef.current = {
    adoptInflight: send.adoptInflight,
    cancelIfSending: send.cancelIfSending,
  };
  const sending = send.sending;

  useEffect(() => {
    if (!active || sending) return;
    if (active.agentIds.length <= 1) return;
    const next = selectConversationAgent({
      currentIds: [],
      nextId: active.agentIds[0],
      allowDangerous: active.allowDangerous,
    });
    if (!next) return;
    void updateConversation(active.id, next)
      .then((updated) => {
        setConversations((prev) => prev.map((c) => (c.id === updated.id ? updated : c)));
      })
      .catch(() => {});
  }, [active, sending]);

  const pickerRows = useMemo(
    () =>
      chatAgentPickerRows({
        catalogIds: AGENT_IDS,
        agentStatus,
      }),
    [agentStatus],
  );
  const activeHasHidden = Boolean(active?.agentIds.some((id) => hiddenIds.has(id)));

  const primaryAgent = active?.agentIds[0] ?? null;

  const currentProvider = useMemo(
    () => providers.find((p) => p.isCurrent) ?? null,
    [providers],
  );

  const hasUsableAgent = agentsReady && agentStatus.some((a) => isChatAgentSelectable(a));

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

  // messages 与 providers 独立并发（不再串在 loadList 之后的瀑布里）
  useEffect(() => {
    stickToBottomRef.current = true;
    if (!activeId) {
      setMessages([]);
      return;
    }
    let cancelled = false;
    setMessagesLoading(true);
    loadMessages(activeId)
      .then((rows) => {
        if (!cancelled && activeIdRef.current === activeId) {
          setMessages(rows);
        }
      })
      .catch((e) => {
        if (!cancelled) setError(e);
      })
      .finally(() => {
        if (!cancelled) setMessagesLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [activeId, loadMessages]);

  useEffect(() => {
    if (!primaryAgent) {
      providersGenRef.current += 1;
      setProviders([]);
      return;
    }
    setProviders([]);
    void loadProviders(primaryAgent);
  }, [primaryAgent, loadProviders]);

  const onTranscriptScroll = useCallback(() => {
    const el = transcriptRef.current;
    if (!el) return;
    const dist = el.scrollHeight - el.scrollTop - el.clientHeight;
    stickToBottomRef.current = dist <= STICK_THRESHOLD_PX;
  }, []);

  useEffect(() => {
    if (!stickToBottomRef.current) return;
    bottomRef.current?.scrollIntoView({ block: 'nearest' });
  }, [messages, sending]);

  const railGroups = useMemo(() => {
    const filtered = filterConversations(conversations, railQuery);
    return groupConversationsByDay(filtered, Date.now(), t);
  }, [conversations, railQuery, t]);

  const filteredCount = useMemo(
    () => filterConversations(conversations, railQuery).length,
    [conversations, railQuery],
  );

  const agentPickerLabel = useMemo(() => agentPickerLabelOf(t, active), [active, t]);

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

  async function patchActive(patch: Parameters<typeof updateConversation>[1]) {
    if (!active) return;
    try {
      const updated = await updateConversation(active.id, patch);
      setConversations((prev) => prev.map((c) => (c.id === updated.id ? updated : c)));
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    }
  }

  async function pickWorkingDirectory() {
    if (!active) return;
    try {
      const picked = await pickDirectory({
        title: t('chat.settings.pickDirTitle'),
        defaultPath: active.cwd ?? null,
      });
      if (picked) {
        await patchActive({ cwd: picked });
      }
    } catch (e) {
      toast({
        title: t('chat.settings.pickDirFailed'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    }
  }

  async function renameTitle(next: string) {
    if (!active) return false;
    const title = next.trim();
    try {
      const updated = await updateConversation(active.id, { title });
      setConversations((prev) => prev.map((c) => (c.id === updated.id ? updated : c)));
      return true;
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
      return false;
    }
  }

  async function selectConversationAgentId(id: AgentId) {
    if (!active || sending) return;
    const row = pickerRows.find((r) => r.id === id);
    if (!row?.selectable) return;
    const next = selectConversationAgent({
      currentIds: active.agentIds,
      nextId: id,
      allowDangerous: active.allowDangerous,
    });
    if (!next) return;
    await patchActive(next);
  }

  async function handleSwitchConnection(ticketId: string) {
    if (!primaryAgent || switchingProvider || hiddenIds.has(primaryAgent)) return;
    const option = connectionOptions.find((row) => row.ticketId === ticketId);
    if (!option || option.isCurrent) return;
    setSwitchingProvider(true);
    try {
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
      toast({ title: t('chat.connection.switched'), variant: 'success' });
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    } finally {
      setSwitchingProvider(false);
    }
  }

  return {
    conversations,
    activeId,
    active,
    messages,
    turns,
    agentStatus,
    agentsReady,
    providers,
    error,
    listLoading,
    messagesLoading,
    sending: send.sending,
    sendingHere: send.sendingHere,
    sendingConversationId: send.sendingConversationId,
    draft,
    setDraft,
    railOpen,
    setRailOpen,
    settingsOpen,
    setSettingsOpen,
    dangerConfirm,
    setDangerConfirm,
    switchingProvider,
    railQuery,
    setRailQuery,
    deleteConfirmId,
    setDeleteConfirmId,
    processMap: send.processMap,
    hiddenIds,
    unconfiguredAuthIds,
    pickerRows,
    installed,
    primaryAgent,
    hasUsableAgent,
    activeHasHidden,
    agentPickerLabel,
    connectionView,
    connectionOptions,
    connectionCaption,
    walletError: ticketWallet.error,
    reloadWallet: ticketWallet.reload,
    blockers: send.blockers,
    railGroups,
    filteredCount,
    retry: send.retry,
    transcriptRef,
    bottomRef,
    onTranscriptScroll,
    handleNewChat,
    confirmDelete,
    patchActive,
    pickWorkingDirectory,
    renameTitle,
    selectConversationAgentId,
    handleSwitchConnection,
    handleSend: send.handleSend,
    retryLast: send.retryLast,
    handleCancel: send.handleCancel,
    cancelSending: send.handleCancel,
    retryLoad,
    focusConversation,
  };
}
