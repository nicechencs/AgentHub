import type { AgentId } from '@/lib/types';
import type { ConnectionTrashHome, ConnectionTrashItem } from '@/lib/backend/contracts';
import { getBackend, refreshRuntimeReadModels } from '@/app/runtime';

export async function listConnectionTrash(
  agentId?: AgentId,
  home?: ConnectionTrashHome,
): Promise<ConnectionTrashItem[]> {
  return getBackend().trash.list(agentId, home);
}

export async function restoreConnectionTrash(id: string): Promise<void> {
  await getBackend().trash.restore(id);
  try {
    await refreshRuntimeReadModels(getBackend(), { models: ['connectionInventory'] });
  } catch {
    // Restore succeeded. Refresh errors stay on the pool snapshot.
  }
}

export async function permanentlyDeleteConnectionTrash(id: string): Promise<void> {
  return getBackend().trash.permanentlyDelete(id);
}
