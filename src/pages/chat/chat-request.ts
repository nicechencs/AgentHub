import type { Conversation } from '@/lib/types';

/** A send continuation may only write to the still-current conversation. */
export function isCurrentChatRequest(
  activeId: string | null,
  activeGeneration: number,
  requestId: string,
  requestGeneration: number,
): boolean {
  return activeId === requestId && activeGeneration === requestGeneration;
}

/** The list and its initial selection are one generation-checked commit. */
export function conversationListState(conversations: Conversation[]): {
  conversations: Conversation[];
  activeId: string | null;
} {
  return {
    conversations,
    activeId: conversations[0]?.id ?? null,
  };
}

/** Keep initialization idempotent across StrictMode effect replays. */
export function createSingleFlight<T>() {
  let inFlight: Promise<T> | null = null;

  return (factory: () => Promise<T>): Promise<T> => {
    if (inFlight) return inFlight;
    const next = factory();
    inFlight = next;
    next.then(
      () => {
        if (inFlight === next) inFlight = null;
      },
      () => {
        if (inFlight === next) inFlight = null;
      },
    );
    return next;
  };
}
