export type ChatPreviewTarget = {
  stack: string[];
};

export function chatPreviewPath(target: ChatPreviewTarget | null | undefined): string {
  if (!target?.stack.length) return '';
  return target.stack[target.stack.length - 1] ?? '';
}

export function chatPreviewCanBack(target: ChatPreviewTarget | null | undefined): boolean {
  return (target?.stack.length ?? 0) > 1;
}

export function openChatPreviewRoot(path: string): ChatPreviewTarget {
  return { stack: [path] };
}

export function pushChatPreview(
  target: ChatPreviewTarget | null | undefined,
  next: string,
): ChatPreviewTarget {
  const stack = target?.stack ?? [];
  const current = stack[stack.length - 1];
  if (!next || next === current) {
    return { stack: stack.length ? stack : [next] };
  }
  return { stack: [...stack, next] };
}

export function popChatPreview(
  target: ChatPreviewTarget | null | undefined,
): ChatPreviewTarget | null {
  const stack = target?.stack ?? [];
  if (stack.length <= 1) return null;
  return { stack: stack.slice(0, -1) };
}
