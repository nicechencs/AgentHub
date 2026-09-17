import type { AgentKey } from '@/lib/types';

export type ChatFilePreviewTarget = {
  kind: 'file';
  stack: string[];
  /** 1-based line on the top-of-stack file (Deepseek Harness openFile.line). */
  line?: number;
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

export function chatPreviewLine(target: ChatInspectTarget | null | undefined): number | undefined {
  if (!isChatFilePreview(target)) return undefined;
  const line = target.line;
  return typeof line === 'number' && line > 0 ? line : undefined;
}

export function chatPreviewCanBack(target: ChatInspectTarget | null | undefined): boolean {
  return isChatFilePreview(target) && target.stack.length > 1;
}

export function openChatPreviewRoot(path: string, line?: number): ChatFilePreviewTarget {
  return {
    kind: 'file',
    stack: [path],
    ...(typeof line === 'number' && line > 0 ? { line } : {}),
  };
}

export function openChatProcessInspect(turn: number, agent: AgentKey): ChatProcessInspectTarget {
  return { kind: 'process', turn, agent };
}

export function pushChatPreview(
  target: ChatInspectTarget | null | undefined,
  next: string,
  line?: number,
): ChatFilePreviewTarget {
  if (!isChatFilePreview(target)) {
    return openChatPreviewRoot(next, line);
  }
  const stack = target.stack;
  const current = stack[stack.length - 1];
  if (!next || next === current) {
    const samePath = { kind: 'file' as const, stack: stack.length ? stack : [next] };
    if (typeof line === 'number' && line > 0) return { ...samePath, line };
    if (target.line != null) return { ...samePath, line: target.line };
    return samePath;
  }
  return openChatPreviewRootFromStack([...stack, next], line);
}

function openChatPreviewRootFromStack(stack: string[], line?: number): ChatFilePreviewTarget {
  return {
    kind: 'file',
    stack,
    ...(typeof line === 'number' && line > 0 ? { line } : {}),
  };
}

export function popChatPreview(
  target: ChatInspectTarget | null | undefined,
): ChatFilePreviewTarget | null {
  if (!isChatFilePreview(target)) return null;
  const stack = target.stack;
  if (stack.length <= 1) return null;
  return { kind: 'file', stack: stack.slice(0, -1) };
}
