import type { ChatBootstrap } from '@/lib/types';

/** Last path segment for a folder chosen in the OS file manager. */
export function folderNameFromCwd(cwd: string): string {
  const trimmed = cwd.trim().replace(/[\\/]+$/, '');
  if (!trimmed) return '';
  const parts = trimmed.split(/[\\/]/).filter((part) => part && part !== '.');
  return parts[parts.length - 1] || trimmed;
}

export function shellOpenChatHref(now = Date.now()): string {
  return `/chat?from=shell&t=${now}`;
}

export function shellOpenChatBootstrap(cwd: string): ChatBootstrap {
  const title = folderNameFromCwd(cwd);
  return { agentIds: [], cwd, title: title || undefined };
}

/** Take pending cwd once, then write bootstrap and navigate. Event payloads are not a source. */
export async function consumePendingOpenChatCwd(input: {
  takePending: () => Promise<string | null>;
  applyBootstrap: (payload: ChatBootstrap) => boolean;
  navigate: (to: string) => void;
  now?: number;
}): Promise<boolean> {
  const cwd = await input.takePending();
  if (!cwd) return false;
  if (!input.applyBootstrap(shellOpenChatBootstrap(cwd))) return false;
  input.navigate(shellOpenChatHref(input.now ?? Date.now()));
  return true;
}
