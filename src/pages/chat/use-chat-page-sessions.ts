import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';
import { useSearchParams } from 'react-router-dom';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { listAgents } from '@/lib/api/agent';
import {
  createConversation,
  deleteConversation,
  ensureDefaultConversation,
  listConversations,
  updateConversation,
} from '@/lib/api/chat';
import { takeChatBootstrap } from '@/lib/chat-bootstrap';
import type { AgentId, AgentStatus, ChatMessage, Conversation } from '@/lib/types';
import { isChatAgentSelectable, newConversationDefaults } from './chat-model';
import { conversationListState, createSingleFlight } from './chat-request';

/**
 * Chat 会话列表：加载、空列表补建、项目跳转、新建 / 删除。
 * 单飞与列表提交仍走 createSingleFlight / conversationListState。
 * 不改发送、取消、切会话、连接切换语义。
 */
export function useChatPageSessions(input: {
  setMessages: Dispatch<SetStateAction<ChatMessage[]>>;
  setDraft: Dispatch<SetStateAction<string>>;
  deleteConfirmId: string | null;
  setDeleteConfirmId: Dispatch<SetStateAction<string | null>>;
  sendRef: MutableRefObject<{
    adoptInflight: (id: string | null) => void;
    cancelIfSending: (id: string) => Promise<void>;
  }>;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();
  const {
    setMessages,
    setDraft,
    deleteConfirmId,
    setDeleteConfirmId,
    sendRef,
  } = input;

  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [agentStatus, setAgentStatus] = useState<AgentStatus[]>([]);
  /** listAgents 成功后才为 true；失败/未完成时不得把「未知」当成「没有可用 Agent」 */
  const [agentsReady, setAgentsReady] = useState(false);
  const [error, setError] = useState<unknown>(null);
  /** 会话列表骨架（不再用整页 spinner 挡住消息区） */
  const [listLoading, setListLoading] = useState(true);
  const ensureSingleFlightRef = useRef<ReturnType<typeof createSingleFlight<Conversation[]>> | null>(
    null,
  );
  const loadListSingleFlightRef = useRef<ReturnType<typeof createSingleFlight<Conversation[]>> | null>(
    null,
  );
  const loadGenerationRef = useRef(0);
  /** Projects 页跳转：bootstrap 只处理一次 */
  const bootstrapDoneRef = useRef(false);

  const active = useMemo(
    () => conversations.find((c) => c.id === activeId) ?? null,
    [conversations, activeId],
  );

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
      sendRef.current.adoptInflight(inflight);
      return next;
    });
  }, [ensureConversation, refreshAgents]);

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
      await sendRef.current.cancelIfSending(id);
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
  };
}
