import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { AGENT_IDS } from '@/config/agents';
import { listChatMessages, updateConversation } from '@/lib/api/chat';
import { pickDirectory } from '@/lib/api/settings';
import type { AgentKey, ChatMessage } from '@/lib/types';
import { groupByTurn } from './chat-format';
import { forgetFallbackCwd, peekFallbackCwd } from '@/lib/chat-cwd-fallback';
import {
  agentChatEnvReady,
  agentHasConfiguredAuth,
  agentPickerLabel as agentPickerLabelOf,
  chatAgentPickerRows,
  chatModNShouldStartNewChat,
  composerNativeEditChord,
  conversationCwdMissing,
  firstUserContentByConversation,
  firstUserContentByListedConversations,
  filterConversations,
  mergeFirstUserContentById,
  groupConversationsByDay,
  isChatAgentSelectable,
  selectConversationAgent,
} from './chat-model';
import { useChatPageChrome } from './use-chat-page-chrome';
import { useChatPageConnection } from './use-chat-page-connection';
import { useChatPageSend } from './use-chat-page-send';
import { useChatPageSessions } from './use-chat-page-sessions';
import { useChatRuntimeOps } from './use-chat-runtime-ops';
import { useNavigate } from 'react-router-dom';
import {
  chatActionDisabledReason,
  clampActionIndex,
  filterChatActions,
  isCommandSearchMode,
  nativeCommandActions,
  type ChatActionDef,
} from './chat-actions';
import { nativeCommandMenuEnabled } from './chat-runtime-model';
import { composerEnterShouldSubmit } from './chat-composer-model';
import { lastTurnOutcome } from './chat-turn-outcome';
import { kiroChatAllowsCommandSearch, kiroChatStance } from './chat-kiro-model';
import { chatEffortLabel, chatModelDisplayName } from './chat-model-labels';
import { bindRuntimeSnapshotToAgent, isRuntimeSessionLocked } from './chat-runtime-model';

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
    adoptInflight: (ids?: string[] | string | null) => void;
    cancelIfSending: (id: string) => Promise<void>;
  }>({
    adoptInflight: () => {},
    cancelIfSending: async () => {},
  });

  const sessions = useChatPageSessions({
    setMessages,
    draft,
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
  const firstUserContentById = useMemo(
    () =>
      mergeFirstUserContentById(
        firstUserContentByListedConversations(conversations),
        firstUserContentByConversation(messages),
      ),
    [conversations, messages],
  );
  const startExtrasRef = useRef<{ images?: { path: string }[]; skills?: { name: string; path: string }[] }>({});
  const runtimeOpsClearRef = useRef<() => void>(() => {});

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
    getStartExtras: () => startExtrasRef.current,
    clearStartExtras: () => runtimeOpsClearRef.current(),
  });
  sendRef.current = {
    adoptInflight: send.adoptInflight,
    cancelIfSending: send.cancelIfSending,
  };
  const sending = send.sending;
  const activeRuntime = bindRuntimeSnapshotToAgent(send.runtime, {
    agentId: active?.agentIds[0],
    conversationId: active?.id,
  });
  const runtimeOps = useChatRuntimeOps({
    active,
    runtimeEnabled: Boolean(activeRuntime?.enabled),
    turnActive: send.sendingHere,
    catalogEpoch: activeRuntime?.catalogEpoch ?? 0,
  });
  startExtrasRef.current = runtimeOps.startExtras;
  runtimeOpsClearRef.current = runtimeOps.clearAttachments;

  const navigate = useNavigate();
  const [searchFocusNonce, setSearchFocusNonce] = useState(0);
  const [historyRevealNonce, setHistoryRevealNonce] = useState(0);
  const [composerFocusNonce, setComposerFocusNonce] = useState(0);
  const [commandIndex, setCommandIndex] = useState(0);
  const hasLatestReply = useMemo(
    () => messages.some((m) => m.role === 'agent' && m.content.trim()),
    [messages],
  );
  const actionContext = useMemo(
    () => ({
      hasLatestReply,
      newChatAllowed: !(agentsReady && !agentStatus.some((a) => isChatAgentSelectable(a))),
    }),
    [agentStatus, agentsReady, hasLatestReply],
  );

  const runtimeCommandActions = useMemo<ChatActionDef[]>(() => {
    if (!send.runtime?.enabled || kiroChatStance(active?.agentIds[0])) return [];
    const actions: ChatActionDef[] = [];
    if (!runtimeOps.frozen) {
      for (const model of runtimeOps.models) {
        actions.push({
          id: `runtime-model:${model.id}`,
          kind: 'local',
          label: `${t('chat.composer.switchModel')}：${chatModelDisplayName(model.id, t)}`,
          description: model.id === runtimeOps.settings.model ? t('chat.runtimeOps.currentModel') : undefined,
          keywords: ['model', '模型', '换模型', model.id, chatModelDisplayName(model.id, t)],
        });
      }
      if (runtimeOps.settings.model) {
        for (const effort of runtimeOps.currentEfforts) {
          actions.push({
            id: `runtime-effort:${effort}`,
            kind: 'local',
            label: `${t('chat.runtimeOps.effort')}：${chatEffortLabel(effort, t)}`,
            description: effort === runtimeOps.settings.effort ? t('chat.runtimeOps.currentSetting') : undefined,
            keywords: ['think', 'thinking', 'effort', '思考', '思考强度', effort, chatEffortLabel(effort, t)],
          });
        }
      }
    }
    for (const item of runtimeOps.extensions) {
      if (item.kind !== 'skill' || !item.callable) continue;
      actions.push({
        id: `runtime-skill:${item.id}`,
        kind: 'local',
        label: `${runtimeOps.selectedSkillIds.includes(item.id) ? t('chat.runtimeOps.cancelUseForTurn') : t('chat.runtimeOps.useForTurn')}：${item.name}`,
        description: t('chat.runtimeOps.skill'),
        keywords: ['skill', '技能', '用于本次', item.name, item.id],
      });
    }
    if (
      nativeCommandMenuEnabled({
        sessionReady: runtimeOps.sessionReady,
        nativeCommands: runtimeOps.nativeCommands,
      })
    ) {
      actions.push(...nativeCommandActions(runtimeOps.nativeCommands));
    }
    return actions;
  }, [
    runtimeOps.currentEfforts,
    runtimeOps.extensions,
    runtimeOps.frozen,
    runtimeOps.models,
    runtimeOps.nativeCommands,
    runtimeOps.selectedSkillIds,
    runtimeOps.sessionReady,
    runtimeOps.settings.effort,
    runtimeOps.settings.model,
    send.runtime?.enabled,
    active?.agentIds,
    t,
  ]);

  const runChatAction = useCallback(
    (action: ChatActionDef) => {
      const reason = chatActionDisabledReason(action, actionContext);
      if (reason) {
        toast({ title: t(`chat.actions.disabled.${reason}` as never), variant: 'danger' });
        return;
      }
      const clearCommandDraft = () => {
        if (isCommandSearchMode(draft)) setDraft('');
      };
      if (action.id.startsWith('runtime-model:')) {
        clearCommandDraft();
        void runtimeOps.switchModel(action.id.slice('runtime-model:'.length));
        return;
      }
      if (action.id.startsWith('runtime-effort:')) {
        clearCommandDraft();
        void runtimeOps.switchEffort(action.id.slice('runtime-effort:'.length));
        return;
      }
      if (action.id.startsWith('runtime-skill:')) {
        clearCommandDraft();
        runtimeOps.toggleSkill(action.id.slice('runtime-skill:'.length));
        return;
      }
      if ((action.kind === 'draft' || action.kind === 'native') && action.draftText) {
        setDraft(action.draftText);
        setComposerFocusNonce((n) => n + 1);
        return;
      }
      if (action.id === 'new-session') {
        clearCommandDraft();
        void handleNewChat();
        return;
      }
      if (action.id === 'open-history') {
        clearCommandDraft();
        setRailOpen(true);
        setHistoryRevealNonce((n) => n + 1);
        return;
      }
      if (action.id === 'focus-history-search') {
        setRailOpen(true);
        setSearchFocusNonce((n) => n + 1);
        clearCommandDraft();
        return;
      }
      if (action.id === 'open-settings') {
        clearCommandDraft();
        setSettingsOpen(true);
        return;
      }
      if (action.id === 'open-agents') {
        clearCommandDraft();
        navigate('/agents');
        return;
      }
      if (action.id === 'open-connections') {
        clearCommandDraft();
        navigate('/connections');
        return;
      }
      if (action.id === 'copy-latest-reply') {
        const latest = [...messages].reverse().find((m) => m.role === 'agent' && m.content.trim());
        if (!latest) {
          toast({ title: t('chat.actions.disabled.noReply'), variant: 'danger' });
          return;
        }
        void navigator.clipboard.writeText(latest.content).then(
          () => toast({ title: t('chat.bubble.copied') }),
          () => toast({ title: t('chat.bubble.copyFailed'), variant: 'danger' }),
        );
        clearCommandDraft();
      }
    },
    [actionContext, draft, handleNewChat, messages, navigate, runtimeOps, setRailOpen, setSettingsOpen, t, toast],
  );
  const commandSearchOpen =
    isCommandSearchMode(draft) && kiroChatAllowsCommandSearch(active?.agentIds[0]);
  const commandItems = useMemo(
    () => filterChatActions(draft, runtimeCommandActions),
    [draft, runtimeCommandActions],
  );
  useEffect(() => {
    setCommandIndex(0);
  }, [draft, commandItems.length]);

  const handleComposerKeyDown = useCallback(
    (e: KeyboardEvent<HTMLTextAreaElement>) => {
      if (
        composerNativeEditChord({
          key: e.key,
          code: e.nativeEvent.code,
          metaKey: e.metaKey,
          ctrlKey: e.ctrlKey,
          altKey: e.altKey,
          shiftKey: e.shiftKey,
        })
      ) {
        return false;
      }
      if (
        chatModNShouldStartNewChat({
          key: e.key,
          code: e.nativeEvent.code,
          metaKey: e.metaKey,
          ctrlKey: e.ctrlKey,
          altKey: e.altKey,
          shiftKey: e.shiftKey,
          overlayOpen: false,
        })
      ) {
        e.preventDefault();
        runChatAction({
          id: 'new-session',
          kind: 'local',
          keywords: [],
        });
        return true;
      }
      if (!commandSearchOpen || commandItems.length === 0) return false;
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setCommandIndex((index) => clampActionIndex(index + 1, commandItems.length));
        return true;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        setCommandIndex((index) => clampActionIndex(index - 1, commandItems.length));
        return true;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        setDraft('');
        return true;
      }
      if (composerEnterShouldSubmit({
        key: e.key,
        shiftKey: e.shiftKey,
        composing: e.nativeEvent.isComposing,
        keyCode: e.nativeEvent.keyCode,
      })) {
        e.preventDefault();
        const action = commandItems[clampActionIndex(commandIndex, commandItems.length)];
        if (action) runChatAction(action);
        return true;
      }
      return false;
    },
    [commandIndex, commandItems, commandSearchOpen, runChatAction],
  );

  const turnOutcome = useMemo(
    () => lastTurnOutcome(turns, sending),
    [sending, turns],
  );
  const notedThinkingFailureRef = useRef<string | null>(null);

  useEffect(() => {
    if (turnOutcome?.kind !== 'failed') return;
    const key = `${activeId ?? ''}\n${turnOutcome.prompt}\n${turnOutcome.errorText ?? ''}`;
    if (notedThinkingFailureRef.current === key) return;
    notedThinkingFailureRef.current = key;
    void runtimeOps.noteThinkingFailure(turnOutcome.errorText);
  }, [activeId, runtimeOps, turnOutcome]);

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
    refreshRuntimeCatalog: runtimeOps.refresh,
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
  }, [messages, sending, send.processMap]);

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

  async function pickWorkingDirectory(nextPath?: string | null) {
    if (!active || send.sendingHere) return;
    try {
      const picked = nextPath?.trim()
        ? nextPath.trim()
        : await pickDirectory({
            title: t('chat.settings.pickDirTitle'),
            defaultPath: peekFallbackCwd(active.id) ?? active.cwd ?? null,
          });
      if (picked) {
        await patchActive({ cwd: picked });
        forgetFallbackCwd(active.id);
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
    if (!active || send.sendingHere) return;
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
    sendingConversationIds: send.sendingConversationIds,
    connectionLocked: Boolean(primaryAgent && send.busyAgentIds.has(primaryAgent)),
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
    cwdMissing: active ? conversationCwdMissing(active) : false,
    fallbackCwd: active ? peekFallbackCwd(active.id) : null,
    renameTitle,
    selectConversationAgentId,
    handleSwitchConnection: connection.handleSwitchConnection,
    handleSwitchModel: connection.handleSwitchModel,
    handleSwitchEffort: connection.handleSwitchEffort,
    modelOptions: connection.modelOptions,
    currentModel: connection.currentModel,
    effortOptions: connection.effortOptions,
    currentEffort: connection.currentEffort,
    switchingModel: connection.switchingModel,
    handleSend: send.handleSend,
    retryLast: send.retryLast,
    handleCancel: send.handleCancel,
    queuedFollowUps: send.queuedFollowUps,
    queuedFollowUpCount: send.queuedFollowUpCount,
    composerFocusNonce,
    cancelQueuedFollowUp: send.cancelQueuedFollowUp,
    clearQueuedFollowUp: send.clearQueuedFollowUp,
    continueLegacyGrok: send.continueLegacyGrok,
    runtime: activeRuntime,
    runtimeLocked: isRuntimeSessionLocked(activeRuntime, {
      conversationId: active?.id,
      nativeSessionId: active?.nativeSessionId,
      hasMessages: messages.length > 0,
    }),
    runtimeOps,
    runtimeCommandActions,
    commandSearchOpen,
    commandIndex,
    setCommandIndex,
    actionContext,
    runChatAction,
    handleComposerKeyDown,
    searchFocusNonce,
    historyRevealNonce,
    firstUserContentById,
    turnOutcome,
    submitRuntimeRequest: send.submitRuntimeRequest,
    steerRuntime: send.steerRuntime,
    cancelSending: send.handleCancel,
    retryLoad,
    focusConversation,
  };
}
