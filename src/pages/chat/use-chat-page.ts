import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useTicketWallet } from '@/app/runtime';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { AGENT_IDS } from '@/config/agents';
import { listAgents } from '@/lib/api/agent';
import {
  createConversation,
  deleteConversation,
  ensureDefaultConversation,
  listChatMessages,
  listConversations,
  updateConversation,
} from '@/lib/api/chat';
import { switchAccount } from '@/lib/api/account';
import { listProviders, switchProvider } from '@/lib/api/provider';
import { pickDirectory } from '@/lib/api/settings';
import { bindTicket, isActiveBindingForAgent } from '@/lib/api/tickets';
import { takeChatBootstrap } from '@/lib/chat-bootstrap';
import type { TicketWallet } from '@/lib/backend/contracts/ticket';
import type {
  AgentId,
  AgentStatus,
  ChatMessage,
  Conversation,
  Provider,
} from '@/lib/types';
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
  newConversationDefaults,
  selectConversationAgent,
} from './chat-model';
import {
  conversationListState,
  createSingleFlight,
} from './chat-request';
import { useChatPageChrome } from './use-chat-page-chrome';
import { useChatPageSend } from './use-chat-page-send';

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
  const [searchParams, setSearchParams] = useSearchParams();
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [agentStatus, setAgentStatus] = useState<AgentStatus[]>([]);
  /** listAgents 成功后才为 true；失败/未完成时不得把「未知」当成「没有可用 Agent」 */
  const [agentsReady, setAgentsReady] = useState(false);
  const [providers, setProviders] = useState<Provider[]>([]);
  const wallet = ticketWallet.wallet;
  const walletReady = ticketWallet.state === 'ready' || ticketWallet.state === 'error';
  const providersGenRef = useRef(0);
  const [error, setError] = useState<unknown>(null);
  /** 会话列表骨架（不再用整页 spinner 挡住消息区） */
  const [listLoading, setListLoading] = useState(true);
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
  const ensureSingleFlightRef = useRef<ReturnType<typeof createSingleFlight<Conversation[]>> | null>(
    null,
  );
  const loadListSingleFlightRef = useRef<ReturnType<typeof createSingleFlight<Conversation[]>> | null>(
    null,
  );
  const loadGenerationRef = useRef(0);
  /** Projects 页跳转：bootstrap 只处理一次 */
  const bootstrapDoneRef = useRef(false);
  activeIdRef.current = activeId;
  if (generationActiveIdRef.current !== activeId) {
    generationActiveIdRef.current = activeId;
    activeGenerationRef.current += 1;
  }

  const active = useMemo(
    () => conversations.find((c) => c.id === activeId) ?? null,
    [conversations, activeId],
  );

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

  const defaultAgents = useCallback((agents: AgentStatus[]): AgentId[] => {
    const selectable = agents.filter((a) => isChatAgentSelectable(a)).map((a) => a.agentId);
    if (selectable.length > 0) return [selectable[0]];
    return [];
  }, []);

  /** 确保至少有一个会话；空列表时自动新建并返回完整列表。 */
  const ensureConversation = useCallback(
    async (convs: Conversation[], agents: AgentStatus[], cwd?: string | null) => {
      if (convs.length > 0) return convs;
      if (!ensureSingleFlightRef.current) {
        ensureSingleFlightRef.current = createSingleFlight<Conversation[]>();
      }
      return ensureSingleFlightRef.current(async () => {
        const ids = defaultAgents(agents);
        if (ids.length === 0) return convs;
        const created = await ensureDefaultConversation(ids, cwd ?? null);
        return [created];
      });
    },
    [defaultAgents],
  );

  const refreshAgents = useCallback(async (opts: { force?: boolean } = {}): Promise<AgentStatus[]> => {
    try {
      const agents = await listAgents(opts);
      setAgentStatus(agents);
      setAgentsReady(true);
      return agents;
    } catch (e) {
      setAgentsReady(false);
      throw e;
    }
  }, []);

  /**
   * 会话列表优先：不因 listAgents（doctor）阻塞会话渲染。
   * agents 仅在空列表需自动建会话时才 await。
   */
  const loadList = useCallback(() => {
    if (!loadListSingleFlightRef.current) {
      loadListSingleFlightRef.current = createSingleFlight<Conversation[]>();
    }
    return loadListSingleFlightRef.current(async () => {
    const convs = await listConversations();
    let next = convs;
    if (convs.length > 0) {
      // agent 状态异步填充 picker，不挡列表；失败记 ready=false，允许重试
      void refreshAgents().catch(() => {});
    } else {
      const agents = await refreshAgents();
      next = await ensureConversation(convs, agents);
    }
    // 以服务端 sending 为准恢复页级 Stop；list 尚未带上 sending 时不要清掉进行中的本地 send
    const inflight = next.find((c) => c.sending)?.id ?? null;
    send.adoptInflight(inflight);
      return next;
    });
  }, [ensureConversation, refreshAgents]);

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
    const generation = ++loadGenerationRef.current;
    let cancelled = false;
    setListLoading(true);
    setError(null);
    loadList()
      .then(async (convs) => {
        if (cancelled || generation !== loadGenerationRef.current) return;
        // Commit the list and its initial selection together. Without this
        // commit the hook kept an empty in-memory rail even though the API
        // load succeeded, and bootstrap could accidentally discard existing
        // conversations when it prepended its new one.
        const committed = conversationListState(convs);
        setConversations(committed.conversations);
        setActiveId(committed.activeId);
        // Projects → Chat：新建会话并预填（可选自动发送）提示
        const fromProjects = searchParams.get('from') === 'projects';
        if (fromProjects && !bootstrapDoneRef.current) {
          bootstrapDoneRef.current = true;
          const boot = takeChatBootstrap();
          // 清掉 query，避免刷新重复创建
          setSearchParams({}, { replace: true });
          if (boot) {
            try {
              const created = await createConversation(
                boot.agentIds.slice(0, 1),
                boot.cwd ?? null,
              );
              let next = created;
              if (boot.title) {
                try {
                  next = await updateConversation(created.id, { title: boot.title });
                } catch {
                  /* title 可选 */
                }
              }
              if (cancelled || generation !== loadGenerationRef.current) return;
              setConversations((prev) => [next, ...prev.filter((c) => c.id !== next.id)]);
              setActiveId(next.id);
              setMessages([]);
              if (boot.prompt?.trim()) {
                setDraft(boot.prompt);
                toast({
                  title: t('chat.toast.fromProjects'),
                  description: t('chat.toast.fromProjectsDesc'),
                  variant: 'success',
                });
              }
              return;
            } catch (e) {
              if (cancelled || generation !== loadGenerationRef.current) return;
              toast({
                title: e instanceof Error ? e.message : String(e),
                variant: 'danger',
              });
            }
          }
        }
        if (cancelled || generation !== loadGenerationRef.current) return;
      })
      .catch((e) => {
        if (!cancelled && generation === loadGenerationRef.current) setError(e);
      })
      .finally(() => {
        if (!cancelled && generation === loadGenerationRef.current) setListLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- bootstrap once on mount/list load
  }, [loadList]);

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

  async function handleNewChat() {
    let status = agentStatus;
    if (!agentsReady) {
      try {
        status = await refreshAgents();
      } catch (e) {
        toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
        return;
      }
    }
    const defaults = newConversationDefaults(active, status);
    if (defaults.agentIds.length === 0) return;
    try {
      const conv = await createConversation(defaults.agentIds, defaults.cwd);
      setConversations((prev) => [conv, ...prev]);
      setActiveId(conv.id);
      setMessages([]);
      setDraft('');
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    }
  }

  async function handleDelete(id: string) {
    try {
      await send.cancelIfSending(id);
      await deleteConversation(id);
      const rest = conversations.filter((c) => c.id !== id);
      if (rest.length === 0) {
        const defaults = newConversationDefaults(active, agentStatus);
        if (defaults.agentIds.length === 0) {
          setConversations([]);
          setActiveId(null);
          setMessages([]);
          setDraft('');
          return;
        }
        const created = await createConversation(defaults.agentIds, defaults.cwd);
        setConversations([created]);
        setActiveId(created.id);
        setMessages([]);
        setDraft('');
        return;
      }
      setConversations(rest);
      if (activeId === id) {
        setActiveId(rest[0].id);
        setMessages([]);
      }
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    }
  }

  async function confirmDelete() {
    if (!deleteConfirmId) return;
    const id = deleteConfirmId;
    setDeleteConfirmId(null);
    await handleDelete(id);
  }

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

  function retryLoad() {
    const generation = ++loadGenerationRef.current;
    let cancelled = false;
    setListLoading(true);
    setError(null);
    loadList()
      .then((next) => {
        if (cancelled || generation !== loadGenerationRef.current) return;
        const committed = conversationListState(next);
        setConversations(committed.conversations);
        setActiveId(committed.activeId);
      })
      .catch((e) => {
        if (!cancelled && generation === loadGenerationRef.current) setError(e);
      })
      .finally(() => {
        if (!cancelled && generation === loadGenerationRef.current) setListLoading(false);
      });
  }

  function focusConversation(id: string) {
    if (!conversations.some((c) => c.id === id)) return;
    setActiveId(id);
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
