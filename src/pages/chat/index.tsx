import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useSearchParams } from 'react-router-dom';
import {
  FolderOpen,
  Loader2,
  MessagesSquare,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Settings2,
  Trash2,
} from 'lucide-react';
import { AgentLogo } from '@/components/shared/AgentLogo';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { MarkdownView } from '@/components/shared/MarkdownView';
import { Button } from '@/components/ui/button';
import { ListSkeleton, Skeleton } from '@/components/ui/skeleton';
import { Hint, Tip } from '@/components/ui/tooltip';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import { useToast } from '@/components/ui/toast';
import { AGENT_IDS, AGENT_MAP, agentDisplayName } from '@/config/agents';
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
import {
  hasProcessDetails,
  processKey,
  processPhaseLabel,
  reduceProcessEvent,
  type AgentProcessView,
  type ProcessMap,
} from '@/lib/chat-process';
import type {
  AgentId,
  AgentStatus,
  ChatEvent,
  ChatMessage,
  Conversation,
  Provider,
} from '@/lib/types';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { cn } from '@/lib/utils';
import { ChatComposer } from './ChatComposer';
import { ChatProcessPanel } from './ChatProcessPanel';
import {
  extractModel,
  formatDurationMs,
  groupByTurn,
  relativeTime,
} from './chat-format';

export default function ChatPage() {
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
  const bottomRef = useRef<HTMLDivElement>(null);
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

  const defaultAgents = useCallback(
    (agents: AgentStatus[]): AgentId[] => {
      const installedIds = agents
        .filter((a) => a.installed && !a.hidden)
        .map((a) => a.agentId);
      if (installedIds.length > 0) return [installedIds[0]];
      return [];
    },
    [],
  );

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
                  description: next.cwd
                    ? '提示词已填入；确认 cwd 后发送即可。'
                    : '提示词已填入；建议先设置工作目录再发送。',
                  variant: 'success',
                });
                if (!next.cwd) setSettingsOpen(true);
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

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: 'nearest' });
  }, [messages, sending]);

  const turns = useMemo(() => groupByTurn(messages), [messages]);

  async function handleNewChat() {
    const agents = defaultAgents(agentStatus);
    if (agents.length === 0) {
      toast({
        title: '没有可对话的 Agent',
        description: '请先安装或取消隐藏 Agent',
        variant: 'danger',
      });
      return;
    }
    try {
      const conv = await createConversation(agents, active?.cwd ?? null);
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
        const ids = defaultAgents(agentStatus);
        if (ids.length === 0) {
          setConversations([]);
          setActiveId(null);
          setMessages([]);
          setDraft('');
          return;
        }
        const created = await createConversation(ids, active?.cwd ?? null);
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

  async function patchActive(patch: Parameters<typeof updateConversation>[1]) {
    if (!active) return;
    try {
      const updated = await updateConversation(active.id, patch);
      setConversations((prev) => prev.map((c) => (c.id === updated.id ? updated : c)));
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
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

  async function handleSend() {
    if (!active || sending) return;
    if (active.agentIds.some((id) => hiddenIds.has(id))) {
      toast({
        title: '当前会话包含已隐藏 Agent',
        description: '请到 Agents 页取消隐藏后再发送',
        variant: 'danger',
      });
      return;
    }
    const prompt = draft.trim();
    if (!prompt) return;
    if (!active.cwd) {
      toast({
        title: '请先设置工作目录',
        description: '点击输入框旁的设置，填写 cwd',
        variant: 'danger',
      });
      setSettingsOpen(true);
      return;
    }

    const sendConvId = active.id;
    setSending(true);
    setSendingConversationId(sendConvId);
    setDraft('');
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

  const agentPickerLabel = useMemo(() => {
    if (!active) return '选择 Agent';
    if (active.agentIds.length === 1) return AGENT_MAP[active.agentIds[0]].name;
    return `${active.agentIds.length} 个 Agent`;
  }, [active]);

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

  if (error && conversations.length === 0 && !listLoading) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <ErrorState
          error={error}
          onRetry={() => {
            setListLoading(true);
            setError(null);
            loadList()
              .then((c) => c[0] && setActiveId(c[0].id))
              .catch(setError)
              .finally(() => setListLoading(false));
          }}
        />
      </div>
    );
  }

  const messageStatusLabel = (status: string, process?: AgentProcessView) => {
    // 过程机更细（排队/启动）；终态以 message.status 为准
    if (process && (status === 'running' || !status)) {
      if (process.phase === 'queued' || process.phase === 'starting' || process.phase === 'running') {
        return processPhaseLabel(process.phase);
      }
    }
    switch (status) {
      case 'running':
        return '生成中';
      case 'error':
      case 'failed':
        return '失败';
      case 'cancelled':
        return '已取消';
      case 'timeout':
        return '超时';
      case 'ok':
      case 'done':
      case 'success':
        return null; // 成功态不显示
      default:
        return status;
    }
  };

  return (
    <div className="flex h-full min-h-0 bg-canvas">
      {/* 左侧会话 rail：画布色，与主区白底分层 */}
      <aside
        className={cn(
          'flex shrink-0 flex-col border-r border-border bg-canvas transition-[width] duration-200',
          railOpen ? 'w-56' : 'w-0 overflow-hidden border-r-0',
        )}
      >
        <div className="flex items-center gap-1.5 p-2">
          <Hint label="收起历史">
            <button
              type="button"
              className="flex h-7 w-7 shrink-0 items-center justify-center rounded-btn text-muted hover:bg-hover hover:text-primary"
              onClick={() => setRailOpen(false)}
              aria-label="收起历史"
            >
              <PanelLeftClose className="h-4 w-4" />
            </button>
          </Hint>
          <Button className="min-w-0 flex-1 justify-start gap-1.5" size="sm" variant="secondary" onClick={() => void handleNewChat()}>
            <Plus className="h-3.5 w-3.5" />
            新建对话
          </Button>
        </div>
        <div className={cn('px-3 pb-1.5', pageRhythm.sectionEyebrow)}>
          对话历史
        </div>
        <div className="flex-1 overflow-y-auto px-1.5 pb-3">
          {listLoading ? (
            <div className="space-y-2 px-1 pt-1">
              {Array.from({ length: 5 }).map((_, i) => (
                <Skeleton key={i} className="h-10 w-full rounded-btn" />
              ))}
            </div>
          ) : conversations.length === 0 ? (
            <div className="px-2 py-4 text-center">
              <p className="text-xs text-muted">暂无对话</p>
              <p className="mt-1 text-xs text-muted">点上方「新建对话」开始</p>
            </div>
          ) : (
            conversations.map((c) => {
              const selected = activeId === c.id;
              return (
                <div
                  key={c.id}
                  className={cn(
                    'group mb-0.5 flex items-center rounded-btn',
                    selected ? 'bg-hover' : 'hover:bg-hover/70',
                  )}
                >
                  <button
                    type="button"
                    onClick={() => setActiveId(c.id)}
                    className={cn(
                      'min-w-0 flex-1 px-2 py-1.5 text-left text-sm',
                      selected ? 'font-medium text-primary' : 'text-secondary',
                    )}
                  >
                    <span className="flex items-center gap-2">
                      <MessagesSquare className="h-3.5 w-3.5 shrink-0 opacity-50" />
                      <span className="truncate">{c.title || '新对话'}</span>
                    </span>
                    <span className="mt-0.5 block pl-5 text-xs text-muted">
                      {relativeTime(c.updatedAt)}
                    </span>
                  </button>
                  <Hint label="删除">
                    <button
                      type="button"
                      className="mr-1 rounded-btn p-1 opacity-0 transition-opacity hover:bg-panel group-hover:opacity-100"
                      aria-label="删除"
                      onClick={() => void handleDelete(c.id)}
                    >
                      <Trash2 className="h-3.5 w-3.5 text-muted hover:text-danger" />
                    </button>
                  </Hint>
                </div>
              );
            })
          )}
        </div>
      </aside>

      {/* 右侧：对话窗口 */}
      <section className="relative flex min-w-0 flex-1 flex-col bg-panel">
        <header
          className={cn(
            'flex h-10 shrink-0 items-center gap-2 border-b border-border',
            pageRhythm.chatChromeX,
          )}
        >
          {!railOpen && (
            <Hint label="展开历史">
              <button
                type="button"
                className="flex h-7 w-7 items-center justify-center rounded-btn text-muted hover:bg-hover hover:text-primary"
                onClick={() => setRailOpen(true)}
                aria-label="展开历史"
              >
                <PanelLeftOpen className="h-4 w-4" />
              </button>
            </Hint>
          )}
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-sm font-semibold text-primary">
              {active?.title || (active ? '新对话' : '对话')}
              {activeHasHidden && (
                <span className="ml-2 text-xs font-normal text-muted">已隐藏</span>
              )}
            </h1>
          </div>
          {active && (
            <Hint label="会话设置">
              <button
                type="button"
                className="flex h-7 w-7 items-center justify-center rounded-btn text-muted hover:bg-hover hover:text-primary"
                aria-label="会话设置"
                onClick={() => setSettingsOpen(true)}
              >
                <Settings2 className="h-4 w-4" />
              </button>
            </Hint>
          )}
        </header>

        {listLoading && !active ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6">
            <ListSkeleton rows={4} className="w-full max-w-2xl" />
          </div>
        ) : null}

        {active && (
          <>
            {/* 消息区可滚；composer 始终贴底，避免长文把输入框顶出视口 */}
            <div className="min-h-0 flex-1 overflow-y-auto">
              {messagesLoading && turns.length === 0 ? (
                <div className="flex h-full flex-col justify-center p-6">
                  <ListSkeleton rows={3} className="mx-auto w-full max-w-2xl" />
                </div>
              ) : turns.length === 0 ? (
                <div className="flex h-full flex-col items-center justify-center px-6 py-10">
                  <div className="text-center">
                    <p className="text-title font-semibold tracking-tight text-primary">
                      开始对话
                    </p>
                    <p className="mt-2 max-w-md text-sm text-muted">
                      选择 Agent 与连接后输入；多选可并排对比
                    </p>
                  </div>
                </div>
              ) : (
                <div className="mx-auto w-full max-w-3xl space-y-6 px-4 py-6">
                  {turns.map((g) => (
                    <div key={g.turn} className="space-y-4">
                      {g.user && (
                        <div className="flex justify-end">
                          <div className="max-w-[85%] rounded-composer bg-subtle px-4 py-2 text-sm text-primary">
                            {/* User prompts are usually plain text; markdown still harmless. */}
                            <MarkdownView content={g.user.content} variant="chat" />
                          </div>
                        </div>
                      )}
                      {g.agents.map((m) => {
                        const agent = m.agentId ?? 'claude';
                        const proc = processMap[processKey(m.turn, agent)];
                        const statusText = messageStatusLabel(m.status, proc);
                        return (
                          <div key={m.id} className="flex gap-3">
                            <AgentLogo agentId={agent} size="md" />
                            <div className="min-w-0 flex-1 pt-0.5">
                              <div className="mb-1 flex items-center gap-2 text-xs text-muted">
                                <span className="font-medium text-secondary">
                                  {agentDisplayName(agent)}
                                </span>
                                {statusText && <span>{statusText}</span>}
                                {m.durationMs > 0 && (
                                  <span>{formatDurationMs(m.durationMs)}</span>
                                )}
                              </div>
                              {hasProcessDetails(proc) && proc ? (
                                <ChatProcessPanel
                                  view={proc}
                                  messageStatus={m.status}
                                  durationMs={m.durationMs}
                                  exitCode={m.exitCode}
                                />
                              ) : null}
                              <div className="text-sm leading-relaxed text-primary">
                                {m.content ? (
                                  <MarkdownView content={m.content} variant="chat" />
                                ) : m.status === 'running' ? (
                                  <span className="inline-flex items-center gap-2 text-muted">
                                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                                    {proc
                                      ? `${processPhaseLabel(proc.phase)}…`
                                      : '生成中…'}
                                  </span>
                                ) : (
                                  <span className="text-muted">{m.error || '（无输出）'}</span>
                                )}
                                {m.error && m.status !== 'ok' && m.content && (
                                  <p className="mt-2 text-sm text-danger">{m.error}</p>
                                )}
                              </div>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  ))}
                  <div ref={bottomRef} />
                </div>
              )}
            </div>

            <div
              className={cn(
                'shrink-0 border-t border-border/60 bg-canvas pb-4 pt-2',
                pageRhythm.chatChromeX,
              )}
            >
              <div className="mx-auto w-full max-w-3xl">
                <ChatComposer
                  draft={draft}
                  setDraft={setDraft}
                  sending={sending}
                  active={active}
                  installed={installed}
                  providers={providers}
                  primaryAgent={primaryAgent}
                  agentPickerLabel={agentPickerLabel}
                  modelPickerLabel={modelPickerLabel}
                  modelPickerSubtitle={modelPickerSubtitle}
                  switchingProvider={switchingProvider}
                  onSend={() => void handleSend()}
                  onCancel={() => void handleCancel()}
                  onToggleAgent={(id) => void toggleConversationAgent(id)}
                  onSwitchProvider={(id) => void handleSwitchProvider(id)}
                  hiddenIds={hiddenIds}
                />
              </div>
            </div>
          </>
        )}
        {!active && !listLoading && (
          <div className="flex flex-1 items-center justify-center p-6">
            <EmptyState
              icon={MessagesSquare}
              title="未选择对话"
              description="选历史，或新建"
              actionLabel="新建"
              onAction={() => void handleNewChat()}
            />
          </div>
        )}
      </section>

      {/* 会话设置：cwd / 自动批准 */}
      <Dialog open={settingsOpen} onOpenChange={setSettingsOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>会话设置</DialogTitle>
            <DialogDescription>工作目录与自动批准</DialogDescription>
          </DialogHeader>
          {active && (
            <div className="space-y-4 py-2">
              <div>
                <label className="mb-1.5 flex items-center gap-1.5 text-xs text-muted">
                  <FolderOpen className="h-3.5 w-3.5" />
                  工作目录
                </label>
                <Input
                  placeholder="例如 D:\\projects\\demo"
                  defaultValue={active.cwd ?? ''}
                  onBlur={(e) => {
                    const v = e.target.value.trim();
                    void patchActive({ cwd: v || null });
                  }}
                />
              </div>
              <label className="flex items-center justify-between gap-3 text-sm">
                <span>
                  <span className="block font-medium">自动批准</span>
                  <Tip className="text-xs text-muted" label="关闭时 CLI 遇审批可能等到超时">
                    跳过工具确认
                  </Tip>
                </span>
                <Switch
                  checked={active.allowDangerous}
                  onCheckedChange={(checked) => {
                    if (checked) {
                      setDangerConfirm(true);
                      return;
                    }
                    void patchActive({ allowDangerous: false });
                  }}
                />
              </label>
            </div>
          )}
          <DialogFooter>
            <Button onClick={() => setSettingsOpen(false)}>完成</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={dangerConfirm} onOpenChange={setDangerConfirm}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>开启自动批准？</DialogTitle>
            <DialogDescription>
              开启后将跳过工具确认，Agent 可直接改文件、执行命令。仅在信任当前工作目录时开启。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setDangerConfirm(false)}>
              取消
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                setDangerConfirm(false);
                void patchActive({ allowDangerous: true });
              }}
            >
              确认开启
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
