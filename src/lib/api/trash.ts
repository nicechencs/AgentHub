import type { AgentId } from '@/lib/types';
import type { ConnectionTrashItem } from '@/lib/backend/contracts';
import { getBackend } from '@/app/runtime';

export async function listConnectionTrash(agentId?: AgentId): Promise<ConnectionTrashItem[]> {
  return getBackend().trash.list(agentId);
}

export async function restoreConnectionTrash(id: string): Promise<void> {
  return getBackend().trash.restore(id);
}

export async function permanentlyDeleteConnectionTrash(id: string): Promise<void> {
  return getBackend().trash.permanentlyDelete(id);
}
