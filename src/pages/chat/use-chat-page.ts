import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { AGENT_IDS } from '@/config/agents';
import { listChatMessages, updateConversation } from '@/lib/api/chat';
import { pickDirectory } from '@/lib/api/settings';
import type { AgentKey, ChatMessage } from '@/lib/types';
import { groupByTurn } from './chat-format';
import {
  agentChatEnvReady,
  agentHasConfiguredAuth,
  agentPickerLabel as agentPickerLabelOf,
  chatAgentPickerRows,
  filterConversations,
  groupConversationsByDay,
  isChatAgentSelectable,
  selectConversationAgent,
} from './chat-model';
import { useChatPageChrome } from './use-chat-page-chrome';
import { useChatPageConnection } from './use-chat-page-connection';
import { useChatPageSend } from './use-chat-page-send';
import { useChatPageSessions } from './use-chat-page-sessions';

export {
  conversationListState,
  createSingleFlight,
  isCurrentChatRequest,
} from './chat-request';

const STICK_THRESHOLD_PX = 80;

export function useChatPage() {
  const { t } = useI18n();
  const { toast } = useToast();
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  /** 当前会话消息 / provider 加载 */
  const [messagesLoading, setMessagesLoading] = useState(false);
  const [messagesError, setMessagesError] = useState<unknown>(null);
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
    const m = new Map<AgentKey, boolean>();
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
    agentsReady,
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

  const hasUsableAgent = agentsReady && agentStatus.some((a) => isChatAgentSelectable(a));

  const connection = useChatPageConnection({
    primaryAgent,
    active,
    hiddenIds,
    agentStatus,
    refreshAgents,
  });

  // messages 与 providers 独立并发（不再串在 loadList 之后的瀑布里）
  useEffect(() => {
    stickToBottomRef.current = true;
    setMessages([]);
    if (!activeId) {
      setMessagesError(null);
      setMessagesLoading(false);
      return;
    }
    let cancelled = false;
    setMessagesLoading(true);
    setMessagesError(null);
    loadMessages(activeId)
      .then((rows) => {
        if (!cancelled && activeIdRef.current === activeId) {
          setMessages(rows);
          setMessagesError(null);
        }
      })
      .catch((e) => {
        if (!cancelled && activeIdRef.current === activeId) {
          setMessages([]);
          setMessagesError(e);
        }
      })
      .finally(() => {
        if (!cancelled && activeIdRef.current === activeId) setMessagesLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [activeId, loadMessages]);

  const retryMessages = useCallback(() => {
    if (!activeId) return;
    const requestedId = activeId;
    setMessages([]);
    setMessagesLoading(true);
    setMessagesError(null);
    loadMessages(requestedId)
      .then((rows) => {
        if (activeIdRef.current === requestedId) {
          setMessages(rows);
          setMessagesError(null);
        }
      })
      .catch((e) => {
        if (activeIdRef.current === requestedId) {
          setMessages([]);
          setMessagesError(e);
        }
      })
      .finally(() => {
        if (activeIdRef.current === requestedId) setMessagesLoading(false);
      });
  }, [activeId, loadMessages]);

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

  async function selectConversationAgentId(id: AgentKey) {
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

  return {
    conversations,
    activeId,
    active,
    messages,
    turns,
    agentStatus,
    agentsReady,
    providers: connection.providers,
    error,
    listLoading,
    messagesLoading,
    messagesError,
    retryMessages,
    sending: send.sending,
    sendingHere: send.sendingHere,
    cancelingHere: send.cancelingHere,
    sendingConversationId: send.sendingConversationId,
    draft,
    setDraft,
    railOpen,
    setRailOpen,
    settingsOpen,
    setSettingsOpen,
    dangerConfirm,
    setDangerConfirm,
    switchingProvider: connection.switchingProvider,
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
    connectionView: connection.connectionView,
    connectionOptions: connection.connectionOptions,
    connectionCaption: connection.connectionCaption,
    walletError: connection.walletError,
    reloadWallet: connection.reloadWallet,
    refreshAgents,
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
    handleSwitchConnection: connection.handleSwitchConnection,
    handleSwitchModel: connection.handleSwitchModel,
    modelOptions: connection.modelOptions,
    currentModel: connection.currentModel,
    switchingModel: connection.switchingModel,
    handleSend: send.handleSend,
    retryLast: send.retryLast,
    handleCancel: send.handleCancel,
    runtime: send.runtime,
    submitRuntimeRequest: send.submitRuntimeRequest,
    steerRuntime: send.steerRuntime,
    cancelSending: send.handleCancel,
    retryLoad,
    focusConversation,
  };
}
