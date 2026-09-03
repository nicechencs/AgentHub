import type { AdapterSourceKind, ForkedConnectionAuthorization } from '@/lib/backend/contracts/adapter';
import { parsePriorityInput } from './api-access-model';

export type PoolOauthEditItem = {
  kind: string;
  sourceKind: AdapterSourceKind;
  sourceId: string;
  addedHere: boolean;
  priority?: number | null;
};

export function poolAuthorizationOauthEditable(item: {
  kind: string;
  sourceKind?: string;
}): boolean {
  return item.kind === 'oauth' && item.sourceKind === 'account';
}

export function poolAuthorizationCopyOnSave(item: Pick<PoolOauthEditItem, 'kind' | 'addedHere'>): boolean {
  return item.kind === 'oauth' && item.addedHere === false;
}

export type SaveOauthPoolLoginDeps = {
  forkConnectionAuthorization: (
    sourceKind: AdapterSourceKind,
    sourceId: string,
  ) => Promise<ForkedConnectionAuthorization>;
  setSourceCustomModels: (
    sourceKind: AdapterSourceKind,
    sourceId: string,
    models: string[],
  ) => Promise<unknown>;
  setRouteAuthorizationPriority: (
    sourceKind: AdapterSourceKind,
    sourceId: string,
    priority: number,
  ) => Promise<number>;
};

export type SaveOauthPoolLoginResult = ForkedConnectionAuthorization & {
  models: string[];
};

/** Copy a Connections-shared official login if needed, then save models and priority. */
export async function saveOauthPoolLogin(
  input: {
    item: PoolOauthEditItem;
    models: readonly string[];
    priority: string;
  },
  deps: SaveOauthPoolLoginDeps,
): Promise<SaveOauthPoolLoginResult> {
  const models = [...new Set(input.models.map((model) => model.trim()).filter(Boolean))];
  const priority = parsePriorityInput(input.priority);
  let forked: ForkedConnectionAuthorization = {
    sourceKind: input.item.sourceKind,
    sourceId: input.item.sourceId,
    originalSourceId: input.item.sourceId,
    copied: false,
  };
  if (poolAuthorizationCopyOnSave(input.item)) {
    forked = await deps.forkConnectionAuthorization(input.item.sourceKind, input.item.sourceId);
  }
  await deps.setSourceCustomModels(forked.sourceKind, forked.sourceId, models);
  if (priority !== null) {
    await deps.setRouteAuthorizationPriority(forked.sourceKind, forked.sourceId, priority);
  }
  return { ...forked, models };
}
