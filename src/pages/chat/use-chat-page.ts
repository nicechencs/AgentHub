import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
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
  agentPickerLabel as agentPickerLabelOf,
  connectionPickerCaption,
  conversationTitle,
  filterConversations,
  groupConversationsByDay,
  newConversationDefaults,
  retryTarget,
  sendBlockers,
} from './chat-model';

const STICK_THRESHOLD_PX = 80;

export function useChatPage() {
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [agentStatus, setAgentStatus] = useState<AgentStatus[]>([]);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [error, setError] = useState<unknown>(null);
  /** 会话列表骨架（不再用整页 spinner 挡住消息区） */
  const [listLoading, setListLoading] = useState(true);
  /** 当前会话消息 / provider 加载 */
  const [messagesLoading, setMessagesLoading] = useState(false);
  const [sending, setSending] = useState(false);
  const [sendingConversationId, setSendingConversationId] = useState<string | null>(null);
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

  const installed = useMemo(() => {
    const m = new Map<AgentId, boolean>();
    for (const a of agentStatus) m.set(a.agentId, a.installed);
    return m;
  }, [agentStatus]);

  const hiddenIds = useMemo(
    () => new Set(agentStatus.filter((a) => a.hidden).map((a) => a.agentId)),
    [agentStatus],
  );
  const activeHasHidden = Boolean(active?.agentIds.some((id) => hiddenIds.has(id)));

  const primaryAgent = active?.agentIds[0] ?? null;

  const currentProvider = useMemo(
    () => providers.find((p) => p.isCurrent) ?? providers[0] ?? null,
    [providers],
  );

  const hasUsableAgent = agentStatus.some((a) => a.installed && !a.hidden);

  const defaultAgents = useCallback((agents: AgentStatus[]): AgentId[] => {
    const installedIds = agents.filter((a) => a.installed && !a.hidden).map((a) => a.agentId);
    if (installedIds.length > 0) return [installedIds[0]];
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

  /**
   * 会话列表优先：不因 listAgents（doctor）阻塞会话渲染。
   * agents 仅在空列表需自动建会话时才 await。
   */
  const loadList = useCallback(async () => {
    const convs = await listConversations();
    if (convs.length > 0) {
      setConversations(convs);
      // agent 状态异步填充 picker，不挡列表
      void listAgents()
        .then(setAgentStatus)
        .catch(() => {});
      return convs;
    }
    const agents = await listAgents();
    setAgentStatus(agents);
    const ensured = await ensureConversation(convs, agents);
    setConversations(ensured);
    return ensured;
  }, [ensureConversation]);

  const loadMessages = useCallback(async (id: string) => {
    return listChatMessages(id);
  }, []);

  const loadProviders = useCallback(async (agentId: AgentId) => {
    try {
      const list = await listProviders(agentId);
      setProviders(list);
    } catch {
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
              const created = await createConversation(boot.agentIds, boot.cwd ?? null);
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
                  title: '已从 Projects 创建会话',
                  description: '提示词已填入；确认工作目录后发送。',
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
      setProviders([]);
      return;
    }
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

  const sendingTitle = useMemo(() => {
    if (!sendingConversationId) return '';
    const row = conversations.find((c) => c.id === sendingConversationId);
    return conversationTitle(row?.title ?? '');
  }, [conversations, sendingConversationId]);

  const blockers = useMemo(() => {
    if (!active) return [];
    return sendBlockers({
      conversation: active,
      hiddenIds,
      sendingConversationId,
      sendingTitle,
    });
  }, [active, hiddenIds, sendingConversationId, sendingTitle]);

  const railGroups = useMemo(() => {
    const filtered = filterConversations(conversations, railQuery);
    return groupConversationsByDay(filtered, Date.now());
  }, [conversations, railQuery]);

  const filteredCount = useMemo(
    () => filterConversations(conversations, railQuery).length,
    [conversations, railQuery],
  );

  const retry = useMemo(() => retryTarget(turns, sending), [turns, sending]);

  const agentPickerLabel = useMemo(() => agentPickerLabelOf(active), [active]);

  const modelPickerLabel = useMemo(() => {
    if (!primaryAgent) return '切换连接';
    if (switchingProvider) return '切换中…';
    if (!currentProvider) return '未配置连接';
    return currentProvider.name;
  }, [primaryAgent, currentProvider, switchingProvider]);

  const modelPickerSubtitle = useMemo(() => {
    if (!currentProvider || switchingProvider) return null;
    return extractModel(currentProvider.configText);
  }, [currentProvider, switchingProvider]);

  const connectionCaption = useMemo(
    () =>
      active
        ? connectionPickerCaption({
            agentIds: active.agentIds,
            primaryAgent,
          })
        : null,
    [active, primaryAgent],
  );

  async function handleNewChat() {
    const defaults = newConversationDefaults(active, agentStatus);
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

  async function renameTitle(next: string) {
    if (!active) return false;
    const title = next;
    try {
      const updated = await updateConversation(active.id, { title });
      setConversations((prev) => prev.map((c) => (c.id === updated.id ? updated : c)));
      return true;
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
      return false;
    }
  }

  async function toggleConversationAgent(id: AgentId) {
    if (!active || sending) return;
    if (installed.get(id) === false) return;
    if (hiddenIds.has(id) && !active.agentIds.includes(id)) return;
    const set = new Set(active.agentIds);
    if (set.has(id)) {
      if (set.size === 1) {
        toast({ title: '至少保留一个 Agent', variant: 'danger' });
        return;
      }
      set.delete(id);
    } else {
      set.add(id);
    }
    const next = AGENT_IDS.filter((a) => set.has(a));
    await patchActive({ agentIds: next });
  }

  async function handleSwitchProvider(providerId: string) {
    if (!primaryAgent || switchingProvider || hiddenIds.has(primaryAgent)) return;
    setSwitchingProvider(true);
    try {
      await switchProvider(primaryAgent, providerId);
      await loadProviders(primaryAgent);
      toast({ title: '已切换连接', variant: 'success' });
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
      sendingConversationId,
      sendingTitle,
    }).length > 0) {
      return;
    }
    if (!prompt) return;

    const sendConvId = active.id;
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
      setSending(false);
      setSendingConversationId(null);
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
        title: '已请求取消',
        description: '正在停止当前生成，过程面板将显示已取消。',
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
    providers,
    error,
    listLoading,
    messagesLoading,
    sending,
    sendingHere,
    sendingConversationId,
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
    installed,
    primaryAgent,
    hasUsableAgent,
    activeHasHidden,
    agentPickerLabel,
    modelPickerLabel,
    modelPickerSubtitle,
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
    renameTitle,
    toggleConversationAgent,
    handleSwitchProvider,
    handleSend,
    retryLast,
    handleCancel,
    cancelSending: handleCancel,
    retryLoad,
    focusConversation,
  };
}
