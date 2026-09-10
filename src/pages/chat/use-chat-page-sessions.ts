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
  openConversationFromSession,
  updateConversation,
} from '@/lib/api/chat';
import {
  chatBootstrapGeneration,
  isChatBootstrapHandoff,
  restoreChatBootstrapIfUnchanged,
  takeChatBootstrap,
} from '@/lib/chat-bootstrap';
import { rememberFallbackCwd } from '@/lib/chat-cwd-fallback';
import type { AgentKey, AgentStatus, ChatMessage, Conversation } from '@/lib/types';
import { draftForFocusedConversation, isChatAgentSelectable, newConversationDefaults, singleAgentConversationPatch } from './chat-model';
import { conversationListState, createSingleFlight } from './chat-request';

/** Keep conversations created by an in-flight shell/projects handoff when list load returns stale. */
export function mergeHandoffConversations(
  prev: Conversation[],
  loaded: Conversation[],
): Conversation[] {
  if (prev.length === 0) return loaded;
  const loadedIds = new Set(loaded.map((conversation) => conversation.id));
  const extras = prev.filter((conversation) => !loadedIds.has(conversation.id));
  if (extras.length === 0) return loaded;
  return [...extras, ...loaded];
}

/**
 * Chat 会话列表：加载、空列表补建、项目跳转、新建 / 删除。
 * 单飞与列表提交仍走 createSingleFlight / conversationListState。
 * 切会话保留各会话草稿；进行中的发送按会话恢复，不互相打断。
 */
export function useChatPageSessions(input: {
  setMessages: Dispatch<SetStateAction<ChatMessage[]>>;
  draft: string;
  setDraft: Dispatch<SetStateAction<string>>;
  deleteConfirmId: string | null;
  setDeleteConfirmId: Dispatch<SetStateAction<string | null>>;
  sendRef: MutableRefObject<{
    adoptInflight: (ids?: string[] | string | null) => void;
    cancelIfSending: (id: string) => Promise<void>;
  }>;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();
  const {
    setMessages,
    draft,
    setDraft,
    deleteConfirmId,
    setDeleteConfirmId,
    sendRef,
  } = input;
  const draftsRef = useRef(new Map<string, string>());
  const conversationsRef = useRef<Conversation[]>([]);

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
  const loadListAllowEnsureRef = useRef<boolean | null>(null);
  const loadGenerationRef = useRef(0);
  /** Multi-agent → single-agent one-shot migration runs once per load generation. */
  const migratedGenerationRef = useRef(-1);

  const active = useMemo(
    () => conversations.find((c) => c.id === activeId) ?? null,
    [conversations, activeId],
  );

  const defaultAgents = useCallback((agents: AgentStatus[]): AgentKey[] => {
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
   * shell/projects bootstrap 完成前不要自动建默认会话。
   */
  const loadList = useCallback((opts?: { allowEnsureDefault?: boolean }) => {
    const allowEnsureDefault = opts?.allowEnsureDefault ?? true;
    if (loadListAllowEnsureRef.current !== allowEnsureDefault) {
      loadListSingleFlightRef.current = createSingleFlight<Conversation[]>();
      loadListAllowEnsureRef.current = allowEnsureDefault;
    }
    if (!loadListSingleFlightRef.current) {
      loadListSingleFlightRef.current = createSingleFlight<Conversation[]>();
    }
    return loadListSingleFlightRef.current(async () => {
      const convs = await listConversations();
      let next = convs;
      if (convs.length > 0 || !allowEnsureDefault) {
        // agent 状态异步填充 picker，不挡列表；失败记 ready=false，允许重试
        void refreshAgents().catch(() => {});
      } else {
        const agents = await refreshAgents();
        next = await ensureConversation(convs, agents);
      }
      // 以服务端 sending 为准恢复进行中的会话；list 尚未带上 sending 时不要清掉本地 send。
      // 恢复失败不得把整份会话列表当成加载失败。
      try {
        sendRef.current.adoptInflight(next.filter((c) => c.sending).map((c) => c.id));
      } catch (e) {
        console.error('[chat] adoptInflight failed', e);
      }
      return next;
    });
  }, [ensureConversation, refreshAgents]);

  /**
   * One-shot migrate any legacy multi-agent conversations to single-agent
   * (product intent: `selectConversationAgent` 单选). Runs once per load
   * generation instead of on every open; failures are aggregated into a
   * single toast and never block the list from rendering.
   */
  const migrateMultiAgentConversations = useCallback(
    async (convs: Conversation[], generation: number): Promise<Conversation[]> => {
      if (migratedGenerationRef.current === generation) return convs;
      migratedGenerationRef.current = generation;
      const targets = convs.filter((c) => singleAgentConversationPatch(c.agentIds) !== null);
      if (targets.length === 0) return convs;

      const results = await Promise.allSettled(
        targets.map((c) => {
          const patch = singleAgentConversationPatch(c.agentIds);
          return patch ? updateConversation(c.id, patch) : Promise.reject(new Error('no patch'));
        }),
      );
      if (generation !== loadGenerationRef.current) return convs;

      const updatedById = new Map<string, Conversation>();
      let failures = 0;
      for (const result of results) {
        if (result.status === 'fulfilled') {
          updatedById.set(result.value.id, result.value);
        } else {
          failures += 1;
        }
      }
      if (failures > 0) {
        toast({ title: t('chat.toast.multiAgentMigrationFailed'), variant: 'danger' });
      }
      if (updatedById.size === 0) return convs;
      return convs.map((c) => updatedById.get(c.id) ?? c);
    },
    [t, toast],
  );

  const waitForHandoff = isChatBootstrapHandoff(searchParams.get('from'));

  useEffect(() => {
    const generation = ++loadGenerationRef.current;
    let cancelled = false;
    setListLoading(true);
    setError(null);
    loadList({ allowEnsureDefault: !waitForHandoff })
      .then(async (convs) => {
        if (cancelled || generation !== loadGenerationRef.current) return;
        // Commit the list and its initial selection together. Without this
        // commit the hook kept an empty in-memory rail even though the API
        // load succeeded, and bootstrap could accidentally discard existing
        // conversations when it prepended its new one.
        setConversations((prev) => {
          const next = mergeHandoffConversations(prev, convs);
          conversationsRef.current = next;
          return next;
        });
        setActiveId((current) => {
          const list = conversationsRef.current;
          if (current && list.some((conversation) => conversation.id === current)) return current;
          return conversationListState(list).activeId;
        });
        void migrateMultiAgentConversations(conversationsRef.current, generation).then((migrated) => {
          if (cancelled || generation !== loadGenerationRef.current) return;
          if (migrated !== conversationsRef.current) {
            conversationsRef.current = migrated;
            setConversations(migrated);
          }
        });
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
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reload when loader identity or handoff gate changes
  }, [loadList, waitForHandoff]);

  useEffect(() => {
    const from = searchParams.get('from');
    if (!isChatBootstrapHandoff(from)) return;
    const boot = takeChatBootstrap();
    const generation = chatBootstrapGeneration();
    if (!boot) {
      setSearchParams({}, { replace: true });
      return;
    }
    let cancelled = false;
    let applied = false;
    void (async () => {
      try {
        let ids = boot.agentIds.filter(Boolean).slice(0, 1);
        if (ids.length === 0) {
          const agents = await refreshAgents().catch(() => agentStatus);
          if (cancelled) return;
          ids = defaultAgents(agents);
        }
        if (cancelled) return;
        if (ids.length === 0) {
          toast({ title: t('chat.rail.newChatDisabled'), variant: 'danger' });
          setSearchParams({}, { replace: true });
          applied = true;
          return;
        }
        if (activeId) draftsRef.current.set(activeId, draft);
        const fromSession = Boolean(boot.sessionId?.trim() || boot.history?.length);
        let next;
        if (fromSession) {
          next = await openConversationFromSession({
            agentId: ids[0],
            sessionId: boot.sessionId,
            cwd: boot.cwd ?? null,
            title: boot.title,
            history: boot.history ?? [],
          });
          if (boot.fallbackCwd?.trim()) {
            rememberFallbackCwd(next.id, boot.fallbackCwd);
          }
        } else {
          const created = await createConversation(ids, boot.cwd ?? null);
          next = created;
          if (boot.title) {
            try {
              next = await updateConversation(created.id, { title: boot.title });
            } catch {
              /* title 可选 */
            }
          }
        }
        if (cancelled) return;
        setConversations((prev) => {
          const list = [next, ...prev.filter((c) => c.id !== next.id)];
          conversationsRef.current = list;
          return list;
        });
        setActiveId(next.id);
        setMessages([]);
        if (!fromSession && boot.prompt?.trim()) {
          setDraft(boot.prompt);
          toast({
            title: t('chat.toast.fromProjects'),
            description: t('chat.toast.fromProjectsDesc'),
            variant: 'success',
          });
        } else {
          setDraft('');
          if (fromSession) {
            toast({
              title: t('chat.toast.fromSession'),
              description: t('chat.toast.fromSessionDesc'),
              variant: 'success',
            });
          }
        }
        setSearchParams({}, { replace: true });
        applied = true;
      } catch (e) {
        if (cancelled) return;
        toast({
          title: e instanceof Error ? e.message : String(e),
          variant: 'danger',
        });
        setSearchParams({}, { replace: true });
        applied = true;
      }
    })();
    return () => {
      cancelled = true;
      if (!applied) restoreChatBootstrapIfUnchanged(boot, generation);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- one-shot when from= is set
  }, [searchParams]);

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
      if (activeId) draftsRef.current.set(activeId, draft);
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
      draftsRef.current.delete(id);
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
        setDraft(draftsRef.current.get(rest[0].id) ?? '');
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
    loadList({ allowEnsureDefault: !isChatBootstrapHandoff(searchParams.get('from')) })
      .then((next) => {
        if (cancelled || generation !== loadGenerationRef.current) return;
        conversationsRef.current = next;
        const committed = conversationListState(next);
        setConversations(committed.conversations);
        setActiveId(committed.activeId);
        void migrateMultiAgentConversations(committed.conversations, generation).then((migrated) => {
          if (cancelled || generation !== loadGenerationRef.current) return;
          if (migrated !== committed.conversations) {
            conversationsRef.current = migrated;
            setConversations(migrated);
          }
        });
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
    if (id !== activeId) {
      setMessages([]);
      setDraft(draftForFocusedConversation(draftsRef.current, activeId, id, draft));
    }
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
