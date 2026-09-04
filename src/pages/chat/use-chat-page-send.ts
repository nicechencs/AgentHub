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
} from '@/lib/api/chat';
import { processKey, reduceProcessEvent, type ProcessMap } from '@/lib/chat-process';
import type { AgentKey, ChatEvent, ChatMessage, Conversation } from '@/lib/types';
import type { TurnGroup } from './chat-format';
import { conversationTitle, retryTarget, sendBlockers } from './chat-model';
import { isCurrentChatRequest } from './chat-request';

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
  const streamingRef = useRef<Record<string, string>>({});
  const [processMap, setProcessMap] = useState<ProcessMap>({});

  useEffect(() => {
    setProcessMap({});
    streamingRef.current = {};
  }, [activeId]);

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

  function applyEvent(ev: ChatEvent, sendConvId: string, sendGeneration: number) {
    if (
      !isCurrentChatRequest(
        activeIdRef.current,
        activeGenerationRef.current,
        sendConvId,
        sendGeneration,
      )
    ) {
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
      toast({ title: e instanceof Error ? e.message : String(e), variant: 'danger' });
      if (activeIdRef.current === sendConvId) {
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
      if (sendingConversationIdRef.current === sendConvId) {
        sendingConversationIdRef.current = null;
        setSending(false);
        setCanceling(false);
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
    if (!sendingConversationId || canceling) return;
    setCanceling(true);
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
    await chatCancel(id).catch(() => {});
    sendingConversationIdRef.current = null;
    setSending(false);
    setCanceling(false);
    setSendingConversationId(null);
  }

  const sendingHere = Boolean(sending && sendingConversationId === active?.id);
  const cancelingHere = Boolean(canceling && sendingConversationId === active?.id);

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
  };
}
