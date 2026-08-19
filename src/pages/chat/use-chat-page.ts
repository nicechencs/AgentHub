import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { AGENT_IDS } from '@/config/agents';
import { listAgents } from '@/lib/api/agent';
import {
  chatCancel,
  chatSend,
  createConversation,
  deleteConversation,
  listChatMessages,
  listConversations,
  updateConversation,
} from '@/lib/api/chat';
import { listProviders, switchProvider } from '@/lib/api/provider';
import { pickDirectory } from '@/lib/api/settings';
import { takeChatBootstrap } from '@/lib/chat-bootstrap';
import { processKey, reduceProcessEvent, type ProcessMap } from '@/lib/chat-process';
import type {
  AgentId,
  AgentStatus,
  ChatEvent,
  ChatMessage,
  Conversation,
  Provider,
} from '@/lib/types';
import { extractModel, groupByTurn } from './chat-format';
import {
  agentHasConfiguredAuth,
  agentPickerLabel as agentPickerLabelOf,
  chatAgentPickerRows,
  chatConnectionPickerView,
  connectionPickerCaption,
  conversationTitle,
  filterConversations,
  groupConversationsByDay,
  isChatAgentSelectable,
  newConversationDefaults,
  selectConversationAgent,
  retryTarget,
  sendBlockers,
} from './chat-model';

const STICK_THRESHOLD_PX = 80;

export function useChatPage() {
  const { t } = useI18n();
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [agentStatus, setAgentStatus] = useState<AgentStatus[]>([]);
  /** listAgents 成功后才为 true；失败/未完成时不得把「未知」当成「没有可用 Agent」 */
  const [agentsReady, setAgentsReady] = useState(false);
  const [providers, setProviders] = useState<Provider[]>([]);
  const providersGenRef = useRef(0);
  const [error, setError] = useState<unknown>(null);
  /** 会话列表骨架（不再用整页 spinner 挡住消息区） */
  const [listLoading, setListLoading] = useState(true);
  /** 当前会话消息 / provider 加载 */
  const [messagesLoading, setMessagesLoading] = useState(false);
  const [sending, setSending] = useState(false);
  const [sendingConversationId, setSendingConversationId] = useState<string | null>(null);
  const sendingConversationIdRef = useRef<string | null>(null);
  const [draft, setDraft] = useState('');
  const [railOpen, setRailOpen] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [dangerConfirm, setDangerConfirm] = useState(false);
  const [switchingProvider, setSwitchingProvider] = useState(false);
  const [railQuery, setRailQuery] = useState('');
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
  const transcriptRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const stickToBottomRef = useRef(true);
  const streamingRef = useRef<Record<string, string>>({});
  const activeIdRef = useRef<string | null>(null);
  /** 当轮过程面板：命令 / stderr / 细状态（仅内存，不落库） */
  const [processMap, setProcessMap] = useState<ProcessMap>({});
  /** Projects 页跳转：bootstrap 只处理一次 */
  const bootstrapDoneRef = useRef(false);
  activeIdRef.current = activeId;

  const active = useMemo(
    () => conversations.find((c) => c.id === activeId) ?? null,
    [conversations, activeId],
  );

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
      const ids = defaultAgents(agents);
      if (ids.length === 0) return convs;
      const created = await createConversation(ids, cwd ?? null);
      return [created];
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
  const loadList = useCallback(async () => {
    const convs = await listConversations();
    if (convs.length > 0) {
      setConversations(convs);
      // agent 状态异步填充 picker，不挡列表；失败记 ready=false，允许重试
      void refreshAgents().catch(() => {});
      return convs;
    }
    const agents = await refreshAgents();
    const ensured = await ensureConversation(convs, agents);
    setConversations(ensured);
    return ensured;
  }, [ensureConversation, refreshAgents]);

  const loadMessages = useCallback(async (id: string) => {
    return listChatMessages(id);
  }, []);

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
    setListLoading(true);
    setError(null);
    loadList()
      .then(async (convs) => {
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
              toast({
                title: e instanceof Error ? e.message : String(e),
                variant: 'danger',
              });
            }
          }
        }
        setActiveId(convs[0]?.id ?? null);
      })
      .catch(setError)
      .finally(() => setListLoading(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps -- bootstrap once on mount/list load
  }, [loadList]);

  // messages 与 providers 独立并发（不再串在 loadList 之后的瀑布里）
  useEffect(() => {
    // 过程面板仅内存；切会话时清空，避免串台
    setProcessMap({});
    streamingRef.current = {};
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

  const turns = useMemo(() => groupByTurn(messages), [messages]);

  const liveSendingConversationId = useMemo(() => {
    if (!sendingConversationId) return null;
    return conversations.some((c) => c.id === sendingConversationId)
      ? sendingConversationId
      : null;
  }, [conversations, sendingConversationId]);

  const sendingTitle = useMemo(() => {
    if (!liveSendingConversationId) return '';
    const row = conversations.find((c) => c.id === liveSendingConversationId);
    return conversationTitle(t, row?.title ?? '');
  }, [conversations, liveSendingConversationId, t]);

  const blockers = useMemo(() => {
    if (!active) return [];
    return sendBlockers({
      conversation: active,
      hiddenIds,
      unconfiguredAuthIds,
      sendingConversationId: liveSendingConversationId,
      sendingTitle,
    });
  }, [active, hiddenIds, unconfiguredAuthIds, liveSendingConversationId, sendingTitle]);

  const railGroups = useMemo(() => {
    const filtered = filterConversations(conversations, railQuery);
    return groupConversationsByDay(filtered, Date.now(), t);
  }, [conversations, railQuery, t]);

  const filteredCount = useMemo(
    () => filterConversations(conversations, railQuery).length,
    [conversations, railQuery],
  );

  const retry = useMemo(() => retryTarget(turns, sending), [turns, sending]);

  const agentPickerLabel = useMemo(() => agentPickerLabelOf(t, active), [active, t]);

  const primaryStatus = useMemo(
    () => (primaryAgent ? agentStatus.find((a) => a.agentId === primaryAgent) : undefined),
    [agentStatus, primaryAgent],
  );

  const connectionView = useMemo(
    () =>
      chatConnectionPickerView(t, {
        primaryAgent,
        switching: switchingProvider,
        status: primaryStatus,
        currentProviderName: currentProvider?.name ?? null,
        currentProviderModel: currentProvider ? extractModel(currentProvider.configText) : null,
      }),
    [primaryAgent, switchingProvider, primaryStatus, currentProvider, t],
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
      if (sendingConversationId === id) {
        await chatCancel(id).catch(() => {});
        sendingConversationIdRef.current = null;
        setSending(false);
        setSendingConversationId(null);
      }
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

  async function handleSwitchProvider(providerId: string) {
    if (!primaryAgent || switchingProvider || hiddenIds.has(primaryAgent)) return;
    setSwitchingProvider(true);
    try {
      await switchProvider(primaryAgent, providerId);
      await Promise.all([
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

  function applyEvent(ev: ChatEvent, sendConvId: string) {
    if (activeIdRef.current !== sendConvId) {
      if (ev.type === 'error') toast({ title: ev.message, variant: 'danger' });
      return;
    }
    // 过程面板状态（命令 / stderr / 细状态）与 messages 并行维护
    setProcessMap((prev) => reduceProcessEvent(prev, ev));

    if (ev.type === 'started') {
      streamingRef.current = {};
      for (const agent of ev.agents) {
        streamingRef.current[processKey(ev.turn, agent)] = '';
        setMessages((prev) => {
          const hasRunning = prev.some(
            (m) => m.turn === ev.turn && m.role === 'agent' && m.agentId === agent,
          );
          if (hasRunning) return prev;
          return [
            ...prev,
            {
              id: `local-${ev.turn}-${agent}`,
              conversationId: sendConvId,
              turn: ev.turn,
              role: 'agent',
              agentId: agent,
              content: '',
              status: 'running',
              durationMs: 0,
              createdAt: new Date().toISOString(),
            },
          ];
        });
      }
      return;
    }
    if (ev.type === 'agentChunk' && ev.stream === 'stdout') {
      const key = processKey(ev.turn, ev.agent);
      streamingRef.current[key] = (streamingRef.current[key] ?? '') + ev.text;
      const content = streamingRef.current[key];
      setMessages((prev) =>
        prev.map((m) =>
          m.role === 'agent' &&
          m.agentId === ev.agent &&
          m.turn === ev.turn &&
          m.status === 'running'
            ? { ...m, content }
            : m,
        ),
      );
      return;
    }
    if (ev.type === 'agentFinished') {
      setMessages((prev) => {
        const withoutLocal = prev.filter(
          (m) =>
            !(
              m.role === 'agent' &&
              m.agentId === ev.agent &&
              m.turn === ev.turn &&
              (m.status === 'running' || m.id.startsWith('local-'))
            ),
        );
        return [...withoutLocal, ev.message];
      });
      return;
    }
    if (ev.type === 'error') {
      toast({ title: ev.message, variant: 'danger' });
    }
  }

  async function sendPrompt(prompt: string, clearDraft: boolean) {
    if (!active || sending) return;
    if (sendBlockers({
      conversation: active,
      hiddenIds,
      unconfiguredAuthIds,
      sendingConversationId: liveSendingConversationId,
      sendingTitle,
    }).length > 0) {
      return;
    }
    if (!prompt) return;

    const sendConvId = active.id;
    sendingConversationIdRef.current = sendConvId;
    setSending(true);
    setSendingConversationId(sendConvId);
    if (clearDraft) setDraft('');
    const turnGuess = messages.reduce((max, m) => Math.max(max, m.turn), 0) + 1;
    setMessages((prev) => [
      ...prev,
      {
        id: `local-user-${Date.now()}`,
        conversationId: sendConvId,
        turn: turnGuess,
        role: 'user',
        content: prompt,
        status: 'ok',
        durationMs: 0,
        createdAt: new Date().toISOString(),
      },
    ]);

    try {
      await chatSend(sendConvId, prompt, (ev) => applyEvent(ev, sendConvId));
      const convs = await listConversations();
      setConversations(convs);
      if (activeIdRef.current === sendConvId) {
        setMessages(await listChatMessages(sendConvId));
      }
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
      if (activeIdRef.current === sendConvId) {
        const rows = await loadMessages(sendConvId).catch(() => null);
        if (rows && activeIdRef.current === sendConvId) setMessages(rows);
      }
    } finally {
      if (sendingConversationIdRef.current === sendConvId) {
        sendingConversationIdRef.current = null;
        setSending(false);
        setSendingConversationId(null);
      }
    }
  }

  async function handleSend() {
    await sendPrompt(draft.trim(), true);
  }

  async function retryLast() {
    const target = retryTarget(turns, sending);
    if (!target) return;
    await sendPrompt(target.prompt, false);
  }

  async function handleCancel() {
    if (!sendingConversationId) return;
    try {
      await chatCancel(sendingConversationId);
      toast({
        title: t('chat.toast.cancelRequested'),
        description: t('chat.toast.cancelRequestedDesc'),
        variant: 'success',
        duration: 4000,
      });
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    }
  }

  function retryLoad() {
    setListLoading(true);
    setError(null);
    loadList()
      .then((c) => c[0] && setActiveId(c[0].id))
      .catch(setError)
      .finally(() => setListLoading(false));
  }

  function focusConversation(id: string) {
    if (!conversations.some((c) => c.id === id)) return;
    setActiveId(id);
  }

  const sendingHere = Boolean(sending && sendingConversationId === active?.id);

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
    sending,
    sendingHere,
    sendingConversationId: liveSendingConversationId,
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
    processMap,
    hiddenIds,
    unconfiguredAuthIds,
    pickerRows,
    installed,
    primaryAgent,
    hasUsableAgent,
    activeHasHidden,
    agentPickerLabel,
    connectionView,
    connectionCaption,
    blockers,
    railGroups,
    filteredCount,
    retry,
    transcriptRef,
    bottomRef,
    onTranscriptScroll,
    handleNewChat,
    confirmDelete,
    patchActive,
    pickWorkingDirectory,
    renameTitle,
    selectConversationAgentId,
    handleSwitchProvider,
    handleSend,
    retryLast,
    handleCancel,
    cancelSending: handleCancel,
    retryLoad,
    focusConversation,
  };
}
