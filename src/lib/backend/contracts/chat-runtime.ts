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

export interface RuntimePermissionOption {
  id: string;
  kind: string;
}

export interface RuntimeFileChange {
  path: string;
  kind?: string;
  /** Protocol-provided snippet. Missing when the payload only named a path. */
  preview?: string;
}

export interface RuntimeRequest {
  id: string;
  runId: string;
  kind: 'command' | 'file' | 'question';
  title: string;
  detail: string;
  questions: RuntimeQuestion[];
  permissionOptions?: RuntimePermissionOption[];
  fileChanges?: RuntimeFileChange[];
}

export type RuntimeDecision = 'allow' | 'deny' | 'allow_always';

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
  decision?: RuntimeDecision;
  answers?: Record<string, string[]>;
}


export interface RuntimeTurnSettings {
  model?: string | null;
  effort?: string | null;
}

export interface RuntimeModelOption {
  id: string;
  efforts: string[];
  defaultEffort?: string | null;
}

export interface RuntimeLocalImage {
  path: string;
}

export interface RuntimeSkillRef {
  name: string;
  path: string;
}

export type RuntimeExtensionKind = 'skill' | 'plugin';

export interface RuntimeExtensionItem {
  id: string;
  name: string;
  kind: RuntimeExtensionKind;
  installed: boolean;
  enabled: boolean;
  loaded: boolean;
  callable: boolean;
  path?: string | null;
}

export interface RuntimeStartExtras {
  images?: RuntimeLocalImage[];
  skills?: RuntimeSkillRef[];
}

export interface RuntimeOptions {
  conversationId: string;
  settings: RuntimeTurnSettings;
  settingsFrozen: boolean;
  models: RuntimeModelOption[];
  extensions: RuntimeExtensionItem[];
  modelsFromCodex: boolean;
  imageInput?: boolean;
  steer?: boolean;
}
