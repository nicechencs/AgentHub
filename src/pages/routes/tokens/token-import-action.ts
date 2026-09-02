/**
 * One-click import of a local-route token into an Agent.
 * Reuses the「写入 Agent」path: plan/bind ticket → switchProvider on the
 * generated local-entry provider (URL + ahb_ key). No second secret store.
 */
import { listAdapterProfiles } from '@/lib/api/adapter';
import { switchProvider } from '@/lib/api/provider';
import { logGuiEvent } from '@/lib/api/settings';
import { bindTicket, planTicket, ticketIdFor } from '@/lib/api/tickets';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { AgentId } from '@/lib/types';
import { switchWriteLast4 } from '@/pages/bridges/client-config-model';
import { localBridgeSiblingForTarget } from '@/pages/bridges/route-graph-model';

export type ImportLocalTokenDeps = {
  planTicket: typeof planTicket;
  bindTicket: typeof bindTicket;
  listAdapterProfiles: typeof listAdapterProfiles;
  switchProvider: typeof switchProvider;
  logGuiEvent: typeof logGuiEvent;
};

const defaultDeps: ImportLocalTokenDeps = {
  planTicket,
  bindTicket,
  listAdapterProfiles,
  switchProvider,
  logGuiEvent,
};

export async function importLocalTokenToAgent(
  input: {
    profile: Pick<AdapterProfile, 'sourceKind' | 'sourceId' | 'targetAgentId' | 'generatedProviderId' | 'route'>;
    agentId: AgentId;
    localToken?: string | null;
    /** Pre-bind siblings; refreshed profiles are preferred after bind. */
    siblingProfiles?: readonly AdapterProfile[];
  },
  deps: ImportLocalTokenDeps = defaultDeps,
): Promise<void> {
  const ticketId = ticketIdFor(input.profile.sourceKind, input.profile.sourceId);
  await deps.planTicket(ticketId, input.agentId);
  const { binding } = await deps.bindTicket(ticketId, input.agentId);

  let generated: string | null = null;
  try {
    const profiles = await deps.listAdapterProfiles();
    const fromBinding = binding.profileId
      ? profiles.find((row) => row.id === binding.profileId)
      : undefined;
    const sibling = localBridgeSiblingForTarget(profiles, input.profile, input.agentId)
      ?? (input.profile.targetAgentId === input.agentId
        ? profiles.find((row) => (
          row.sourceKind === input.profile.sourceKind
          && row.sourceId === input.profile.sourceId
          && row.targetAgentId === input.agentId
        ))
        : undefined);
    generated = fromBinding?.generatedProviderId?.trim()
      || sibling?.generatedProviderId?.trim()
      || null;
  } catch {
    const fallback = localBridgeSiblingForTarget(
      input.siblingProfiles ?? [],
      input.profile,
      input.agentId,
    );
    generated = fallback?.generatedProviderId?.trim()
      || (input.profile.targetAgentId === input.agentId
        ? input.profile.generatedProviderId?.trim() || null
        : null);
  }

  if (!generated) {
    throw new Error('已接上，但找不到写入目标工具的本机地址');
  }

  await deps.switchProvider(input.agentId, generated);
  void deps.logGuiEvent('switch_write', {
    agent: input.agentId,
    last4: switchWriteLast4(input.localToken),
  });
}
