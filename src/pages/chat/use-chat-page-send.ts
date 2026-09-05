import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import {
  chatCancel,
  chatSend,
  listChatMessages,
  listConversations,
  runtimeCancel,
  runtimeReply,
  runtimeSnapshot,
  runtimeStart,
  runtimeSteer,
} from '@/lib/api/chat';
import type { RuntimeRequest, RuntimeSnapshot } from '@/lib/api/chat';
import type { ProcessMap } from '@/lib/chat-process';
import type { AgentKey, ChatEvent, ChatMessage, Conversation } from '@/lib/types';
import type { TurnGroup } from './chat-format';
import { conversationTitle, retryTarget, sendBlockers } from './chat-model';
import { isCurrentChatRequest } from './chat-request';
import { acceptsRuntimeSnapshot, isLatestRuntimeRead, isRuntimeActive, readRuntimeTransport, requestMatchesRuntime } from './chat-runtime-model';
import {
  beginRuntimeStart,
  acceptRuntimeSnapshotVersion,
  advanceRuntimeWatermark,
  enqueueRuntimeSnapshot as enqueueRuntimeSnapshotSource,
  isRuntimeTerminal,
  isLatestRuntimeSnapshot,
  rememberRuntimeSnapshot,
  reduceRuntimeConversationEvent,
  runtimeConversationView,
  requestRuntimeCancel,
  upsertRuntimeMessage,
  type RuntimeConversationView,
  type RuntimeRunRecord,
  type RuntimeSnapshotVersion,
} from './runtime-run-state';

/**
 * Chat 发送 / 取消 / 流式事件 / 过程面板。
 * 世代判定仍走 isCurrentChatRequest；不改发送、取消、切会话语义。
 */
export function useChatPageSend(input: {
  active: Conversation | null;
  activeId: string | null;
  messages: ChatMessage[];
  setMessages: Dispatch<SetStateAction<ChatMessage[]>>;
  setConversations: Dispatch<SetStateAction<Conversation[]>>;
  conversations: Conversation[];
  hiddenIds: Set<AgentKey>;
  envNotReadyIds: Set<AgentKey>;
  unconfiguredAuthIds: Set<AgentKey>;
  agentsReady: boolean;
  activeIdRef: MutableRefObject<string | null>;
  activeGenerationRef: MutableRefObject<number>;
  loadMessages: (id: string) => Promise<ChatMessage[]>;
  draft: string;
  setDraft: Dispatch<SetStateAction<string>>;
  turns: TurnGroup[];
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const {
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
  } = input;

  const [sending, setSending] = useState(false);
  const [canceling, setCanceling] = useState(false);
  const [sendingConversationId, setSendingConversationId] = useState<string | null>(null);
  const sendingConversationIdRef = useRef<string | null>(null);
  const sendingOperationRef = useRef(0);
  const [processMap, setProcessMap] = useState<ProcessMap>({});
  const [runtime, setRuntime] = useState<RuntimeSnapshot | null>(null);
  const runtimeSequenceRef = useRef(new Map<string, number>());
  const runtimeRecordsRef = useRef(new Map<string, RuntimeRunRecord>());
  const runtimeViewsRef = useRef(new Map<string, RuntimeConversationView>());
  const runtimeSnapshotVersionsRef = useRef(new Map<string, RuntimeSnapshotVersion>());
  const runtimeCurrentMessagesRef = useRef(new Map<string, ChatMessage | null>());
  const runtimeSnapshotLaneRef = useRef(new Map<string, Promise<void>>());
  const runtimeSourceVersionRef = useRef(new Map<string, number>());
  const runtimeIdRef = useRef<string | null>(null);
  const runtimeReadRef = useRef(new Map<string, number>());
  const runtimeProbeRef = useRef(new Set<string>());
  const runtimeProbeCancelRef = useRef(new Set<string>());

  useEffect(() => {
    setProcessMap({});
    if (activeId) {
      setProcessMap(runtimeConversationView(runtimeViewsRef.current, activeId).processMap);
    }
    runtimeIdRef.current = runtimeRecordsRef.current.get(activeId ?? '')?.runId ?? null;
    setRuntime(null);
  }, [activeId]);

  const nextRuntimeRead = (conversationId: string) => {
    const next = (runtimeReadRef.current.get(conversationId) ?? 0) + 1;
    runtimeReadRef.current.set(conversationId, next);
    return next;
  };

  const runtimeSequence = (conversationId: string) =>
    runtimeSequenceRef.current.get(conversationId) ?? 0;

  const enqueueRuntimeSnapshot = (
    conversationId: string,
    source: () => Promise<RuntimeSnapshot>,
    handle: (snapshot: RuntimeSnapshot, sourceVersion: number) => void | Promise<void>,
  ) => {
    return enqueueRuntimeSnapshotSource(
      runtimeSnapshotLaneRef.current,
      runtimeSourceVersionRef.current,
      conversationId,
      source,
      handle,
    );
  };

  const clearSendingFor = (conversationId: string) => {
    if (sendingConversationIdRef.current !== conversationId) return;
    sendingOperationRef.current += 1;
    sendingConversationIdRef.current = null;
    setSending(false);
    setCanceling(false);
    setSendingConversationId(null);
  };

  const recordRuntimeSnapshot = (snapshot: RuntimeSnapshot) => {
    const record = rememberRuntimeSnapshot(runtimeRecordsRef.current, snapshot);
    if ('currentMessage' in snapshot) {
      runtimeCurrentMessagesRef.current.set(snapshot.conversationId, snapshot.currentMessage ?? null);
    }
    advanceRuntimeWatermark(
      runtimeSequenceRef.current,
      snapshot.conversationId,
      snapshot.lastSequence,
    );
    if (snapshot.conversationId === activeIdRef.current) runtimeIdRef.current = record.runId;
    return record;
  };

  const applyRuntimeSnapshot = (
    snapshot: RuntimeSnapshot,
    conversationId: string,
    generation: number,
    sourceVersion: number,
    applyUi: boolean,
  ) => {
    if (
      !acceptRuntimeSnapshotVersion(
        runtimeSnapshotVersionsRef.current,
        snapshot,
        sourceVersion,
      )
    ) return;
    const sequence = runtimeSequence(conversationId);
    recordRuntimeSnapshot(snapshot);
    const canRender =
      applyUi &&
      acceptsRuntimeSnapshot(activeIdRef.current, activeGenerationRef.current, conversationId, generation);
    if (canRender && snapshot.currentMessage) {
      setMessages((previous) => upsertRuntimeMessage(previous, snapshot.currentMessage!));
    }
    for (const item of snapshot.events) {
      if (item.sequence > sequence) applyEvent(item.event, conversationId, generation, applyUi, 'runtime');
    }
    advanceRuntimeWatermark(runtimeSequenceRef.current, conversationId, snapshot.lastSequence);
    if (!canRender) return;
    if (snapshot.gap) {
      void loadMessages(conversationId).then((rows) => {
        if (
          isCurrentChatRequest(activeIdRef.current, activeGenerationRef.current, conversationId, generation) &&
          isLatestRuntimeSnapshot(runtimeSnapshotVersionsRef.current, conversationId, sourceVersion)
        ) {
          const currentMessage = runtimeCurrentMessagesRef.current.get(conversationId);
          setMessages(currentMessage ? upsertRuntimeMessage(rows, currentMessage) : rows);
        }
      });
    }
    runtimeIdRef.current = snapshot.runId;
    setRuntime(snapshot);
    const activePhase = isRuntimeActive(snapshot.phase);
    if (activePhase) {
      sendingConversationIdRef.current = conversationId;
      setSendingConversationId(conversationId);
      setSending(true);
    } else if (sendingConversationIdRef.current === conversationId) {
      sendingConversationIdRef.current = null;
      setSendingConversationId(null);
      setSending(false);
      setCanceling(false);
      void loadMessages(conversationId).then((rows) => {
        if (
          isCurrentChatRequest(activeIdRef.current, activeGenerationRef.current, conversationId, generation) &&
          isLatestRuntimeSnapshot(runtimeSnapshotVersionsRef.current, conversationId, sourceVersion)
        ) {
          const currentMessage = runtimeCurrentMessagesRef.current.get(conversationId);
          setMessages(currentMessage ? upsertRuntimeMessage(rows, currentMessage) : rows);
        }
      });
    } else if (isRuntimeTerminal(runtimeRecordsRef.current.get(conversationId))) {
      clearSendingFor(conversationId);
    }
  };

  useEffect(() => {
    if (!activeId) return;
    let disposed = false;
    let inFlight = false;
    const id = activeId;
    const generation = activeGenerationRef.current;
    const read = async () => {
      if (inFlight) return;
      inFlight = true;
      const readId = nextRuntimeRead(id);
      try {
        await enqueueRuntimeSnapshot(
          id,
          () => runtimeSnapshot(id, runtimeSequence(id)),
          (snapshot, sourceVersion) => {
            applyRuntimeSnapshot(
              snapshot,
              id,
              generation,
              sourceVersion,
              !disposed && isLatestRuntimeRead(readId, runtimeReadRef.current.get(id) ?? 0),
            );
          },
        );
      } catch (error) {
        if (!disposed && isLatestRuntimeRead(readId, runtimeReadRef.current.get(id) ?? 0) && runtime?.enabled) {
          toast({ title: error instanceof Error ? error.message : String(error), variant: 'danger' });
        }
      } finally {
        inFlight = false;
      }
    };
    void read();
    const shouldPoll = runtime?.enabled && isRuntimeActive(runtime.phase);
    if (!shouldPoll) return () => { disposed = true; };
    const timer = window.setInterval(() => void read(), 400);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [activeId, runtime?.enabled, runtime?.phase]);

  // A run remains owned by its conversation while another session is selected.
  // Poll it lightly so a terminal state releases the page-wide sending guard.
  useEffect(() => {
    const id = sendingConversationId;
    if (!id || id === activeId) return;
    let disposed = false;
    let inFlight = false;
    const read = async () => {
      if (inFlight) return;
      inFlight = true;
      const readId = nextRuntimeRead(id);
      try {
        await enqueueRuntimeSnapshot(
          id,
          () => runtimeSnapshot(id, runtimeSequence(id)),
          (snapshot, sourceVersion) => {
            applyRuntimeSnapshot(
              snapshot,
              id,
              activeGenerationRef.current,
              sourceVersion,
              false,
            );
            if (!disposed && isLatestRuntimeRead(readId, runtimeReadRef.current.get(id) ?? 0) && isRuntimeTerminal(runtimeRecordsRef.current.get(id))) {
              clearSendingFor(id);
            }
          },
        );
      } catch {
        // The active session will surface a current error when it is revisited.
      } finally {
        inFlight = false;
      }
    };
    void read();
    const timer = window.setInterval(() => void read(), 400);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [activeId, sendingConversationId]);

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
      envNotReadyIds,
      unconfiguredAuthIds,
      agentsReady,
      sendingConversationId: liveSendingConversationId,
      sendingTitle,
    });
  }, [active, hiddenIds, envNotReadyIds, unconfiguredAuthIds, agentsReady, liveSendingConversationId, sendingTitle]);

  const retry = useMemo(() => retryTarget(turns, sending), [turns, sending]);

  function applyEvent(
    ev: ChatEvent,
    sendConvId: string,
    sendGeneration: number,
    render = true,
    mode: 'runtime' | 'legacy' = 'legacy',
  ) {
    const isCurrent = isCurrentChatRequest(
      activeIdRef.current,
      activeGenerationRef.current,
      sendConvId,
      sendGeneration,
    );
    const shouldRender = render && isCurrent;
    const view = reduceRuntimeConversationEvent(
      runtimeViewsRef.current,
      sendConvId,
      ev,
      shouldRender ? messages : [],
      mode,
    );
    if (
      !shouldRender
    ) {
      return;
    }
    // 过程面板状态（命令 / stderr / 细状态）与 messages 并行维护
    setProcessMap(view.processMap);

    if (ev.type === 'started') {
      if (mode === 'runtime') return;
      for (const agent of ev.agents) {
        setMessages((prev) => {
          const hasAgent = prev.some(
            (m) => m.turn === ev.turn && m.role === 'agent' && m.agentId === agent,
          );
          if (hasAgent) return prev;
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
      if (mode === 'runtime') return;
      const content = view.streams[`${ev.turn}:${ev.agent}`] ?? '';
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
        const existingById = withoutLocal.findIndex((m) => m.id === ev.message.id);
        if (existingById >= 0) {
          return withoutLocal.map((message, index) =>
            index === existingById ? ev.message : message,
          );
        }
        // Historical runtime events can be replayed after the DB message has
        // already been loaded. Keep that final row instead of appending it a
        // second time under a different message id.
        if (withoutLocal.some(
          (m) =>
            m.role === 'agent' &&
            m.agentId === ev.agent &&
            m.turn === ev.turn &&
            m.status !== 'running',
        )) {
          return withoutLocal;
        }
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
      envNotReadyIds,
      unconfiguredAuthIds,
      sendingConversationId: liveSendingConversationId,
      sendingTitle,
    }).length > 0) {
      return;
    }
    if (!prompt) return;

    const sendConvId = active.id;
    const sendGeneration = activeGenerationRef.current;
    const operationId = sendingOperationRef.current + 1;
    sendingOperationRef.current = operationId;
    sendingConversationIdRef.current = sendConvId;
    setSending(true);
    setSendingConversationId(sendConvId);
    if (clearDraft) setDraft('');
    const turnGuess = messages.reduce((max, m) => Math.max(max, m.turn), 0) + 1;
    const localUserId = `local-user-${Date.now()}`;
    setMessages((prev) => [
      ...prev,
      {
        id: localUserId,
        conversationId: sendConvId,
        turn: turnGuess,
        role: 'user',
        content: prompt,
        status: 'ok',
        durationMs: 0,
        createdAt: new Date().toISOString(),
      },
    ]);

    // A runtime-enabled snapshot is the sole decision point.  Failure to read
    // it is surfaced and never silently changes a new Codex chat to legacy.
    runtimeProbeRef.current.add(sendConvId);
    const transport = await readRuntimeTransport(() =>
      enqueueRuntimeSnapshot(
        sendConvId,
        () => runtimeSnapshot(sendConvId),
        (snapshot, sourceVersion) => {
          applyRuntimeSnapshot(
            snapshot,
            sendConvId,
            sendGeneration,
            sourceVersion,
            false,
          );
        },
      ),
    );
    runtimeProbeRef.current.delete(sendConvId);
    if (transport.kind === 'unavailable') {
      runtimeProbeCancelRef.current.delete(sendConvId);
      const e = new Error('无法连接聊天服务');
      if (isCurrentChatRequest(activeIdRef.current, activeGenerationRef.current, sendConvId, sendGeneration)) {
        toast({ title: e.message, variant: 'danger' });
        setMessages((prev) => prev.filter((message) => message.id !== localUserId));
        setDraft(prompt);
      }
      clearSendingFor(sendConvId);
      return;
    }
    if (runtimeProbeCancelRef.current.delete(sendConvId)) {
      // The request was cancelled while the transport decision was pending.
      // No new runtime turn or legacy process has been started yet.
      clearSendingFor(sendConvId);
      if (isCurrentChatRequest(activeIdRef.current, activeGenerationRef.current, sendConvId, sendGeneration)) {
        setDraft(prompt);
        setMessages((prev) => prev.filter((message) => message.id !== localUserId));
      }
      return;
    }
    if (transport.kind === 'runtime') {
      beginRuntimeStart(
        runtimeRecordsRef.current,
        sendConvId,
        runtimeSequence(sendConvId),
      );
      try {
        await enqueueRuntimeSnapshot(
          sendConvId,
          () => runtimeStart(sendConvId, prompt, crypto.randomUUID()),
          async (nextSnapshot, sourceVersion) => {
            const currentStartRecord = runtimeRecordsRef.current.get(sendConvId);
            if (currentStartRecord?.cancelRequested && nextSnapshot.runId && isRuntimeActive(nextSnapshot.phase)) {
              await runtimeCancel(sendConvId, nextSnapshot.runId);
            }
            applyRuntimeSnapshot(
              nextSnapshot,
              sendConvId,
              sendGeneration,
              sourceVersion,
              true,
            );
          },
        );
      } catch (e) {
        const current = isCurrentChatRequest(
          activeIdRef.current,
          activeGenerationRef.current,
          sendConvId,
          sendGeneration,
        );
        if (current) {
          toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
          setDraft(prompt);
        }
        const rows = await loadMessages(sendConvId).catch(() => null);
        if (rows && isCurrentChatRequest(activeIdRef.current, activeGenerationRef.current, sendConvId, sendGeneration)) {
          setMessages(rows);
        }
        const recovered = await enqueueRuntimeSnapshot(
          sendConvId,
          () => runtimeSnapshot(sendConvId, runtimeSequence(sendConvId)),
          (nextSnapshot, sourceVersion) => {
            applyRuntimeSnapshot(
              nextSnapshot,
              sendConvId,
              sendGeneration,
              sourceVersion,
              true,
            );
          },
        ).catch(() => null);
        if (!recovered && sendingOperationRef.current === operationId) {
          runtimeRecordsRef.current.delete(sendConvId);
          clearSendingFor(sendConvId);
        }
      }
      return;
    }

    try {
      await chatSend(sendConvId, prompt, (ev) => applyEvent(ev, sendConvId, sendGeneration));
      // Events from the original generation are deliberately ignored after
      // A → B → A. If A is current again when the send finishes, use the
      // current generation for a fresh DB convergence read so the final
      // persisted reply/running state cannot be lost with the old stream.
      if (activeIdRef.current !== sendConvId) return;
      const refreshGeneration = activeGenerationRef.current;
      const convs = await listConversations();
      if (
        !isCurrentChatRequest(
          activeIdRef.current,
          activeGenerationRef.current,
          sendConvId,
          refreshGeneration,
        )
      ) {
        return;
      }
      setConversations(convs);
      const rows = await listChatMessages(sendConvId);
      if (
        isCurrentChatRequest(
          activeIdRef.current,
          activeGenerationRef.current,
          sendConvId,
          refreshGeneration,
        )
      ) {
        setMessages(rows);
      }
    } catch (e) {
      const current = isCurrentChatRequest(
        activeIdRef.current,
        activeGenerationRef.current,
        sendConvId,
        sendGeneration,
      );
      if (current) {
        toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
        const refreshGeneration = activeGenerationRef.current;
        const rows = await loadMessages(sendConvId).catch(() => null);
        if (
          rows &&
          isCurrentChatRequest(
            activeIdRef.current,
            activeGenerationRef.current,
            sendConvId,
            refreshGeneration,
          )
        ) {
          setMessages(rows);
        }
      }
    } finally {
      if (sendingOperationRef.current === operationId && sendingConversationIdRef.current === sendConvId) {
        clearSendingFor(sendConvId);
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

  async function cancelRuntimeTarget(conversationId: string): Promise<'pending' | 'requested' | 'none'> {
    if (runtimeProbeRef.current.has(conversationId)) {
      runtimeProbeCancelRef.current.add(conversationId);
      return 'pending';
    }
    let target = requestRuntimeCancel(runtimeRecordsRef.current, conversationId);
    // A server-owned run may have been restored from the conversation list
    // before this hook has read its runtime snapshot. Resolve that state once
    // before falling back to the legacy cancellation command.
    if (target.kind === 'legacy') {
      const transport = await readRuntimeTransport(() =>
        enqueueRuntimeSnapshot(
          conversationId,
          () => runtimeSnapshot(conversationId),
          (snapshot, sourceVersion) => {
            applyRuntimeSnapshot(
              snapshot,
              conversationId,
              activeGenerationRef.current,
              sourceVersion,
              false,
            );
          },
        ),
      );
      if (transport.kind === 'runtime') {
        target = requestRuntimeCancel(runtimeRecordsRef.current, conversationId);
      }
    }
    if (target.kind === 'pending') return 'pending';
    if (target.kind === 'none') return 'none';
    if (target.kind === 'runtime') {
      await runtimeCancel(conversationId, target.runId);
      return 'requested';
    }
    await chatCancel(conversationId);
    return 'requested';
  }

  async function handleCancel() {
    if (!sendingConversationId || canceling) return;
    setCanceling(true);
    try {
      await cancelRuntimeTarget(sendingConversationId);
      toast({
        title: t('chat.toast.cancelRequested'),
        description: t('chat.toast.cancelRequestedDesc'),
        variant: 'success',
        duration: 4000,
      });
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    } finally {
      setCanceling(false);
    }
  }

  function adoptInflight(id: string | null) {
    if (!id) return;
    sendingConversationIdRef.current = id;
    setSendingConversationId(id);
    setCanceling(false);
    setSending(true);
  }

  async function cancelIfSending(id: string) {
    if (sendingConversationId !== id) return;
    await cancelRuntimeTarget(id).catch(() => {});
    clearSendingFor(id);
  }

  const sendingHere = Boolean(sending && sendingConversationId === active?.id);
  const cancelingHere = Boolean(canceling && sendingConversationId === active?.id);

  async function submitRuntimeRequest(request: RuntimeRequest, decision?: 'allow' | 'deny', answers?: Record<string, string[]>) {
    if (!active || !requestMatchesRuntime(request, runtimeIdRef.current)) return;
    await runtimeReply({ conversationId: active.id, runId: request.runId, requestId: request.id, clientRequestId: crypto.randomUUID(), decision, answers });
  }

  async function steerRuntime(prompt: string) {
    if (!active || !runtime?.enabled || !runtimeIdRef.current || !prompt.trim()) return;
    try {
      await runtimeSteer(active.id, runtimeIdRef.current, prompt.trim(), crypto.randomUUID());
    } catch (error) {
      toast({ title: error instanceof Error ? error.message : String(error), variant: 'danger' });
      throw error;
    }
  }

  return {
    sending,
    sendingHere,
    cancelingHere,
    sendingConversationId: liveSendingConversationId,
    processMap,
    blockers,
    retry,
    handleSend,
    retryLast,
    handleCancel,
    adoptInflight,
    cancelIfSending,
    runtime,
    submitRuntimeRequest,
    steerRuntime,
  };
}
