/**
 * One-click import of a local-route token into an Agent.
 * Reuses the「写入 Agent」path: switchProvider on the generated local-entry
 * provider (URL + ahb_ key). Bind only when that provider is not there yet.
 */
import { listAdapterProfiles } from '@/lib/api/adapter';
import { switchProvider } from '@/lib/api/provider';
import { logGuiEvent } from '@/lib/api/settings';
import { bindTicket, planTicket, ticketIdFor } from '@/lib/api/tickets';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { AgentKey } from '@/lib/types';
import { switchWriteLast4 } from '@/pages/routes/shared/client-config-model';
import { localBridgeSiblingForTarget } from '@/pages/routes/shared/route-graph-model';

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

type ImportProfile = Pick<
  AdapterProfile,
  'sourceKind' | 'sourceId' | 'targetAgentId' | 'generatedProviderId' | 'route'
>;

function generatedForTarget(
  profiles: readonly ImportProfile[],
  source: ImportProfile,
  agentId: AgentKey,
): string | null {
  const sibling = localBridgeSiblingForTarget(profiles, source, agentId)
    ?? (source.targetAgentId === agentId
      ? profiles.find((row) => (
        row.sourceKind === source.sourceKind
        && row.sourceId === source.sourceId
        && row.targetAgentId === agentId
      ))
      : undefined);
  const fromSibling = sibling?.generatedProviderId?.trim();
  if (fromSibling) return fromSibling;
  if (source.targetAgentId === agentId) {
    return source.generatedProviderId?.trim() || null;
  }
  return null;
}

export async function importLocalTokenToAgent(
  input: {
    profile: ImportProfile;
    agentId: AgentKey;
    localToken?: string | null;
    /** Pre-bind siblings; refreshed profiles are preferred after bind. */
    siblingProfiles?: readonly AdapterProfile[];
  },
  deps: ImportLocalTokenDeps = defaultDeps,
): Promise<void> {
  let generated = generatedForTarget(
    input.siblingProfiles ?? [input.profile],
    input.profile,
    input.agentId,
  );

  if (!generated) {
    const ticketId = ticketIdFor(input.profile.sourceKind, input.profile.sourceId);
    await deps.planTicket(ticketId, input.agentId);
    const { binding } = await deps.bindTicket(ticketId, input.agentId);
    try {
      const profiles = await deps.listAdapterProfiles();
      const fromBinding = binding.profileId
        ? profiles.find((row) => row.id === binding.profileId)?.generatedProviderId?.trim()
        : undefined;
      generated = fromBinding || generatedForTarget(profiles, input.profile, input.agentId);
    } catch {
      generated = generatedForTarget(
        input.siblingProfiles ?? [input.profile],
        input.profile,
        input.agentId,
      );
    }
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
