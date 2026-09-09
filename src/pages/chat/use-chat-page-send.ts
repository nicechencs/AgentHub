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
  runtimeContinueLegacy,
  runtimeReply,
  runtimeSnapshot,
  runtimeStart,
  runtimeSteer,
} from '@/lib/api/chat';
import type { RuntimeRequest, RuntimeSnapshot } from '@/lib/api/chat';
import type { ProcessMap } from '@/lib/chat-process';
import type { AgentKey, ChatEvent, ChatMessage, Conversation } from '@/lib/types';
import type { TurnGroup } from './chat-format';
import { busyAgentsForSends, incomingSendingIds, liveSendingIds, retryTarget, sendBlockers } from './chat-model';
import { isCurrentChatRequest } from './chat-request';
import {
  appendQueuedFollowUp,
  grokShouldFlushFollowUp,
  prependQueuedFollowUp,
  queuedFollowUpLabel,
  restoreQueuedFollowUpOnCancel,
  shiftQueuedFollowUp,
} from './chat-grok-follow-up';
import { acceptsRuntimeSnapshot, isLatestRuntimeRead, isRuntimeActive, readRuntimeTransport, requestMatchesRuntime, runtimeReplyFields } from './chat-runtime-model';
import {
  beginRuntimeStart,
  acceptRuntimeSnapshotVersion,
  advanceRuntimeWatermark,
  enqueueRuntimeSnapshot as enqueueRuntimeSnapshotSource,
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

function titleFromPrompt(prompt: string): string {
  const trimmed = prompt.trim();
  return trimmed.length > 30 ? `${trimmed.slice(0, 30)}…` : trimmed;
}

/**
 * Chat 发送 / 取消 / 流式事件 / 过程面板。
 * 世代判定仍走 isCurrentChatRequest。发送按会话隔离，允许多个会话同时进行。
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
  getStartExtras?: () => { images?: { path: string }[]; skills?: { name: string; path: string }[] };
  clearStartExtras?: () => void;
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
    getStartExtras,
    clearStartExtras,
  } = input;

  const sendingIdsRef = useRef(new Set<string>());
  const [sendingIds, setSendingIds] = useState<string[]>([]);
  const cancelingIdsRef = useRef(new Set<string>());
  const [cancelingIds, setCancelingIds] = useState<string[]>([]);
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
  const followUpsRef = useRef(new Map<string, string[]>());
  const [followUpById, setFollowUpById] = useState<Record<string, { label: string; count: number }>>({});

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

  const publishFollowUps = () => {
    const next: Record<string, { label: string; count: number }> = {};
    for (const [id, items] of followUpsRef.current.entries()) {
      const label = queuedFollowUpLabel(items);
      if (!label) continue;
      next[id] = {
        label,
        count: items.map((item) => item.trim()).filter(Boolean).length,
      };
    }
    setFollowUpById(next);
  };

  const setFollowUpQueue = (conversationId: string, items: string[]) => {
    const next = items.map((item) => item.trim()).filter(Boolean);
    if (next.length > 0) followUpsRef.current.set(conversationId, next);
    else followUpsRef.current.delete(conversationId);
    publishFollowUps();
  };

  const appendFollowUp = (conversationId: string, prompt: string) => {
    setFollowUpQueue(
      conversationId,
      appendQueuedFollowUp(followUpsRef.current.get(conversationId) ?? [], prompt),
    );
  };

  const prependFollowUp = (conversationId: string, prompt: string) => {
    setFollowUpQueue(
      conversationId,
      prependQueuedFollowUp(followUpsRef.current.get(conversationId) ?? [], prompt),
    );
  };

  const dequeueFollowUp = (conversationId: string): string | null => {
    const shifted = shiftQueuedFollowUp(followUpsRef.current.get(conversationId) ?? []);
    if (!shifted) return null;
    setFollowUpQueue(conversationId, shifted.rest);
    return shifted.next;
  };

  const clearFollowUp = (conversationId: string) => {
    setFollowUpQueue(conversationId, []);
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

  const publishSendingIds = () => {
    setSendingIds([...sendingIdsRef.current]);
  };

  const markSending = (conversationId: string) => {
    if (sendingIdsRef.current.has(conversationId)) return;
    sendingIdsRef.current.add(conversationId);
    publishSendingIds();
  };

  const clearSendingFor = (conversationId: string) => {
    if (!sendingIdsRef.current.has(conversationId)) return;
    sendingIdsRef.current.delete(conversationId);
    if (cancelingIdsRef.current.delete(conversationId)) {
      setCancelingIds([...cancelingIdsRef.current]);
    }
    publishSendingIds();
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
    const previousPhase = runtimeRecordsRef.current.get(conversationId)?.phase;
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
    const activePhase = isRuntimeActive(snapshot.phase);
    const wasSending = sendingIdsRef.current.has(conversationId);
    if (activePhase) {
      markSending(conversationId);
    } else if (canRender || (previousPhase != null && isRuntimeActive(previousPhase))) {
      clearSendingFor(conversationId);
    }
    if (
      !activePhase &&
      wasSending &&
      grokShouldFlushFollowUp(previousPhase, snapshot.phase)
    ) {
      const queued = dequeueFollowUp(conversationId);
      if (queued) void dispatchQueuedFollowUp(conversationId, queued);
    }
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
    if (!activePhase && wasSending) {
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
  };

  const activeAgentId = active?.agentIds[0] ?? null;

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
  }, [activeId, activeAgentId, runtime?.enabled, runtime?.phase]);

  // Background runs stay owned by their conversations. Poll them so a terminal
  // snapshot can release that session without a page-wide sending lock.
  useEffect(() => {
    const ids = sendingIds.filter((id) => id !== activeId);
    if (ids.length === 0) return;
    let disposed = false;
    const inFlight = new Set<string>();
    const read = async (id: string) => {
      if (inFlight.has(id)) return;
      inFlight.add(id);
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
          },
        );
      } catch {
        // The active session will surface a current error when it is revisited.
      } finally {
        inFlight.delete(id);
      }
    };
    const tick = () => {
      if (disposed) return;
      for (const id of ids) void read(id);
    };
    tick();
    const timer = window.setInterval(tick, 400);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [activeId, sendingIds]);

  const liveSendingConversationIds = useMemo(
    () => liveSendingIds(sendingIds, conversations),
    [conversations, sendingIds],
  );
  const busyAgentIds = useMemo(
    () => busyAgentsForSends(conversations, liveSendingConversationIds),
    [conversations, liveSendingConversationIds],
  );
  const sendingHere = Boolean(active?.id && liveSendingConversationIds.includes(active.id));
  const cancelingHere = Boolean(active?.id && cancelingIds.includes(active.id));

  const blockers = useMemo(() => {
    if (!active) return [];
    return sendBlockers({
      conversation: active,
      hiddenIds,
      envNotReadyIds,
      unconfiguredAuthIds,
      agentsReady,
    });
  }, [active, hiddenIds, envNotReadyIds, unconfiguredAuthIds, agentsReady]);

  const retry = useMemo(() => retryTarget(turns, sendingHere), [turns, sendingHere]);

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

  async function dispatchQueuedFollowUp(conversationId: string, prompt: string) {
    const text = prompt.trim();
    if (!text) return;
    if (activeIdRef.current === conversationId && !sendingIdsRef.current.has(conversationId)) {
      await sendPrompt(text, false);
      return;
    }
    markSending(conversationId);
    const generation = activeGenerationRef.current;
    const applyUi = conversationId === activeIdRef.current;
    const fail = (error: unknown) => {
      clearSendingFor(conversationId);
      prependFollowUp(conversationId, text);
      toast({
        title: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    };
    const transport = await readRuntimeTransport(() =>
      enqueueRuntimeSnapshot(
        conversationId,
        () => runtimeSnapshot(conversationId, runtimeSequence(conversationId)),
        (snapshot, sourceVersion) => {
          applyRuntimeSnapshot(snapshot, conversationId, generation, sourceVersion, false);
        },
      ),
    );
    if (transport.kind === 'unavailable') {
      fail(new Error('无法连接聊天服务'));
      return;
    }
    if (transport.kind === 'runtime') {
      try {
        await enqueueRuntimeSnapshot(
          conversationId,
          () => runtimeStart(conversationId, text, crypto.randomUUID()),
          (snapshot, sourceVersion) => {
            applyRuntimeSnapshot(
              snapshot,
              conversationId,
              generation,
              sourceVersion,
              applyUi,
            );
          },
        );
      } catch (error) {
        fail(error);
      }
      return;
    }
    try {
      await chatSend(conversationId, text, (ev) => applyEvent(ev, conversationId, generation, applyUi));
    } catch (error) {
      fail(error);
    } finally {
      if (sendingIdsRef.current.has(conversationId)) {
        clearSendingFor(conversationId);
        const queued = dequeueFollowUp(conversationId);
        if (queued) void dispatchQueuedFollowUp(conversationId, queued);
      }
    }
  }

  async function sendPrompt(prompt: string, clearDraft: boolean) {
    if (!active) return;
    if (sendingIdsRef.current.has(active.id)) {
      const next = prompt.trim();
      if (!next) return;
      appendFollowUp(active.id, next);
      if (clearDraft) setDraft('');
      toast({
        title: t('chat.toast.queuedAfterTurn'),
        variant: 'success',
        duration: 2500,
      });
      return;
    }
    if (sendBlockers({
      conversation: active,
      hiddenIds,
      envNotReadyIds,
      unconfiguredAuthIds,
      agentsReady,
    }).length > 0) {
      return;
    }
    if (!prompt) return;

    const sendConvId = active.id;
    const sendGeneration = activeGenerationRef.current;
    markSending(sendConvId);
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
    if (!active.title.trim()) {
      const title = titleFromPrompt(prompt);
      setConversations((prev) => prev.map((item) => (
        item.id === sendConvId ? { ...item, title } : item
      )));
    }

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
          () => runtimeStart(sendConvId, prompt, crypto.randomUUID(), getStartExtras?.()),
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
        clearStartExtras?.();
      } catch (e) {
        const current = isCurrentChatRequest(
          activeIdRef.current,
          activeGenerationRef.current,
          sendConvId,
          sendGeneration,
        );
        const cancelledDuringStart = Boolean(
          runtimeRecordsRef.current.get(sendConvId)?.cancelRequested,
        );
        if (current && !cancelledDuringStart) {
          toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
          setDraft(prompt);
        } else if (cancelledDuringStart && current) {
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
        if (!recovered) {
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
      clearSendingFor(sendConvId);
      const queued = dequeueFollowUp(sendConvId);
      if (queued) void dispatchQueuedFollowUp(sendConvId, queued);
    }
  }

  async function handleSend() {
    await sendPrompt(draft.trim(), true);
  }

  async function retryLast() {
    const target = retryTarget(turns, sendingHere);
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
    if (target.kind === 'pending') {
      const record = runtimeRecordsRef.current.get(conversationId);
      try {
        await runtimeCancel(conversationId, record?.runId ?? '');
      } catch {
        // Worker may not exist yet; local pendingStart already recorded cancel.
      }
      return 'pending';
    }
    if (target.kind === 'none') return 'none';
    if (target.kind === 'runtime') {
      await runtimeCancel(conversationId, target.runId);
      return 'requested';
    }
    await chatCancel(conversationId);
    return 'requested';
  }

  async function handleCancel() {
    const id = active?.id;
    if (!id || !sendingIdsRef.current.has(id) || cancelingIdsRef.current.has(id)) return;
    const restored = restoreQueuedFollowUpOnCancel({
      draft,
      queue: followUpsRef.current.get(id) ?? [],
    });
    if (restored.draft !== draft) setDraft(restored.draft);
    setFollowUpQueue(id, restored.queue);
    cancelingIdsRef.current.add(id);
    setCancelingIds([...cancelingIdsRef.current]);
    try {
      await cancelRuntimeTarget(id);
      toast({
        title: t('chat.toast.cancelRequested'),
        description: t('chat.toast.cancelRequestedDesc'),
        variant: 'success',
        duration: 4000,
      });
    } catch (e) {
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
    } finally {
      if (cancelingIdsRef.current.delete(id)) {
        setCancelingIds([...cancelingIdsRef.current]);
      }
    }
  }

  function adoptInflight(ids?: string[] | string | null) {
    let changed = false;
    for (const id of incomingSendingIds(ids)) {
      if (sendingIdsRef.current.has(id)) continue;
      sendingIdsRef.current.add(id);
      changed = true;
    }
    if (changed) publishSendingIds();
  }

  async function cancelIfSending(id: string) {
    if (!sendingIdsRef.current.has(id)) return;
    await cancelRuntimeTarget(id).catch(() => {});
    clearSendingFor(id);
  }

  async function submitRuntimeRequest(request: RuntimeRequest, decision?: 'allow' | 'deny' | 'allow_always', answers?: Record<string, string[]>) {
    if (!active || !requestMatchesRuntime(request, runtimeIdRef.current)) {
      throw new Error('stale runtime request');
    }
    try {
      await runtimeReply({
        conversationId: active.id,
        runId: request.runId,
        requestId: request.id,
        clientRequestId: crypto.randomUUID(),
        ...runtimeReplyFields(request, decision, answers),
      });
    } catch (error) {
      toast({ title: error instanceof Error ? error.message : String(error), variant: 'danger' });
      throw error;
    }
  }

  async function continueLegacyGrok() {
    if (!active) return;
    try {
      const snapshot = await runtimeContinueLegacy(active.id);
      const sourceVersion = (runtimeSourceVersionRef.current.get(active.id) ?? 0) + 1;
      runtimeSourceVersionRef.current.set(active.id, sourceVersion);
      applyRuntimeSnapshot(
        snapshot,
        active.id,
        activeGenerationRef.current,
        sourceVersion,
        true,
      );
      toast({
        title: t('chat.toast.legacyContinued'),
        variant: 'success',
        duration: 2500,
      });
    } catch (error) {
      toast({
        title: t('chat.toast.legacyContinueFail'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    }
  }

  async function steerRuntime(prompt: string): Promise<boolean> {
    if (!active || !runtime?.enabled || !runtimeIdRef.current || !prompt.trim()) return false;
    try {
      await runtimeSteer(active.id, runtimeIdRef.current, prompt.trim(), crypto.randomUUID());
      toast({ title: t('chat.toast.steered'), variant: 'success', duration: 2500 });
      return true;
    } catch (error) {
      toast({ title: error instanceof Error ? error.message : String(error), variant: 'danger' });
      throw error;
    }
  }

  return {
    sending: sendingHere,
    sendingHere,
    cancelingHere,
    sendingConversationIds: liveSendingConversationIds,
    busyAgentIds,
    processMap,
    blockers,
    retry,
    handleSend,
    retryLast,
    handleCancel,
    queuedFollowUp: activeId ? followUpById[activeId]?.label ?? null : null,
    queuedFollowUpCount: activeId ? followUpById[activeId]?.count ?? 0 : 0,
    clearQueuedFollowUp: () => {
      if (activeId) clearFollowUp(activeId);
    },
    continueLegacyGrok,
    adoptInflight,
    cancelIfSending,
    runtime,
    submitRuntimeRequest,
    steerRuntime,
  };
}
