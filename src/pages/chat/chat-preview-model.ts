import type { AgentKey } from '@/lib/types';

export type ChatFilePreviewTarget = {
  kind: 'file';
  stack: string[];
};

export type ChatProcessInspectTarget = {
  kind: 'process';
  turn: number;
  agent: AgentKey;
};

export type ChatInspectTarget = ChatFilePreviewTarget | ChatProcessInspectTarget;

/** File-or-process inspect target for the chat right pane. */
export type ChatPreviewTarget = ChatInspectTarget;

export function isChatFilePreview(
  target: ChatInspectTarget | null | undefined,
): target is ChatFilePreviewTarget {
  return target?.kind === 'file';
}

export function isChatProcessInspect(
  target: ChatInspectTarget | null | undefined,
): target is ChatProcessInspectTarget {
  return target?.kind === 'process';
}

export function chatPreviewPath(target: ChatInspectTarget | null | undefined): string {
  if (!isChatFilePreview(target) || !target.stack.length) return '';
  return target.stack[target.stack.length - 1] ?? '';
}

export function chatPreviewCanBack(target: ChatInspectTarget | null | undefined): boolean {
  return isChatFilePreview(target) && target.stack.length > 1;
}

export function openChatPreviewRoot(path: string): ChatFilePreviewTarget {
  return { kind: 'file', stack: [path] };
}

export function openChatProcessInspect(turn: number, agent: AgentKey): ChatProcessInspectTarget {
  return { kind: 'process', turn, agent };
}

export function pushChatPreview(
  target: ChatInspectTarget | null | undefined,
  next: string,
): ChatFilePreviewTarget {
  if (!isChatFilePreview(target)) {
    return openChatPreviewRoot(next);
  }
  const stack = target.stack;
  const current = stack[stack.length - 1];
  if (!next || next === current) {
    return { kind: 'file', stack: stack.length ? stack : [next] };
  }
  return { kind: 'file', stack: [...stack, next] };
}

export function popChatPreview(
  target: ChatInspectTarget | null | undefined,
): ChatFilePreviewTarget | null {
  if (!isChatFilePreview(target)) return null;
  const stack = target.stack;
  if (stack.length <= 1) return null;
  return { kind: 'file', stack: stack.slice(0, -1) };
}
