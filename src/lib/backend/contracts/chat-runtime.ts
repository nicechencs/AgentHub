import type { ChatEvent, ChatMessage } from '@/lib/types';

export type RuntimePhase =
  | 'idle' | 'starting' | 'running' | 'waiting' | 'cancelling'
  | 'completed' | 'failed' | 'cancelled' | 'interrupted';

export interface RuntimeQuestion {
  id: string;
  header: string;
  question: string;
  options: Array<{ label: string; description: string }>;
  isOther: boolean;
  isSecret: boolean;
}

export interface RuntimeRequest {
  id: string;
  runId: string;
  kind: 'command' | 'file' | 'question';
  title: string;
  detail: string;
  questions: RuntimeQuestion[];
}

export interface RuntimeEvent { sequence: number; event: ChatEvent }

export interface RuntimeSnapshot {
  conversationId: string;
  enabled: boolean;
  runId: string | null;
  phase: RuntimePhase;
  lastSequence: number;
  events: RuntimeEvent[];
  pendingRequests: RuntimeRequest[];
  gap: boolean;
  /** Full current agent message read in the same durable snapshot transaction. */
  currentMessage?: ChatMessage | null;
}

export interface RuntimeReply {
  conversationId: string;
  runId: string;
  requestId: string;
  clientRequestId: string;
  decision?: 'allow' | 'deny';
  answers?: Record<string, string[]>;
}
