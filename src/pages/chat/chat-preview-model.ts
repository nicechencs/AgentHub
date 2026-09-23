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
  /** Transcript step this detail is showing. Absent on older targets. */
  stepKey?: string;
};

export type ChatEditPreviewTarget = {
  kind: 'edit';
  path: string;
  /** Clicked turn. A later turn that touched the same path must not replace this diff. */
  turn?: number;
};

export type ChatInspectTarget =
  | ChatFilePreviewTarget
  | ChatProcessInspectTarget
  | ChatEditPreviewTarget;

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

export function isChatEditPreview(
  target: ChatInspectTarget | null | undefined,
): target is ChatEditPreviewTarget {
  return target?.kind === 'edit';
}

export function chatPreviewPath(target: ChatInspectTarget | null | undefined): string {
  if (isChatEditPreview(target)) return target.path;
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

export function openChatProcessInspect(
  turn: number,
  agent: AgentKey,
  stepKey?: string,
): ChatProcessInspectTarget {
  return stepKey
    ? { kind: 'process', turn, agent, stepKey }
    : { kind: 'process', turn, agent };
}

/** Transcript row for one thinking / tool / error step. Shared with the detail pane. */
export function processInspectStepKey(index: number): string {
  return `step:${index}`;
}

/** Placeholder row shown before the first process step arrives. */
export const PROCESS_INSPECT_GENERATING_KEY = 'generating';

/** Index encoded by `processInspectStepKey`, or null for the generating row. */
export function processInspectStepIndex(stepKey: string | null | undefined): number | null {
  if (!stepKey?.startsWith('step:')) return null;
  const raw = stepKey.slice('step:'.length);
  if (!/^\d+$/.test(raw)) return null;
  return Number(raw);
}

/**
 * The open detail stays up while the user moves to another step.
 * Clicking the step that is already showing closes it.
 */
export function processInspectRowAction(
  paneOpen: boolean,
  selectedStepKey: string | null | undefined,
  stepKey: string,
): 'close' | 'focus' {
  return paneOpen && selectedStepKey === stepKey ? 'close' : 'focus';
}

export function openChatEditPreview(path: string, turn?: number): ChatEditPreviewTarget {
  return typeof turn === 'number' ? { kind: 'edit', path, turn } : { kind: 'edit', path };
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
